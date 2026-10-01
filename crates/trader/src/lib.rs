//! Copy-trade executor.
//!
//! Default behaviour: every signal is recorded as a dry-run trade (what we
//! would have bought, at what price, sized by `sol_per_trade`) and appended
//! to `trades.jsonl`. With `dry_run_quote` on, pump.fun / PumpSwap signals
//! additionally get a live curve/pool quote in the logs.
//!
//! With `LIVE_TRADING=true` and `SOLAMI_TRADER_KEYPAIR` set, supported
//! signals (pump.fun bonding curves, PumpSwap WSOL pools) are executed for
//! real: swap instructions are built from on-chain state and landed through
//! Solami Beam. Failures are recorded as `live_error` and never stop the loop.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use solana_signer::Signer;
use tokio::sync::broadcast;
use tracing::{info, warn};

use shadow_core::{now_unix, Signal, TradeRecord};

pub mod swap;

const MAX_TRADE_RECORDS: usize = 500;
const HEALTH_TRANSFER_LAMPORTS: u64 = 1000;
const TIP_SOL: f64 = 0.0001;
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const COMPUTE_UNIT_LIMIT: u32 = 400_000;
const QUOTE_CACHE_TTL_SECS: i64 = 60;

#[derive(Debug, Clone)]
pub struct TraderConfig {
    pub live: bool,
    pub sol_per_trade: f64,
    pub max_daily_sol: f64,
    pub keypair_b58: Option<String>,
    pub beam_health_check: bool,
    pub api_key: String,
    pub log_path: String,
    pub slippage_bps: u64,
    pub priority_fee_microlamports: u64,
    pub dry_run_quote: bool,
}

pub struct TradeStore {
    pub records: RwLock<VecDeque<Arc<TradeRecord>>>,
    pub tx: broadcast::Sender<Arc<TradeRecord>>,
    spent: RwLock<(i64, f64)>,
}

impl Default for TradeStore {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(256);
        TradeStore {
            records: RwLock::new(VecDeque::new()),
            tx,
            spent: RwLock::new((now_unix() / 86_400, 0.0)),
        }
    }
}

impl TradeStore {
    fn record(&self, rec: TradeRecord) {
        let rec = Arc::new(rec);
        let mut records = self.records.write().unwrap();
        records.push_front(rec.clone());
        while records.len() > MAX_TRADE_RECORDS {
            records.pop_back();
        }
        drop(records);
        let _ = self.tx.send(rec);
    }

    fn spend(&self, sol: f64) {
        let today = now_unix() / 86_400;
        let mut spent = self.spent.write().unwrap();
        if spent.0 != today {
            *spent = (today, 0.0);
        }
        spent.1 += sol;
    }

    fn remaining_today(&self, max_daily: f64) -> f64 {
        let today = now_unix() / 86_400;
        let spent = self.spent.read().unwrap();
        let used = if spent.0 == today { spent.1 } else { 0.0 };
        (max_daily - used).max(0.0)
    }
}

type LiveClient = solami::Solami<solami::On<solami::RpcKit>, solami::Off, solami::On<solami::SwqosClient>>;
type ReadClient = solami::Solami<solami::On<solami::RpcKit>, solami::Off, solami::Off>;

struct LiveExecutor {
    client: LiveClient,
    keypair: solami::Keypair,
}

pub async fn run(
    cfg: TraderConfig,
    mut rx: broadcast::Receiver<Arc<Signal>>,
    store: Arc<TradeStore>,
    beam_latency_ms: Arc<AtomicU64>,
) {
    if cfg.beam_health_check {
        if let Some(kp) = cfg.keypair_b58.clone() {
            let api_key = cfg.api_key.clone();
            let latency = beam_latency_ms.clone();
            tokio::spawn(async move {
                match beam_health_check(&api_key, &kp).await {
                    Ok((sig, ms)) => {
                        latency.store(ms, Ordering::Relaxed);
                        info!(%sig, latency_ms = ms, "beam health check landed");
                    }
                    Err(e) => warn!(error = %e, "beam health check failed"),
                }
            });
        } else {
            warn!("BEAM_HEALTH_CHECK set but SOLAMI_TRADER_KEYPAIR missing");
        }
    }

    let live: Option<LiveExecutor> = if cfg.live {
        match cfg.keypair_b58.clone() {
            None => {
                warn!("LIVE_TRADING=true but SOLAMI_TRADER_KEYPAIR missing; recording dry-run intents");
                None
            }
            Some(kp) => match solami::builder().with_rpc(&cfg.api_key).with_beam(&kp).build().await
            {
                Ok(client) => {
                    let keypair = solami::Keypair::from_base58_string(&kp);
                    info!(payer = %keypair.pubkey(), "live trading enabled via Solami Beam");
                    Some(LiveExecutor { client, keypair })
                }
                Err(e) => {
                    warn!(error = %e, "live client init failed; recording dry-run intents");
                    None
                }
            },
        }
    } else {
        None
    };

    let quote_client: Option<ReadClient> = if live.is_none() && cfg.dry_run_quote {
        match solami::builder().with_rpc(&cfg.api_key).build().await {
            Ok(client) => Some(client),
            Err(e) => {
                warn!(error = %e, "quote client init failed; dry-run quotes disabled");
                None
            }
        }
    } else {
        None
    };

    // mint -> (fetched_at, quote result); failures are cached too so a broken
    // mint does not trigger an RPC call per signal.
    let mut quote_cache: HashMap<String, (i64, Result<(u64, u64), String>)> = HashMap::new();

    loop {
        let sig = match rx.recv().await {
            Ok(s) => s,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(skipped = n, "trader lagged behind engine");
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        };

        let remaining = store.remaining_today(cfg.max_daily_sol);
        if cfg.sol_per_trade > remaining {
            warn!(
                mint = %sig.mint,
                remaining_sol = remaining,
                "daily budget exhausted, skipping signal"
            );
            store.record(TradeRecord {
                signal_id: sig.id.clone(),
                mint: sig.mint.clone(),
                mode: "skipped_budget".into(),
                sol_amount: 0.0,
                signature: None,
                landed_ms: None,
                error: Some(format!("daily budget exhausted ({remaining:.4} SOL left)")),
                at: now_unix(),
            });
            continue;
        }

        let plan = swap::plan_for_dex(&sig.dex);

        if let Some(exec) = &live {
            match plan {
                swap::SwapPlan::Unsupported => {
                    info!(mint = %sig.mint, dex = %sig.dex, "live: dex not supported, skipping");
                    store.record(TradeRecord {
                        signal_id: sig.id.clone(),
                        mint: sig.mint.clone(),
                        mode: "skipped_dex".into(),
                        sol_amount: 0.0,
                        signature: None,
                        landed_ms: None,
                        error: Some(format!("dex {} not supported for live buys", sig.dex)),
                        at: now_unix(),
                    });
                }
                _ => {
                    store.spend(cfg.sol_per_trade);
                    match execute_live(exec, &sig, &cfg).await {
                        Ok((signature, ms)) => {
                            info!(
                                mint = %sig.mint,
                                dex = %sig.dex,
                                sol = cfg.sol_per_trade,
                                %signature,
                                landed_ms = ms,
                                "live: buy landed"
                            );
                            let rec = TradeRecord {
                                signal_id: sig.id.clone(),
                                mint: sig.mint.clone(),
                                mode: "live".into(),
                                sol_amount: cfg.sol_per_trade,
                                signature: Some(signature.to_string()),
                                landed_ms: Some(ms),
                                error: None,
                                at: now_unix(),
                            };
                            append_jsonl(&cfg.log_path, &rec);
                            store.record(rec);
                        }
                        Err(e) => {
                            warn!(mint = %sig.mint, dex = %sig.dex, error = %e, "live: buy failed");
                            let rec = TradeRecord {
                                signal_id: sig.id.clone(),
                                mint: sig.mint.clone(),
                                mode: "live_error".into(),
                                sol_amount: cfg.sol_per_trade,
                                signature: None,
                                landed_ms: None,
                                error: Some(format!("{e:#}")),
                                at: now_unix(),
                            };
                            append_jsonl(&cfg.log_path, &rec);
                            store.record(rec);
                        }
                    }
                }
            }
            continue;
        }

        store.spend(cfg.sol_per_trade);

        let quote = if cfg.dry_run_quote && !matches!(plan, swap::SwapPlan::Unsupported) {
            dry_run_quote(
                quote_client.as_ref().map(|c| &**c as _),
                &sig,
                &cfg,
                &mut quote_cache,
            )
            .await
        } else {
            None
        };

        match quote {
            Some(Ok((expected, min_out))) => info!(
                mint = %sig.mint,
                symbol = ?sig.symbol,
                dex = %sig.dex,
                lamports_in = (cfg.sol_per_trade * LAMPORTS_PER_SOL as f64) as u64,
                expected_tokens = expected,
                min_tokens_out = min_out,
                price_usd = sig.price_usd,
                "dry-run: would buy (quoted)"
            ),
            Some(Err(e)) => info!(
                mint = %sig.mint,
                symbol = ?sig.symbol,
                price_usd = sig.price_usd,
                sol = cfg.sol_per_trade,
                quote_error = %e,
                "dry-run: would buy (quote unavailable)"
            ),
            None => info!(
                mint = %sig.mint,
                symbol = ?sig.symbol,
                price_usd = sig.price_usd,
                sol = cfg.sol_per_trade,
                risk = sig.risk_score,
                trigger = %sig.trigger,
                "dry-run: would buy"
            ),
        }
        let rec = TradeRecord {
            signal_id: sig.id.clone(),
            mint: sig.mint.clone(),
            mode: "dry_run".into(),
            sol_amount: cfg.sol_per_trade,
            signature: None,
            landed_ms: None,
            error: None,
            at: now_unix(),
        };
        append_jsonl(&cfg.log_path, &rec);
        store.record(rec);
    }
}

async fn dry_run_quote(
    rpc: Option<&solana_client::nonblocking::rpc_client::RpcClient>,
    sig: &Signal,
    cfg: &TraderConfig,
    cache: &mut HashMap<String, (i64, Result<(u64, u64), String>)>,
) -> Option<Result<(u64, u64), String>> {
    let rpc = rpc?;
    let now = now_unix();
    if let Some((at, cached)) = cache.get(&sig.mint) {
        if now - at < QUOTE_CACHE_TTL_SECS {
            return Some(cached.clone());
        }
    }
    let mint: solami::Pubkey = sig.mint.parse().ok()?;
    let lamports = (cfg.sol_per_trade * LAMPORTS_PER_SOL as f64) as u64;
    let result = swap::quote_buy(rpc, &mint, &sig.dex, lamports, cfg.slippage_bps)
        .await
        .map_err(|e| format!("{e:#}"));
    cache.insert(sig.mint.clone(), (now, result.clone()));
    Some(result)
}

async fn execute_live(
    exec: &LiveExecutor,
    sig: &Signal,
    cfg: &TraderConfig,
) -> Result<(solami::Signature, u64)> {
    let payer = exec.keypair.pubkey();
    let mint: solami::Pubkey = sig.mint.parse().context("invalid mint pubkey")?;
    let lamports = (cfg.sol_per_trade * LAMPORTS_PER_SOL as f64) as u64;

    let quote = swap::build_buy_ixs(
        &exec.client,
        &payer,
        &mint,
        &sig.dex,
        lamports,
        cfg.slippage_bps,
    )
    .await
    .context("build buy instructions")?;

    let mut ixs = Vec::with_capacity(quote.ixs.len() + 3);
    ixs.push(compute_budget_ix(2, COMPUTE_UNIT_LIMIT as u64));
    ixs.push(compute_budget_ix(3, cfg.priority_fee_microlamports));
    ixs.extend(quote.ixs);
    ixs.push(solami::build_tip_ix(&payer, TIP_SOL));

    let blockhash = exec.client.get_latest_blockhash().await?;
    let tx = solami::Transaction::new_signed_with_payer(&ixs, Some(&payer), &[&exec.keypair], blockhash);
    let vtx = solami::VersionedTransaction::from(tx);
    let t0 = std::time::Instant::now();
    let signature = exec.client.land_transaction(&vtx).await.context("beam send")?;
    Ok((signature, t0.elapsed().as_millis() as u64))
}

/// ComputeBudget instruction: tag 2 = set_compute_unit_limit (u32),
/// tag 3 = set_compute_unit_price (u64 microlamports).
fn compute_budget_ix(tag: u8, value: u64) -> solami::Instruction {
    let data = if tag == 2 {
        let mut d = Vec::with_capacity(5);
        d.push(tag);
        d.extend_from_slice(&(value as u32).to_le_bytes());
        d
    } else {
        let mut d = Vec::with_capacity(9);
        d.push(tag);
        d.extend_from_slice(&value.to_le_bytes());
        d
    };
    solami::Instruction::new_with_bytes(swap::COMPUTE_BUDGET_PROGRAM, &data, vec![])
}

fn append_jsonl(path: &str, rec: &TradeRecord) {
    use std::io::Write;
    let line = match serde_json::to_string(rec) {
        Ok(l) => l,
        Err(_) => return,
    };
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Land a minimal self-transfer through Beam and measure landing latency.
/// Proves the write path works against mainnet before real size goes through.
async fn beam_health_check(api_key: &str, keypair_b58: &str) -> Result<(solami::Signature, u64)> {
    let client = solami::builder()
        .with_rpc(api_key)
        .with_beam(keypair_b58)
        .build()
        .await?;
    let payer = solami::Keypair::from_base58_string(keypair_b58);
    let blockhash = client.get_latest_blockhash().await?;
    let tx = solami::Transaction::new_signed_with_payer(
        &[
            solami::system_instruction::transfer(
                &payer.pubkey(),
                &payer.pubkey(),
                HEALTH_TRANSFER_LAMPORTS,
            ),
            solami::build_tip_ix(&payer.pubkey(), TIP_SOL),
        ],
        Some(&payer.pubkey()),
        &[&payer],
        blockhash,
    );
    let vtx = solami::VersionedTransaction::from(tx);
    let t0 = std::time::Instant::now();
    let sig = client.land_transaction(&vtx).await.context("beam send")?;
    Ok((sig, t0.elapsed().as_millis() as u64))
}
