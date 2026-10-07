//! Wallet style profiler: accumulates per-wallet behavior from the gRPC
//! wallet-track trade stream and turns it into a follow gate.
//!
//! For every tracked wallet we pair buys with sells (FIFO per mint) into
//! round trips and keep: round-trip count, wins (sell proceeds > buy spend,
//! SOL-denominated — an estimate, per the event data we have), and recent
//! hold durations. The median hold time classifies the wallet as "scalper"
//! (<30 min) or "swing". The gate then blocks chronically losing wallets,
//! boosts proven winners, and lets unprofiled wallets through at 1x while
//! their sample builds up (cold start).
//!
//! Profiles persist to a JSON file (atomic write-tmp-then-rename, same
//! pattern as positions.json) and are restored on startup.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::EngineConfig;

/// Median hold time below this classifies a wallet as a scalper.
const SCALPER_HOLD_SECS: i64 = 30 * 60;
/// Win rate at or above this earns the position-size boost.
const BOOST_WIN_RATE: f64 = 0.6;
/// Size multiplier for wallets at/above `BOOST_WIN_RATE`.
const BOOST_MULTIPLIER: f64 = 1.5;
/// Keep the last N hold durations per wallet for the median.
const MAX_HOLD_SAMPLES: usize = 64;
/// Bound open (unsold) buy entries per wallet so dust sprees don't grow it.
const MAX_OPEN_BUYS: usize = 128;
/// Bound the profile map; beyond this, new wallets simply stay unprofiled
/// (they fall through to the cold-start path).
const MAX_PROFILES: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenBuy {
    mint: String,
    sol: f64,
    at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WalletProfile {
    pub round_trips: u32,
    pub wins: u32,
    /// Recent hold durations (seconds), bounded to `MAX_HOLD_SAMPLES`.
    hold_secs: Vec<i64>,
    /// Buys not yet matched by a sell; oldest first (FIFO).
    open_buys: Vec<OpenBuy>,
}

impl WalletProfile {
    pub fn win_rate(&self) -> f64 {
        if self.round_trips == 0 {
            0.0
        } else {
            self.wins as f64 / self.round_trips as f64
        }
    }

    fn median_hold_secs(&self) -> Option<i64> {
        if self.hold_secs.is_empty() {
            return None;
        }
        let mut sorted = self.hold_secs.clone();
        sorted.sort_unstable();
        Some(sorted[sorted.len() / 2])
    }

    /// "scalper" for sub-30-minute median holds, "swing" otherwise; None
    /// before the first completed round trip.
    pub fn style(&self) -> Option<&'static str> {
        self.median_hold_secs()
            .map(|m| if m < SCALPER_HOLD_SECS { "scalper" } else { "swing" })
    }
}

/// Follow decision for one wallet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gate {
    /// Drop this wallet's signals entirely.
    Block,
    /// Let the signal through with this position-size multiplier.
    Pass(f64),
}

pub struct WalletProfiler {
    profiles: RwLock<HashMap<String, WalletProfile>>,
    /// JSON file for crash/restart recovery; None disables persistence.
    path: Option<String>,
}

impl WalletProfiler {
    pub fn new(path: Option<String>) -> Self {
        WalletProfiler { profiles: RwLock::new(HashMap::new()), path }
    }

    pub fn record_buy(&self, wallet: &str, mint: &str, sol: f64, at: i64) {
        let recorded = {
            let mut profiles = self.profiles.write().unwrap();
            if !profiles.contains_key(wallet) && profiles.len() >= MAX_PROFILES {
                false // map full; wallet stays unprofiled (cold-start path)
            } else {
                let p = profiles.entry(wallet.to_owned()).or_default();
                p.open_buys.push(OpenBuy { mint: mint.to_owned(), sol, at });
                while p.open_buys.len() > MAX_OPEN_BUYS {
                    p.open_buys.remove(0);
                }
                true
            }
        };
        // Persist open buys too: a restart that loses them turns the matching
        // sell into a silently ignored round trip.
        if recorded {
            self.persist();
        }
    }

    /// Match a sell against the wallet's oldest open buy of the same mint.
    /// Unmatched sells (bought before tracking started) are ignored.
    pub fn record_sell(&self, wallet: &str, mint: &str, sol_received: f64, at: i64) {
        {
            let mut profiles = self.profiles.write().unwrap();
            let Some(p) = profiles.get_mut(wallet) else { return };
            let Some(i) = p.open_buys.iter().position(|b| b.mint == mint) else { return };
            let open = p.open_buys.remove(i);
            p.round_trips += 1;
            if sol_received > open.sol {
                p.wins += 1;
            }
            p.hold_secs.push((at - open.at).max(0));
            while p.hold_secs.len() > MAX_HOLD_SAMPLES {
                p.hold_secs.remove(0);
            }
        }
        self.persist();
    }

    pub fn style(&self, wallet: &str) -> Option<String> {
        self.profiles
            .read()
            .unwrap()
            .get(wallet)
            .and_then(|p| p.style().map(str::to_owned))
    }

    /// Follow gate for a wallet about to trigger a signal.
    pub fn gate(&self, wallet: &str, cfg: &EngineConfig) -> Gate {
        let profiles = self.profiles.read().unwrap();
        let Some(p) = profiles.get(wallet) else {
            return if cfg.follow_unknown_wallets { Gate::Pass(1.0) } else { Gate::Block };
        };
        if p.round_trips < cfg.min_wallet_round_trips {
            // Cold start: not enough samples to judge.
            return if cfg.follow_unknown_wallets { Gate::Pass(1.0) } else { Gate::Block };
        }
        let wr = p.win_rate();
        if wr < cfg.min_wallet_win_rate {
            return Gate::Block;
        }
        if wr >= BOOST_WIN_RATE {
            return Gate::Pass(BOOST_MULTIPLIER);
        }
        Gate::Pass(1.0)
    }

    /// Rewrite all profiles as one JSON object, atomically (tmp + rename).
    fn persist(&self) {
        let Some(path) = &self.path else { return };
        let json = {
            let profiles = self.profiles.read().unwrap();
            match serde_json::to_string(&*profiles) {
                Ok(j) => j,
                Err(e) => {
                    warn!(error = %e, "wallet profiles serialize failed");
                    return;
                }
            }
        };
        let tmp = format!("{path}.tmp");
        let result = std::fs::write(&tmp, json).and_then(|_| {
            let _ = std::fs::remove_file(path);
            std::fs::rename(&tmp, path)
        });
        if let Err(e) = result {
            warn!(error = %e, path, "wallet profiles persist failed");
        }
    }

    /// Load persisted profiles. Missing file = fresh start; a corrupt file is
    /// logged and skipped (kept on disk for inspection). Never panics.
    pub fn restore(&self) {
        let Some(path) = &self.path else { return };
        let raw = match std::fs::read_to_string(path) {
            Ok(r) => r,
            Err(_) => return,
        };
        match serde_json::from_str::<HashMap<String, WalletProfile>>(&raw) {
            Ok(p) => {
                let n = p.len();
                *self.profiles.write().unwrap() = p;
                info!(restored = n, path, "wallet profiles restored from file");
            }
            Err(e) => warn!(error = %e, path, "wallet profiles file corrupt; starting fresh"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> EngineConfig {
        EngineConfig::default()
    }

    /// Drive `n` round trips: `wins` of them profitable, each held `hold` secs.
    fn drive(p: &WalletProfiler, wallet: &str, trips: u32, wins: u32, hold: i64) {
        for i in 0..trips {
            let t0 = 1_000_000 + (i as i64) * 10_000;
            let win = i < wins;
            let (spend, received) = if win { (1.0, 1.2) } else { (1.0, 0.8) };
            p.record_buy(wallet, "mintX", spend, t0);
            p.record_sell(wallet, "mintX", received, t0 + hold);
        }
    }

    #[test]
    fn round_trip_accumulation_and_win_estimate() {
        let p = WalletProfiler::new(None);
        p.record_buy("w", "mintA", 1.0, 100);
        p.record_buy("w", "mintB", 2.0, 110);
        p.record_sell("w", "mintA", 1.5, 400); // win, 300s hold
        p.record_sell("w", "mintB", 1.0, 500); // loss, 390s hold
        p.record_sell("w", "mintC", 9.9, 600); // no open buy — ignored
        let profiles = p.profiles.read().unwrap();
        let w = &profiles["w"];
        assert_eq!(w.round_trips, 2);
        assert_eq!(w.wins, 1);
        assert!((w.win_rate() - 0.5).abs() < 1e-9);
        assert!(w.open_buys.is_empty());
    }

    #[test]
    fn sell_matches_oldest_open_buy_of_same_mint() {
        let p = WalletProfiler::new(None);
        p.record_buy("w", "mintA", 1.0, 100);
        p.record_buy("w", "mintA", 5.0, 200);
        p.record_sell("w", "mintA", 1.1, 300); // matches the 1.0 buy → win
        let profiles = p.profiles.read().unwrap();
        let w = &profiles["w"];
        assert_eq!(w.round_trips, 1);
        assert_eq!(w.wins, 1);
        assert_eq!(w.open_buys.len(), 1);
        assert!((w.open_buys[0].sol - 5.0).abs() < 1e-9);
    }

    #[test]
    fn style_classification_boundary() {
        let p = WalletProfiler::new(None);
        assert_eq!(p.style("w"), None); // no data yet
        drive(&p, "scalp", 1, 1, SCALPER_HOLD_SECS - 1);
        assert_eq!(p.style("scalp").as_deref(), Some("scalper"));
        drive(&p, "swing", 1, 1, SCALPER_HOLD_SECS);
        assert_eq!(p.style("swing").as_deref(), Some("swing"));
        // Median of [100, 2000, 3000] = 2000 → swing even with a scalpy sample.
        p.record_buy("mixed", "mintX", 1.0, 100);
        p.record_sell("mixed", "mintX", 1.1, 200); // hold 100
        p.record_buy("mixed", "mintX", 1.0, 300);
        p.record_sell("mixed", "mintX", 1.1, 2300); // hold 2000
        p.record_buy("mixed", "mintX", 1.0, 400);
        p.record_sell("mixed", "mintX", 1.1, 3400); // hold 3000
        assert_eq!(p.style("mixed").as_deref(), Some("swing"));
    }

    #[test]
    fn gate_cold_start_passes_at_1x() {
        let p = WalletProfiler::new(None);
        assert_eq!(p.gate("unknown", &cfg()), Gate::Pass(1.0));
        drive(&p, "thin", 2, 2, 60); // below min_wallet_round_trips
        assert_eq!(p.gate("thin", &cfg()), Gate::Pass(1.0));
    }

    #[test]
    fn gate_blocks_unknown_when_follow_unknown_off() {
        let p = WalletProfiler::new(None);
        let cfg = EngineConfig { follow_unknown_wallets: false, ..cfg() };
        assert_eq!(p.gate("unknown", &cfg), Gate::Block);
        drive(&p, "thin", 2, 2, 60);
        assert_eq!(p.gate("thin", &cfg), Gate::Block);
    }

    #[test]
    fn gate_blocks_chronic_loser() {
        let p = WalletProfiler::new(None);
        drive(&p, "loser", 3, 1, 60); // win rate 0.33 < 0.40
        assert_eq!(p.gate("loser", &cfg()), Gate::Block);
        drive(&p, "coinflip", 4, 2, 60); // win rate 0.50: passes, no boost
        assert_eq!(p.gate("coinflip", &cfg()), Gate::Pass(1.0));
    }

    #[test]
    fn gate_boosts_proven_winner() {
        let p = WalletProfiler::new(None);
        drive(&p, "winner", 3, 3, 60); // win rate 1.0 ≥ 0.60
        assert_eq!(p.gate("winner", &cfg()), Gate::Pass(1.5));
        drive(&p, "sixty", 5, 3, 60); // exactly 0.60 → boost
        assert_eq!(p.gate("sixty", &cfg()), Gate::Pass(1.5));
    }

    #[test]
    fn persistence_round_trip() {
        let path = std::env::temp_dir()
            .join(format!("shadow-wallet-profiles-test-{}.json", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let _ = std::fs::remove_file(&path);
        {
            let p = WalletProfiler::new(Some(path.clone()));
            drive(&p, "w", 3, 2, 60);
            p.record_buy("w", "mintOpen", 0.7, 999);
        }
        let p = WalletProfiler::new(Some(path.clone()));
        assert_eq!(p.gate("w", &cfg()), Gate::Pass(1.0)); // nothing yet
        p.restore();
        let profiles = p.profiles.read().unwrap();
        let w = &profiles["w"];
        assert_eq!(w.round_trips, 3);
        assert_eq!(w.wins, 2);
        assert_eq!(w.open_buys.len(), 1);
        drop(profiles);
        // 2/3 win rate ≥ 0.60 → boost survives the restart
        assert_eq!(p.gate("w", &cfg()), Gate::Pass(1.5));
        let _ = std::fs::remove_file(&path);
    }
}
