//! Copy-trade executor with a position book and exit rules.
//!
//! Default behaviour: every signal is recorded as a dry-run trade (what we
//! would have bought, at what price, sized by `sol_per_trade`) and appended
//! to `trades.jsonl`. With `dry_run_quote` on, pump.fun / PumpSwap signals
//! additionally get a live curve/pool quote in the logs. Buys open a
//! `Position`; the exit loop checks open positions every
//! `position_check_secs` against the Blur price feed and closes them on
//! take-profit / stop-loss / time-stop — in dry-run the exit is simulated so
//! win-rate statistics close the loop.
//!
//! With `LIVE_TRADING=true` and `SOLAMI_TRADER_KEYPAIR` set, supported
//! signals (pump.fun bonding curves, PumpSwap WSOL pools) are executed for
//! real: swap instructions are built from on-chain state and landed through
//! Solami Beam. Failures are recorded as `live_error` and never stop the loop.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use solana_signer::Signer;
use tokio::sync::broadcast;
use tracing::{info, warn};

use shadow_core::{now_unix, Signal, TradeRecord};
use shadow_ingest::BlurRest;

use position::{Position, PositionStore};

pub mod position;
pub mod swap;

const MAX_TRADE_RECORDS: usize = 500;
const HEALTH_TRANSFER_LAMPORTS: u64 = 1000;
const TIP_SOL: f64 = 0.0001;
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const COMPUTE_UNIT_LIMIT: u32 = 400_000;
const QUOTE_CACHE_TTL_SECS: i64 = 60;
const MAX_PRICE_FAILURES: u32 = 10;
const MAX_SELL_ATTEMPTS: u32 = 5;
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);
const RESEND_EVERY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct TraderConfig {
    pub live: bool,
    pub sol_per_trade: f64,
    pub max_daily_sol: f64,
    pub keypair_b58: Option<String>,
    /// Beam-over-QUIC auth identity (a registered swQoS key). Falls back to
    /// `keypair_b58` when unset.
    pub swqos_keypair_b58: Option<String>,
    pub beam_health_check: bool,
    pub api_key: String,
    pub log_path: String,
    pub slippage_bps: u64,
    pub priority_fee_microlamports: u64,
    pub dry_run_quote: bool,
    pub take_profit_pct: f64,
    pub stop_loss_pct: f64,
    pub max_hold_secs: i64,
    pub position_check_secs: i64,
    pub blur_rest_base: String,
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

type LiveClient =
    solami::Solami<solami::On<solami::RpcKit>, solami::Off, solami::On<solami::SwqosClient>>;
type ReadClient = solami::Solami<solami::On<solami::RpcKit>, solami::Off, solami::Off>;
/// mint -> (fetched_at, quote result); failures are cached too.
type QuoteCache = HashMap<String, (i64, Result<(u64, u64), String>)>;

struct LiveExecutor {
    client: LiveClient,
    keypair: solami::Keypair,
}

pub async fn run(
    cfg: TraderConfig,
    mut rx: broadcast::Receiver<Arc<Signal>>,
    store: Arc<TradeStore>,
    beam_latency_ms: Arc<AtomicU64>,
    wallet_balance: Arc<AtomicU64>,
    positions: Arc<PositionStore>,
) {
    // Restore persisted open positions before the exit loop starts, so a
    // restart resumes tracking (and live-selling) tokens already in the wallet.
    positions.restore();

    // Beam authenticates over QUIC with a registered swQoS key, which is a
    // separate identity from the payer wallet. Falling back to the payer only
    // works if the payer itself is registered as a swQoS key.
    let beam_key = match (&cfg.swqos_keypair_b58, &cfg.keypair_b58) {
        (Some(s), _) => Some(s.clone()),
        (None, Some(k)) => {
            warn!("SOLAMI_SWQOS_KEYPAIR missing; using payer keypair for Beam auth");
            Some(k.clone())
        }
        (None, None) => None,
    };

    if cfg.beam_health_check {
        match (cfg.keypair_b58.clone(), beam_key.clone()) {
            (Some(kp), Some(beam)) => {
                let api_key = cfg.api_key.clone();
                let latency = beam_latency_ms.clone();
                tokio::spawn(async move {
                    match beam_health_check(&api_key, &beam, &kp).await {
                        Ok((sig, ms)) => {
                            latency.store(ms, Ordering::Relaxed);
                            info!(%sig, latency_ms = ms, "beam health check landed");
                        }
                        Err(e) => warn!(error = %e, "beam health check failed"),
                    }
                });
            }
            _ => warn!("BEAM_HEALTH_CHECK set but SOLAMI_TRADER_KEYPAIR missing"),
        }
    }

    // Wallet balance poller: whenever a payer keypair is configured (live or
    // dry-run), report its SOL balance to the dashboard every 30s.
    if let Some(kp) = cfg.keypair_b58.clone() {
        let api_key = cfg.api_key.clone();
        tokio::spawn(async move {
            let client: ReadClient = match solami::builder().with_rpc(&api_key).build().await {
                Ok(c) => c,
                Err(e) => {
                    warn!(error = %e, "wallet balance client init failed");
                    return;
                }
            };
            let payer = solami::Keypair::from_base58_string(&kp).pubkey();
            let mut warned = false;
            loop {
                match client.get_balance(&payer).await {
                    Ok(lamports) => {
                        wallet_balance.store(lamports, Ordering::Relaxed);
                        warned = false;
                    }
                    Err(e) => {
                        if !warned {
                            warn!(error = %e, "wallet balance poll failed");
                            warned = true;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
    }

    let live: Option<Arc<LiveExecutor>> = if cfg.live {
        match (cfg.keypair_b58.clone(), beam_key.clone()) {
            (None, _) => {
                warn!(
                    "LIVE_TRADING=true but SOLAMI_TRADER_KEYPAIR missing; recording dry-run intents"
                );
                None
            }
            (Some(kp), Some(beam)) => {
                match solami::builder().with_rpc(&cfg.api_key).with_beam(&beam).build().await {
                    Ok(client) => {
                        let keypair = solami::Keypair::from_base58_string(&kp);
                        info!(payer = %keypair.pubkey(), "live trading enabled via Solami Beam");
                        Some(Arc::new(LiveExecutor { client, keypair }))
                    }
                    Err(e) => {
                        warn!(error = %e, "live client init failed; recording dry-run intents");
                        None
                    }
                }
            }
            // beam_key is only None when the payer keypair is None, which the
            // first arm already covers.
            (Some(_), None) => unreachable!(),
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

    {
        let cfg = cfg.clone();
        let positions = positions.clone();
        let store = store.clone();
        let live = live.clone();
        tokio::spawn(async move { exit_loop(cfg, positions, store, live).await });
    }

    // mint -> (fetched_at, quote result); failures are cached too so a broken
    // mint does not trigger an RPC call per signal.
    let mut quote_cache: QuoteCache = HashMap::new();

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
                realized_pnl_sol: None,
                reason: None,
                at: now_unix(),
            });
            continue;
        }

        // one position per mint: a fresh signal for a held mint is ignored
        if positions.is_open(&sig.mint) {
            info!(mint = %sig.mint, "already holding this mint, skipping signal");
            store.record(TradeRecord {
                signal_id: sig.id.clone(),
                mint: sig.mint.clone(),
                mode: "skipped_position".into(),
                sol_amount: 0.0,
                signature: None,
                landed_ms: None,
                error: Some("position already open for this mint".into()),
                realized_pnl_sol: None,
                reason: None,
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
                        realized_pnl_sol: None,
                        reason: None,
                        at: now_unix(),
                    });
                }
                _ => {
                    store.spend(cfg.sol_per_trade);
                    match execute_live(exec, &sig, &cfg).await {
                        Ok((signature, ms, expected_tokens)) => {
                            info!(
                                mint = %sig.mint,
                                dex = %sig.dex,
                                sol = cfg.sol_per_trade,
                                %signature,
                                landed_ms = ms,
                                "live: buy landed"
                            );
                            let payer = exec.keypair.pubkey();
                            let mint_pk: solami::Pubkey = sig.mint.parse().unwrap_or_default();
                            let tokens = swap::ata_balance(&exec.client, &payer, &mint_pk)
                                .await
                                .unwrap_or(expected_tokens);
                            positions.try_open(Position {
                                signal_id: sig.id.clone(),
                                mint: sig.mint.clone(),
                                symbol: sig.symbol.clone(),
                                dex: sig.dex.clone(),
                                entry_price_usd: sig.price_usd,
                                sol_in: cfg.sol_per_trade,
                                tokens,
                                opened_at: now_unix(),
                                last_price_usd: None,
                                status: position::PositionStatus::Open,
                                price_failures: 0,
                                sell_attempts: 0,
                            });
                            let rec = TradeRecord {
                                signal_id: sig.id.clone(),
                                mint: sig.mint.clone(),
                                mode: "live".into(),
                                sol_amount: cfg.sol_per_trade,
                                signature: Some(signature.to_string()),
                                landed_ms: Some(ms),
                                error: None,
                                realized_pnl_sol: None,
                                reason: None,
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
                                realized_pnl_sol: None,
                                reason: None,
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

        match &quote {
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
        // quoted tokens when available; 0 means "PnL by price ratio"
        let tokens = quote.and_then(|q| q.ok()).map(|(expected, _)| expected).unwrap_or(0);
        positions.try_open(Position {
            signal_id: sig.id.clone(),
            mint: sig.mint.clone(),
            symbol: sig.symbol.clone(),
            dex: sig.dex.clone(),
            entry_price_usd: sig.price_usd,
            sol_in: cfg.sol_per_trade,
            tokens,
            opened_at: now_unix(),
            last_price_usd: None,
            status: position::PositionStatus::Open,
            price_failures: 0,
            sell_attempts: 0,
        });
        let rec = TradeRecord {
            signal_id: sig.id.clone(),
            mint: sig.mint.clone(),
            mode: "dry_run".into(),
            sol_amount: cfg.sol_per_trade,
            signature: None,
            landed_ms: None,
            error: None,
            realized_pnl_sol: None,
            reason: None,
            at: now_unix(),
        };
        append_jsonl(&cfg.log_path, &rec);
        store.record(rec);
    }
}

// ---- exit loop -------------------------------------------------------------

async fn exit_loop(
    cfg: TraderConfig,
    positions: Arc<PositionStore>,
    store: Arc<TradeStore>,
    live: Option<Arc<LiveExecutor>>,
) {
    let rest = BlurRest::new(cfg.api_key.clone(), Some(cfg.blur_rest_base.clone()));
    let mut price_cache: HashMap<String, (i64, Option<f64>)> = HashMap::new();
    let mut interval =
        tokio::time::interval(Duration::from_secs(cfg.position_check_secs.max(5) as u64));
    loop {
        interval.tick().await;
        let now = now_unix();
        for pos in positions.open_snapshot() {
            match cached_price(&rest, &pos.mint, &mut price_cache).await {
                Some(price) if price > 0.0 => {
                    positions.note_price(&pos.mint, price);
                    let pnl_pct = (price / pos.entry_price_usd - 1.0) * 100.0;
                    let age = now - pos.opened_at;
                    let reason = if pnl_pct >= cfg.take_profit_pct {
                        Some("take_profit")
                    } else if pnl_pct <= cfg.stop_loss_pct {
                        Some("stop_loss")
                    } else if age >= cfg.max_hold_secs {
                        Some("time_stop")
                    } else {
                        None
                    };
                    if let Some(reason) = reason {
                        close_position(&cfg, &positions, &store, live.as_deref(), &pos, reason, price)
                            .await;
                    }
                }
                _ => {
                    let failures = positions.bump_price_failure(&pos.mint);
                    if failures >= MAX_PRICE_FAILURES {
                        warn!(mint = %pos.mint, "price unknown for 10 rounds, abandoning position");
                        positions.close(&pos.mint, "price_unknown", None, None);
                    }
                }
            }
        }
    }
}

async fn cached_price(
    rest: &BlurRest,
    mint: &str,
    cache: &mut HashMap<String, (i64, Option<f64>)>,
) -> Option<f64> {
    let now = now_unix();
    if let Some((at, price)) = cache.get(mint) {
        if now - *at < QUOTE_CACHE_TTL_SECS {
            return *price;
        }
    }
    let price = rest.token_price_usd(mint).await.ok().flatten();
    cache.insert(mint.to_owned(), (now, price));
    price
}

async fn close_position(
    cfg: &TraderConfig,
    positions: &Arc<PositionStore>,
    store: &Arc<TradeStore>,
    live: Option<&LiveExecutor>,
    pos: &Position,
    reason: &str,
    exit_price_usd: f64,
) {
    // PnL basis: price ratio between entry and exit (works with tokens=0 too);
    // live mode replaces it with the quoted sell output, fees included.
    let pnl_sol = pos.sol_in * (exit_price_usd / pos.entry_price_usd - 1.0);

    if let Some(exec) = live {
        match execute_sell(exec, pos, cfg).await {
            Ok((signature, ms, lamports_out)) => {
                let live_pnl = lamports_out as f64 / LAMPORTS_PER_SOL as f64 - pos.sol_in;
                info!(
                    mint = %pos.mint,
                    reason,
                    pnl_sol = live_pnl,
                    %signature,
                    landed_ms = ms,
                    "live: sell landed"
                );
                let rec = TradeRecord {
                    signal_id: pos.signal_id.clone(),
                    mint: pos.mint.clone(),
                    mode: "live_sell".into(),
                    sol_amount: lamports_out as f64 / LAMPORTS_PER_SOL as f64,
                    signature: Some(signature.to_string()),
                    landed_ms: Some(ms),
                    error: None,
                    realized_pnl_sol: Some(live_pnl),
                    reason: Some(reason.into()),
                    at: now_unix(),
                };
                append_jsonl(&cfg.log_path, &rec);
                store.record(rec);
                positions.close(&pos.mint, reason, Some(exit_price_usd), Some(live_pnl));
            }
            Err(e) => {
                warn!(mint = %pos.mint, reason, error = %e, "live: sell failed");
                let rec = TradeRecord {
                    signal_id: pos.signal_id.clone(),
                    mint: pos.mint.clone(),
                    mode: "live_error".into(),
                    sol_amount: 0.0,
                    signature: None,
                    landed_ms: None,
                    error: Some(format!("{e:#}")),
                    realized_pnl_sol: None,
                    reason: Some(reason.into()),
                    at: now_unix(),
                };
                append_jsonl(&cfg.log_path, &rec);
                store.record(rec);
                let attempts = positions.bump_sell_attempt(&pos.mint);
                if attempts >= MAX_SELL_ATTEMPTS {
                    warn!(mint = %pos.mint, "sell failed 5 times, abandoning position");
                    positions.close(&pos.mint, "sell_failed", Some(exit_price_usd), None);
                }
            }
        }
        return;
    }

    info!(
        mint = %pos.mint,
        reason,
        pnl_sol,
        entry = pos.entry_price_usd,
        exit = exit_price_usd,
        "dry-run: would sell"
    );
    let rec = TradeRecord {
        signal_id: pos.signal_id.clone(),
        mint: pos.mint.clone(),
        mode: "dry_sell".into(),
        sol_amount: pos.sol_in + pnl_sol,
        signature: None,
        landed_ms: None,
        error: None,
        realized_pnl_sol: Some(pnl_sol),
        reason: Some(reason.into()),
        at: now_unix(),
    };
    append_jsonl(&cfg.log_path, &rec);
    store.record(rec);
    positions.close(&pos.mint, reason, Some(exit_price_usd), Some(pnl_sol));
}

async fn execute_sell(
    exec: &LiveExecutor,
    pos: &Position,
    cfg: &TraderConfig,
) -> Result<(solami::Signature, u64, u64)> {
    let payer = exec.keypair.pubkey();
    let mint: solami::Pubkey = pos.mint.parse().context("invalid mint pubkey")?;

    let quote = swap::build_sell_ixs(&exec.client, &payer, &mint, &pos.dex, cfg.slippage_bps)
        .await
        .context("build sell instructions")?;

    let mut ixs = Vec::with_capacity(quote.ixs.len() + 3);
    ixs.push(compute_budget_ix(2, COMPUTE_UNIT_LIMIT as u64));
    ixs.push(compute_budget_ix(3, cfg.priority_fee_microlamports));
    ixs.extend_from_slice(&quote.ixs);
    ixs.push(solami::build_tip_ix(&payer, TIP_SOL));

    let blockhash = exec.client.get_latest_blockhash().await?;
    let tx =
        solami::Transaction::new_signed_with_payer(&ixs, Some(&payer), &[&exec.keypair], blockhash);
    let vtx = solami::VersionedTransaction::from(tx);
    let (signature, ms) = beam_send_and_confirm(&exec.client, &vtx).await?;
    Ok((signature, ms, quote.expected_lamports_out))
}

async fn dry_run_quote(
    rpc: Option<&solana_client::nonblocking::rpc_client::RpcClient>,
    sig: &Signal,
    cfg: &TraderConfig,
    cache: &mut QuoteCache,
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
) -> Result<(solami::Signature, u64, u64)> {
    let payer = exec.keypair.pubkey();
    let mint: solami::Pubkey = sig.mint.parse().context("invalid mint pubkey")?;
    let lamports = (cfg.sol_per_trade * LAMPORTS_PER_SOL as f64) as u64;

    let quote = swap::build_buy_ixs(&exec.client, &payer, &mint, &sig.dex, lamports, cfg.slippage_bps)
        .await
        .context("build buy instructions")?;

    let mut ixs = Vec::with_capacity(quote.ixs.len() + 3);
    ixs.push(compute_budget_ix(2, COMPUTE_UNIT_LIMIT as u64));
    ixs.push(compute_budget_ix(3, cfg.priority_fee_microlamports));
    ixs.extend_from_slice(&quote.ixs);
    ixs.push(solami::build_tip_ix(&payer, TIP_SOL));

    let blockhash = exec.client.get_latest_blockhash().await?;
    let tx =
        solami::Transaction::new_signed_with_payer(&ixs, Some(&payer), &[&exec.keypair], blockhash);
    let vtx = solami::VersionedTransaction::from(tx);
    let (signature, ms) = beam_send_and_confirm(&exec.client, &vtx).await?;
    Ok((signature, ms, quote.expected_tokens))
}

/// Send a transaction through Beam and wait until it confirms on-chain.
///
/// `land_transaction` only means the relay accepted the bytes over QUIC — it
/// says nothing about inclusion (our first health check "landed" yet never
/// appeared on-chain). So we poll the signature status over RPC, resending
/// the same transaction every couple of seconds until it confirms, fails,
/// or times out.
async fn beam_send_and_confirm(
    client: &LiveClient,
    tx: &solami::VersionedTransaction,
) -> Result<(solami::Signature, u64)> {
    let t0 = std::time::Instant::now();
    let sig = client.land_transaction(tx).await.context("beam send")?;
    let mut last_resend = t0;
    loop {
        match client.get_signature_status(&sig).await {
            Ok(Some(Ok(()))) => return Ok((sig, t0.elapsed().as_millis() as u64)),
            Ok(Some(Err(e))) => anyhow::bail!("tx landed but failed: {e}"),
            Ok(None) | Err(_) => {
                if t0.elapsed() > CONFIRM_TIMEOUT {
                    anyhow::bail!("tx not confirmed within {CONFIRM_TIMEOUT:?}");
                }
                if last_resend.elapsed() >= RESEND_EVERY {
                    let _ = client.land_transaction(tx).await;
                    last_resend = std::time::Instant::now();
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        }
    }
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
async fn beam_health_check(
    api_key: &str,
    beam_key_b58: &str,
    keypair_b58: &str,
) -> Result<(solami::Signature, u64)> {
    let client = solami::builder()
        .with_rpc(api_key)
        .with_beam(beam_key_b58)
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
    beam_send_and_confirm(&client, &vtx).await
}
