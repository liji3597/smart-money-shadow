//! Position book: one open position per mint, plus a bounded ring of closed
//! ones. Opened on buy (live fill or dry-run intent), closed by the exit loop
//! (take-profit / stop-loss / time-stop). Open and close transitions are
//! broadcast so the API can stream them.
//!
//! Open/close transitions are also persisted to a JSON file (atomic
//! write-tmp-then-rename) and restored on startup, so a backend restart
//! doesn't orphan tokens still sitting in the wallet.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{info, warn};

use shadow_core::now_unix;

const MAX_CLOSED: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PositionStatus {
    Open,
    Closed {
        reason: String,
        exit_price_usd: Option<f64>,
        pnl_sol: Option<f64>,
        closed_at: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub signal_id: String,
    pub mint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub dex: String,
    pub entry_price_usd: f64,
    pub sol_in: f64,
    /// Dry-run: quoted expected tokens (0 when no quote was available — PnL is
    /// then computed from the price ratio). Live: actual ATA balance after fill.
    /// Serialized, so a restored live position keeps its sell size.
    pub tokens: u64,
    pub opened_at: i64,
    /// Last price seen by the exit loop (display only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_price_usd: Option<f64>,
    pub status: PositionStatus,
    /// Ephemeral exit-loop counters: deliberately not persisted; a restored
    /// position restarts with a clean failure budget.
    #[serde(skip)]
    pub price_failures: u32,
    #[serde(skip)]
    pub sell_attempts: u32,
}

pub struct PositionStore {
    open: RwLock<HashMap<String, Position>>,
    closed: RwLock<VecDeque<Position>>,
    pub tx: broadcast::Sender<Arc<Position>>,
    /// JSON file for crash/restart recovery; None disables persistence.
    path: Option<String>,
}

impl Default for PositionStore {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(256);
        PositionStore {
            open: RwLock::new(HashMap::new()),
            closed: RwLock::new(VecDeque::new()),
            tx,
            path: None,
        }
    }
}

impl PositionStore {
    /// A store that rewrites `path` (whole JSON array, atomically) on every
    /// open/close transition and can `restore()` from it on startup.
    pub fn with_path(path: String) -> Self {
        PositionStore {
            path: Some(path),
            ..Self::default()
        }
    }

    pub fn is_open(&self, mint: &str) -> bool {
        self.open.read().unwrap().contains_key(mint)
    }

    /// Opens a position; returns false when the mint is already held.
    pub fn try_open(&self, pos: Position) -> bool {
        {
            let mut open = self.open.write().unwrap();
            if open.contains_key(&pos.mint) {
                return false;
            }
            let _ = self.tx.send(Arc::new(pos.clone()));
            open.insert(pos.mint.clone(), pos);
        }
        self.persist();
        true
    }

    pub fn open_snapshot(&self) -> Vec<Position> {
        self.open.read().unwrap().values().cloned().collect()
    }

    pub fn closed_snapshot(&self) -> Vec<Position> {
        self.closed.read().unwrap().iter().cloned().collect()
    }

    pub fn note_price(&self, mint: &str, price_usd: f64) {
        if let Some(p) = self.open.write().unwrap().get_mut(mint) {
            p.last_price_usd = Some(price_usd);
            p.price_failures = 0;
        }
    }

    pub fn bump_price_failure(&self, mint: &str) -> u32 {
        let mut open = self.open.write().unwrap();
        let Some(p) = open.get_mut(mint) else { return 0 };
        p.price_failures += 1;
        p.price_failures
    }

    pub fn bump_sell_attempt(&self, mint: &str) -> u32 {
        let mut open = self.open.write().unwrap();
        let Some(p) = open.get_mut(mint) else { return 0 };
        p.sell_attempts += 1;
        p.sell_attempts
    }

    pub fn close(&self, mint: &str, reason: &str, exit_price_usd: Option<f64>, pnl_sol: Option<f64>) {
        let mut open = self.open.write().unwrap();
        let Some(mut p) = open.remove(mint) else { return };
        drop(open);
        p.status = PositionStatus::Closed {
            reason: reason.to_owned(),
            exit_price_usd,
            pnl_sol,
            closed_at: now_unix(),
        };
        let _ = self.tx.send(Arc::new(p.clone()));
        {
            let mut closed = self.closed.write().unwrap();
            closed.push_front(p);
            while closed.len() > MAX_CLOSED {
                closed.pop_back();
            }
        }
        self.persist();
    }

    /// Rewrite the whole book (open + closed) as one JSON array. Writes to
    /// `<path>.tmp` first, then renames over the target; the remove before
    /// rename is needed because Windows refuses to rename over an existing
    /// file. Failures are logged, never fatal.
    fn persist(&self) {
        let Some(path) = &self.path else { return };
        let positions: Vec<Position> = {
            let open = self.open.read().unwrap();
            let closed = self.closed.read().unwrap();
            open.values().chain(closed.iter()).cloned().collect()
        };
        let json = match serde_json::to_string(&positions) {
            Ok(j) => j,
            Err(e) => {
                warn!(error = %e, "position book serialize failed");
                return;
            }
        };
        let tmp = format!("{path}.tmp");
        let result = std::fs::write(&tmp, json).and_then(|_| {
            let _ = std::fs::remove_file(path);
            std::fs::rename(&tmp, path)
        });
        if let Err(e) = result {
            warn!(error = %e, path, "position book persist failed");
        }
    }

    /// Load persisted positions into the book. Only `Open` positions are
    /// restored (closed ones are history, already reflected in the trade log).
    /// A corrupt file is reset to the current (empty) book; never panics.
    pub fn restore(&self) {
        let Some(path) = &self.path else { return };
        let raw = match std::fs::read_to_string(path) {
            Ok(r) => r,
            Err(_) => return, // no file yet — fresh start
        };
        let positions: Vec<Position> = match serde_json::from_str(&raw) {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, path, "positions file corrupt; resetting it");
                self.persist();
                return;
            }
        };
        let mut restored = 0usize;
        {
            let mut open = self.open.write().unwrap();
            for p in positions {
                if matches!(p.status, PositionStatus::Open) {
                    open.insert(p.mint.clone(), p);
                    restored += 1;
                }
            }
        }
        info!(restored, path, "restored open positions from file");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> String {
        std::env::temp_dir()
            .join(format!("shadow-positions-test-{tag}-{}.json", std::process::id()))
            .to_string_lossy()
            .into_owned()
    }

    fn sample(mint: &str) -> Position {
        Position {
            signal_id: format!("sig-{mint}"),
            mint: mint.to_owned(),
            symbol: Some("TEST".into()),
            dex: "pumpfun".into(),
            entry_price_usd: 0.001,
            sol_in: 0.02,
            tokens: 12_345,
            opened_at: now_unix(),
            last_price_usd: None,
            status: PositionStatus::Open,
            price_failures: 0,
            sell_attempts: 0,
        }
    }

    #[test]
    fn open_persists_deserializable_file() {
        let path = temp_path("persist");
        let _ = std::fs::remove_file(&path);
        let store = PositionStore::with_path(path.clone());
        assert!(store.try_open(sample("mintA")));
        let raw = std::fs::read_to_string(&path).expect("positions file written");
        let loaded: Vec<Position> = serde_json::from_str(&raw).expect("valid positions json");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].mint, "mintA");
        assert_eq!(loaded[0].tokens, 12_345);
        assert!(matches!(loaded[0].status, PositionStatus::Open));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn restore_reopens_positions() {
        let path = temp_path("restore");
        let _ = std::fs::remove_file(&path);
        {
            let store = PositionStore::with_path(path.clone());
            assert!(store.try_open(sample("mintB")));
            store.close("mintB", "take_profit", Some(0.002), Some(0.02));
            assert!(store.try_open(sample("mintC")));
        }
        let store = PositionStore::with_path(path.clone());
        assert!(!store.is_open("mintC"));
        store.restore();
        // closed positions stay history; open ones come back
        assert!(!store.is_open("mintB"));
        assert!(store.is_open("mintC"));
        let _ = std::fs::remove_file(&path);
    }
}
