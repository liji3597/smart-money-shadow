//! Signal PnL backfill: subscribes to the signal broadcast, periodically
//! re-prices signal mints via the Blur REST `token/price` endpoint, and
//! aggregates win-rate statistics for `/api/performance`.
//!
//! PnL data lives in a sidecar map keyed by signal id and is merged into
//! signal JSON at the route layer, so `shadow_core::Signal` stays untouched.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::broadcast;
use tracing::{info, warn};

use shadow_core::{now_unix, Signal};
use shadow_ingest::BlurRest;

const ROUND_INTERVAL_SECS: u64 = 60;
const MAX_QUERIES_PER_ROUND: usize = 20;
const PRICE_CACHE_SECS: i64 = 60;
const PRICE_CACHE_PRUNE_SECS: i64 = 300;
const MAX_MINT_FAILURES: u32 = 10;
const HISTORY_CAP: usize = 5000;
const H1: i64 = 3_600;
const H24: i64 = 86_400;

/// PnL fields merged into a signal's JSON at the route layer.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SignalPnl {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_price_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_1h_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_24h_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerfPeak {
    pub signal_id: String,
    pub mint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub pnl_pct: f64,
    /// "1h" or "24h" — which window the figure comes from.
    pub window: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerfStats {
    pub total_signals: usize,
    pub pending_signals: usize,
    pub measured_1h: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate_1h_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_pnl_1h_pct: Option<f64>,
    pub measured_24h: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate_24h_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_pnl_24h_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_signal: Option<PerfPeak>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worst_signal: Option<PerfPeak>,
}

struct Tracked {
    mint: String,
    symbol: Option<String>,
    signal_price: f64,
    created_at: i64,
    pnl_1h_pct: Option<f64>,
    pnl_24h_pct: Option<f64>,
}

/// A signal that aged past 24h; kept (capped) so aggregates survive removal
/// from the tracking list.
struct Done {
    signal_id: String,
    mint: String,
    symbol: Option<String>,
    pnl_1h_pct: Option<f64>,
    pnl_24h_pct: Option<f64>,
}

#[derive(Default)]
struct Inner {
    tracked: HashMap<String, Tracked>,
    done: VecDeque<Done>,
    /// signal id -> mergeable PnL fields (covers tracked and done).
    pnl_by_signal: HashMap<String, SignalPnl>,
    /// mint -> (fetched_at, price_usd).
    price_cache: HashMap<String, (i64, f64)>,
    /// mint -> consecutive fetch failures.
    mint_failures: HashMap<String, u32>,
}

#[derive(Clone, Default)]
pub struct PnlTracker {
    inner: Arc<RwLock<Inner>>,
}

impl PnlTracker {
    pub fn pnl_for(&self, signal_id: &str) -> Option<SignalPnl> {
        self.inner.read().unwrap().pnl_by_signal.get(signal_id).cloned()
    }

    fn track(&self, s: &Signal) {
        if s.price_usd <= 0.0 {
            warn!(signal = %s.id, mint = %s.mint, "signal with non-positive price, not tracking pnl");
            return;
        }
        let mut inner = self.inner.write().unwrap();
        inner.tracked.insert(
            s.id.clone(),
            Tracked {
                mint: s.mint.clone(),
                symbol: s.symbol.clone(),
                signal_price: s.price_usd,
                created_at: s.created_at,
                pnl_1h_pct: None,
                pnl_24h_pct: None,
            },
        );
        inner.pnl_by_signal.entry(s.id.clone()).or_default();
    }

    /// One backfill round: pick up to MAX_QUERIES_PER_ROUND mints that need a
    /// fresh price, fetch them sequentially, then resolve pending windows.
    async fn backfill_round(&self, rest: &BlurRest) {
        let now = now_unix();
        let mints: Vec<String> = {
            let inner = self.inner.read().unwrap();
            let mut out: Vec<String> = Vec::new();
            for t in inner.tracked.values() {
                let age = now - t.created_at;
                let needs_1h = age >= H1 && t.pnl_1h_pct.is_none();
                let needs_24h = age >= H24 && t.pnl_24h_pct.is_none();
                if !(needs_1h || needs_24h) {
                    continue;
                }
                if inner.mint_failures.get(&t.mint).copied().unwrap_or(0) >= MAX_MINT_FAILURES {
                    continue;
                }
                let fresh = matches!(
                    inner.price_cache.get(&t.mint),
                    Some((at, _)) if now - *at < PRICE_CACHE_SECS
                );
                if !fresh && !out.contains(&t.mint) {
                    out.push(t.mint.clone());
                }
                if out.len() >= MAX_QUERIES_PER_ROUND {
                    break;
                }
            }
            out
        };

        for mint in &mints {
            match rest.token_price_usd(mint).await {
                Ok(Some(p)) if p > 0.0 => {
                    let mut inner = self.inner.write().unwrap();
                    inner.price_cache.insert(mint.clone(), (now_unix(), p));
                    inner.mint_failures.remove(mint);
                }
                Ok(_) => self.record_failure(mint, "empty/non-positive price"),
                Err(e) => self.record_failure(mint, &format!("{e:#}")),
            }
        }

        self.resolve(now);
    }

    fn record_failure(&self, mint: &str, why: &str) {
        let mut inner = self.inner.write().unwrap();
        let n = inner.mint_failures.entry(mint.to_owned()).or_insert(0);
        *n += 1;
        if *n >= MAX_MINT_FAILURES {
            warn!(mint, failures = *n, "giving up on mint price");
        } else {
            warn!(mint, failures = *n, error = why, "price fetch failed");
        }
    }

    /// Apply cached prices to pending windows and retire finished signals.
    fn resolve(&self, now: i64) {
        let mut guard = self.inner.write().unwrap();
        let Inner {
            tracked,
            done,
            pnl_by_signal,
            price_cache,
            mint_failures,
        } = &mut *guard;
        let mut finished: Vec<String> = Vec::new();
        for (id, t) in tracked.iter_mut() {
            let age = now - t.created_at;
            let price = price_cache.get(&t.mint).map(|(_, p)| *p);
            let gave_up = mint_failures.get(&t.mint).copied().unwrap_or(0) >= MAX_MINT_FAILURES;
            let pnl = price.map(|p| (p - t.signal_price) / t.signal_price * 100.0);
            if age >= H1 && t.pnl_1h_pct.is_none() {
                t.pnl_1h_pct = pnl;
            }
            if age >= H24 && t.pnl_24h_pct.is_none() {
                t.pnl_24h_pct = pnl;
            }
            let entry = pnl_by_signal.entry(id.clone()).or_default();
            entry.current_price_usd = price;
            entry.pnl_1h_pct = t.pnl_1h_pct;
            entry.pnl_24h_pct = t.pnl_24h_pct;
            // Retire once past 24h with both windows settled (measured or the
            // mint given up on).
            if age >= H24 && (t.pnl_24h_pct.is_some() || gave_up) {
                finished.push(id.clone());
            }
        }
        for id in finished {
            if let Some(t) = tracked.remove(&id) {
                done.push_front(Done {
                    signal_id: id,
                    mint: t.mint,
                    symbol: t.symbol,
                    pnl_1h_pct: t.pnl_1h_pct,
                    pnl_24h_pct: t.pnl_24h_pct,
                });
            }
        }
        while done.len() > HISTORY_CAP {
            done.pop_back();
        }
        // Prune stale cache entries and failure counters no longer referenced.
        let tracked_mints: std::collections::HashSet<&str> =
            tracked.values().map(|t| t.mint.as_str()).collect();
        price_cache.retain(|_, (at, _)| now - *at < PRICE_CACHE_PRUNE_SECS);
        mint_failures.retain(|m, _| tracked_mints.contains(m.as_str()));
    }

    pub fn stats(&self) -> PerfStats {
        let inner = self.inner.read().unwrap();
        let mut p1h: Vec<f64> = Vec::new();
        let mut p24h: Vec<f64> = Vec::new();
        let mut best: Option<PerfPeak> = None;
        let mut worst: Option<PerfPeak> = None;
        let mut consider = |id: &str, mint: &str, symbol: &Option<String>, p1: Option<f64>, p24: Option<f64>| {
            if let Some(p) = p1 {
                p1h.push(p);
            }
            if let Some(p) = p24 {
                p24h.push(p);
            }
            // Rank by the most mature window available.
            let (pnl, window) = match (p24, p1) {
                (Some(p), _) => (p, "24h"),
                (None, Some(p)) => (p, "1h"),
                (None, None) => return,
            };
            let peak = PerfPeak {
                signal_id: id.to_owned(),
                mint: mint.to_owned(),
                symbol: symbol.clone(),
                pnl_pct: pnl,
                window: window.to_owned(),
            };
            if best.as_ref().is_none_or(|b| pnl > b.pnl_pct) {
                best = Some(peak.clone());
            }
            if worst.as_ref().is_none_or(|w| pnl < w.pnl_pct) {
                worst = Some(peak);
            }
        };
        for (id, t) in &inner.tracked {
            consider(id, &t.mint, &t.symbol, t.pnl_1h_pct, t.pnl_24h_pct);
        }
        for d in &inner.done {
            consider(&d.signal_id, &d.mint, &d.symbol, d.pnl_1h_pct, d.pnl_24h_pct);
        }
        let summarize = |v: &[f64]| -> (usize, Option<f64>, Option<f64>) {
            if v.is_empty() {
                return (0, None, None);
            }
            let wins = v.iter().filter(|p| **p > 0.0).count();
            (
                v.len(),
                Some(wins as f64 / v.len() as f64 * 100.0),
                Some(v.iter().sum::<f64>() / v.len() as f64),
            )
        };
        let (measured_1h, win_rate_1h_pct, avg_pnl_1h_pct) = summarize(&p1h);
        let (measured_24h, win_rate_24h_pct, avg_pnl_24h_pct) = summarize(&p24h);
        PerfStats {
            total_signals: inner.tracked.len() + inner.done.len(),
            pending_signals: inner.tracked.len(),
            measured_1h,
            win_rate_1h_pct,
            avg_pnl_1h_pct,
            measured_24h,
            win_rate_24h_pct,
            avg_pnl_24h_pct,
            best_signal: best,
            worst_signal: worst,
        }
    }
}

/// Consume the signal broadcast forever, running a backfill round every
/// ROUND_INTERVAL_SECS.
pub async fn run(tracker: PnlTracker, mut rx: broadcast::Receiver<Arc<Signal>>, rest: BlurRest) {
    let mut round = tokio::time::interval(Duration::from_secs(ROUND_INTERVAL_SECS));
    round.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    info!("signal pnl tracker started");
    loop {
        tokio::select! {
            sig = rx.recv() => match sig {
                Ok(s) => tracker.track(&s),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "pnl tracker lagged behind signals");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = round.tick() => tracker.backfill_round(&rest).await,
        }
    }
}
