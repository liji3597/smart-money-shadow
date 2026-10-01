//! Native Yellowstone gRPC wallet tracking: subscribes to on-chain
//! transactions involving the smart-money set and derives `WalletBuy` events
//! from pre/post token-balance deltas. Complements the Blur stream, which
//! only decodes a subset of DEXes.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use tokio::sync::{broadcast, watch};
use tracing::{debug, info, warn};

use shadow_core::WalletBuy;

use crate::StreamHealth;

const FILTER_LABEL: &str = "smart";
/// Wait this long after the last smart-set change before pushing a filter
/// update, so a leaderboard refresh doesn't flap the stream.
const DEBOUNCE: Duration = Duration::from_secs(30);

const WSOL: &str = "So11111111111111111111111111111111111111112";
const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const USDT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";

/// DEX program ids. Order matters: specific venues before the Jupiter
/// aggregator, since a routed transaction touches both.
const DEX_PROGRAMS: &[(&str, &str)] = &[
    ("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P", "pump.fun"),
    ("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA", "pumpswap"),
    ("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8", "raydium"),
    ("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C", "raydium"),
    ("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK", "raydium"),
    ("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc", "orca"),
    ("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo", "meteora"),
    ("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4", "jupiter"),
];

/// Run the wallet-track loop forever, reconnecting with capped exponential
/// backoff. Buys are fanned out on `event_tx`; the subscribed wallet set
/// follows `wallet_rx`.
pub async fn run_wallet_track(
    api_key: String,
    wallet_rx: watch::Receiver<Arc<HashSet<String>>>,
    event_tx: broadcast::Sender<Arc<WalletBuy>>,
    health: StreamHealth,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        match track_once(&api_key, &wallet_rx, &event_tx, &health).await {
            Ok(()) => warn!("wallet-track stream ended by server, reconnecting"),
            Err(e) => {
                let msg = e.to_string();
                // Pro plan: at most 2 concurrent gRPC streams.
                if msg.contains("concurrent streams") {
                    warn!(error = %msg, "wallet-track stream slot busy, retrying in 60s");
                    health.connected.store(false, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }
                warn!(error = %msg, "wallet-track error, reconnecting");
            }
        }
        health.connected.store(false, Ordering::Relaxed);
        let n = health.reconnects.fetch_add(1, Ordering::Relaxed) + 1;
        info!(reconnect = n, backoff_ms = backoff.as_millis() as u64, "wallet-track backoff");
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

async fn track_once(
    api_key: &str,
    wallet_rx: &watch::Receiver<Arc<HashSet<String>>>,
    event_tx: &broadcast::Sender<Arc<WalletBuy>>,
    health: &StreamHealth,
) -> Result<()> {
    let mut rx = wallet_rx.clone();
    while rx.borrow().is_empty() {
        rx.changed().await.context("wallet watch closed")?;
    }
    let mut current: Arc<HashSet<String>> = rx.borrow().clone();

    let mut client = solami::builder().with_grpc(api_key).build().await?;
    let (filter_tx, mut updates) = client
        .grpc()
        .subscribe_transactions(
            FILTER_LABEL,
            current.iter().cloned().collect(),
            solami::CommitmentLevel::Processed,
        )
        .await?;
    health.connected.store(true, Ordering::Relaxed);
    info!(wallets = current.len(), "wallet-track stream connected");

    let mut dirty_at: Option<Instant> = None;
    loop {
        tokio::select! {
            update = updates.next() => {
                let update = match update {
                    Some(u) => u.map_err(|s| anyhow!("grpc status: {s}"))?,
                    None => return Ok(()),
                };
                let Some(solami::GrpcUpdateKind::Transaction(tx)) = update.update_oneof else {
                    continue;
                };
                health.last_slot.store(tx.slot, Ordering::Relaxed);
                for buy in parse_wallet_buys(&tx, &current) {
                    let _ = event_tx.send(Arc::new(buy));
                }
            }
            changed = rx.changed() => {
                changed.context("wallet watch closed")?;
                dirty_at = Some(Instant::now() + DEBOUNCE);
            }
            // select! creates the branch future even when a precondition would
            // disable it, so never unwrap here — park on pending() instead.
            _ = async {
                match dirty_at {
                    Some(t) => tokio::time::sleep_until(t.into()).await,
                    None => std::future::pending().await,
                }
            } => {
                dirty_at = None;
                let next = rx.borrow().clone();
                if next != current && !next.is_empty() {
                    filter_tx
                        .send(filter_request(next.iter().cloned().collect()))
                        .await
                        .map_err(|_| anyhow!("filter channel closed"))?;
                    info!(wallets = next.len(), "wallet-track filter updated");
                    current = next;
                }
            }
        }
    }
}

fn filter_request(accounts: Vec<String>) -> solami::SubscribeRequest {
    solami::SubscriptionBuilder::new()
        .commitment(solami::CommitmentLevel::Processed)
        .transactions(
            FILTER_LABEL,
            solami::TxFilter {
                vote: Some(false),
                failed: Some(false),
                account_include: accounts,
                account_exclude: vec![],
                account_required: vec![],
                signature: None,
            },
        )
        .build()
}

/// A smart wallet "bought" a mint when its token balance for that mint net
/// increases while its quote side (SOL, WSOL, USDC or USDT) net decreases.
fn parse_wallet_buys(
    update: &solami::geyser::SubscribeUpdateTransaction,
    smart: &HashSet<String>,
) -> Vec<WalletBuy> {
    let Some(info) = &update.transaction else {
        return Vec::new();
    };
    let Some(meta) = &info.meta else {
        return Vec::new();
    };
    let Some(msg) = info.transaction.as_ref().and_then(|t| t.message.as_ref()) else {
        debug!(slot = update.slot, "wallet-track: transaction without message");
        return Vec::new();
    };

    let keys: Vec<String> = msg
        .account_keys
        .iter()
        .map(|k| bs58::encode(k).into_string())
        .collect();
    let signature = bs58::encode(&info.signature).into_string();
    let dex = detect_dex(&keys, meta);

    // (mint, owner, pre_raw, post_raw) per token account index.
    let mut accts: HashMap<u32, (String, String, u128, u128)> = HashMap::new();
    for b in &meta.pre_token_balances {
        let e = accts.entry(b.account_index).or_default();
        if !b.mint.is_empty() {
            e.0.clone_from(&b.mint);
        }
        if !b.owner.is_empty() {
            e.1.clone_from(&b.owner);
        }
        e.2 = raw_amount(b);
    }
    for b in &meta.post_token_balances {
        let e = accts.entry(b.account_index).or_default();
        if !b.mint.is_empty() {
            e.0.clone_from(&b.mint);
        }
        if !b.owner.is_empty() {
            e.1.clone_from(&b.owner);
        }
        e.3 = raw_amount(b);
    }

    // Net raw deltas per (wallet, mint).
    let mut by_wallet: HashMap<&str, HashMap<&str, i128>> = HashMap::new();
    for (mint, owner, pre, post) in accts.values() {
        if mint.is_empty() || !smart.contains(owner.as_str()) {
            continue;
        }
        let delta = *post as i128 - *pre as i128;
        *by_wallet
            .entry(owner.as_str())
            .or_default()
            .entry(mint.as_str())
            .or_default() += delta;
    }

    let mut buys = Vec::new();
    for (wallet, deltas) in by_wallet {
        let sol_delta = sol_delta(&keys, meta, wallet);
        let quote_spent = [WSOL, USDC, USDT]
            .iter()
            .any(|q| deltas.get(q).is_some_and(|d| *d < 0));
        if !quote_spent && sol_delta >= 0 {
            continue;
        }
        let wsol_spent = deltas
            .get(WSOL)
            .filter(|d| **d < 0)
            .map(|d| (-*d) as f64 / 1e9)
            .unwrap_or(0.0);
        let sol_spent = (-sol_delta.min(0)) as f64 / 1e9 + wsol_spent;
        for (mint, delta) in deltas {
            if delta <= 0 || mint == WSOL || mint == USDC || mint == USDT {
                continue;
            }
            buys.push(WalletBuy {
                wallet: wallet.to_owned(),
                mint: mint.to_owned(),
                sol_spent,
                slot: update.slot,
                signature: signature.clone(),
                dex: dex.clone(),
            });
        }
    }
    buys
}

fn raw_amount(b: &solami::solana::storage::confirmed_block::TokenBalance) -> u128 {
    b.ui_token_amount
        .as_ref()
        .and_then(|u| u.amount.parse().ok())
        .unwrap_or(0)
}

/// Lamport delta of the wallet's own account (fee included). 0 when the
/// wallet isn't a static account key.
fn sol_delta(
    keys: &[String],
    meta: &solami::solana::storage::confirmed_block::TransactionStatusMeta,
    wallet: &str,
) -> i128 {
    let Some(i) = keys.iter().position(|k| k == wallet) else {
        return 0;
    };
    let pre = meta.pre_balances.get(i).copied().unwrap_or(0) as i128;
    let post = meta.post_balances.get(i).copied().unwrap_or(0) as i128;
    post - pre
}

fn detect_dex(
    keys: &[String],
    meta: &solami::solana::storage::confirmed_block::TransactionStatusMeta,
) -> String {
    let loaded: Vec<String> = meta
        .loaded_writable_addresses
        .iter()
        .chain(&meta.loaded_readonly_addresses)
        .map(|a| bs58::encode(a).into_string())
        .collect();
    for (pid, name) in DEX_PROGRAMS {
        if keys.iter().any(|k| k == pid) || loaded.iter().any(|a| a == pid) {
            return (*name).to_owned();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use solami::geyser::{SubscribeUpdateTransaction, SubscribeUpdateTransactionInfo};
    use solami::solana::storage::confirmed_block::{
        Message, TokenBalance, Transaction, TransactionStatusMeta, UiTokenAmount,
    };

    const WALLET: &str = "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf";
    const MINT: &str = "E3JvmGcGFDzhu2Cnxyeq5BRvN7HH9JZUsfAUh2v8pump";
    const PUMPSWAP_PROGRAM: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";

    fn b58(s: &str) -> Vec<u8> {
        bs58::decode(s).into_vec().unwrap()
    }

    fn token_balance(account_index: u32, mint: &str, owner: &str, amount: u64) -> TokenBalance {
        TokenBalance {
            account_index,
            mint: mint.to_owned(),
            ui_token_amount: Some(UiTokenAmount {
                ui_amount: 0.0,
                decimals: 6,
                amount: amount.to_string(),
                ui_amount_string: String::new(),
            }),
            owner: owner.to_owned(),
            program_id: String::new(),
        }
    }

    /// Transaction fixture: WALLET is account key 0 with the given pre/post
    /// lamport balances; the PumpSwap program is loaded so the DEX is detected.
    fn update(
        pre_tokens: Vec<TokenBalance>,
        post_tokens: Vec<TokenBalance>,
        pre_sol: u64,
        post_sol: u64,
    ) -> SubscribeUpdateTransaction {
        SubscribeUpdateTransaction {
            transaction: Some(SubscribeUpdateTransactionInfo {
                signature: vec![7u8; 64],
                is_vote: false,
                transaction: Some(Transaction {
                    signatures: vec![],
                    message: Some(Message {
                        account_keys: vec![b58(WALLET)],
                        ..Default::default()
                    }),
                }),
                meta: Some(TransactionStatusMeta {
                    pre_balances: vec![pre_sol],
                    post_balances: vec![post_sol],
                    pre_token_balances: pre_tokens,
                    post_token_balances: post_tokens,
                    loaded_writable_addresses: vec![b58(PUMPSWAP_PROGRAM)],
                    ..Default::default()
                }),
                index: 0,
            }),
            slot: 42,
        }
    }

    fn smart_set() -> HashSet<String> {
        HashSet::from([WALLET.to_owned()])
    }

    #[test]
    fn buy_detected_when_tokens_increase_and_sol_decreases() {
        let tx = update(
            vec![],
            vec![token_balance(3, MINT, WALLET, 1_000)],
            1_000_000_000,
            900_000_000,
        );
        let buys = parse_wallet_buys(&tx, &smart_set());
        assert_eq!(buys.len(), 1);
        let buy = &buys[0];
        assert_eq!(buy.wallet, WALLET);
        assert_eq!(buy.mint, MINT);
        assert!((buy.sol_spent - 0.1).abs() < 1e-9, "sol_spent = {}", buy.sol_spent);
        assert_eq!(buy.slot, 42);
        assert_eq!(buy.dex, "pumpswap");
        assert_eq!(buy.signature, bs58::encode([7u8; 64]).into_string());
    }

    #[test]
    fn buy_detected_when_wsol_decreases() {
        // Quote spent via WSOL token account instead of native SOL.
        let tx = update(
            vec![token_balance(2, WSOL, WALLET, 500_000_000)],
            vec![
                token_balance(2, WSOL, WALLET, 0),
                token_balance(3, MINT, WALLET, 1_000),
            ],
            1_000_000_000,
            1_000_000_000,
        );
        let buys = parse_wallet_buys(&tx, &smart_set());
        assert_eq!(buys.len(), 1);
        assert!((buys[0].sol_spent - 0.5).abs() < 1e-9, "sol_spent = {}", buys[0].sol_spent);
    }

    #[test]
    fn airdrop_is_not_a_buy() {
        // Tokens arrive but no SOL/quote leaves the wallet.
        let tx = update(
            vec![],
            vec![token_balance(3, MINT, WALLET, 1_000)],
            1_000_000_000,
            1_000_000_000,
        );
        assert!(parse_wallet_buys(&tx, &smart_set()).is_empty());
    }

    #[test]
    fn sell_is_not_a_buy() {
        let tx = update(
            vec![token_balance(3, MINT, WALLET, 1_000)],
            vec![token_balance(3, MINT, WALLET, 0)],
            1_000_000_000,
            1_100_000_000,
        );
        assert!(parse_wallet_buys(&tx, &smart_set()).is_empty());
    }

    #[test]
    fn balances_of_other_wallets_are_ignored() {
        let tx = update(
            vec![],
            vec![token_balance(3, MINT, "someone-else", 1_000)],
            1_000_000_000,
            900_000_000,
        );
        assert!(parse_wallet_buys(&tx, &smart_set()).is_empty());
    }

    #[test]
    fn missing_meta_or_transaction_yields_no_buys() {
        let mut tx = update(vec![], vec![token_balance(3, MINT, WALLET, 1_000)], 1_000_000_000, 0);
        tx.transaction.as_mut().unwrap().meta = None;
        assert!(parse_wallet_buys(&tx, &smart_set()).is_empty());

        let empty = SubscribeUpdateTransaction { transaction: None, slot: 1 };
        assert!(parse_wallet_buys(&empty, &smart_set()).is_empty());
    }

    #[test]
    fn raw_amount_parses_string_amount() {
        assert_eq!(raw_amount(&token_balance(0, MINT, WALLET, 12_345)), 12_345);
        let mut b = token_balance(0, MINT, WALLET, 1);
        b.ui_token_amount = None;
        assert_eq!(raw_amount(&b), 0);
        let mut b = token_balance(0, MINT, WALLET, 1);
        b.ui_token_amount.as_mut().unwrap().amount = "not-a-number".to_owned();
        assert_eq!(raw_amount(&b), 0);
    }

    #[test]
    fn detect_dex_prefers_specific_venue_over_jupiter() {
        let meta = TransactionStatusMeta {
            loaded_writable_addresses: vec![b58(PUMPSWAP_PROGRAM)],
            ..Default::default()
        };
        let keys = vec!["JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_owned()];
        assert_eq!(detect_dex(&keys, &meta), "pumpswap");
        assert_eq!(detect_dex(&[], &TransactionStatusMeta::default()), "");
    }
}
