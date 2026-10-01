//! Position book: one open position per mint, plus a bounded ring of closed
//! ones. Opened on buy (live fill or dry-run intent), closed by the exit loop
//! (take-profit / stop-loss / time-stop). Open and close transitions are
//! broadcast so the API can stream them.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use serde::Serialize;
use tokio::sync::broadcast;

use shadow_core::now_unix;

const MAX_CLOSED: usize = 500;

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
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
    pub tokens: u64,
    pub opened_at: i64,
    /// Last price seen by the exit loop (display only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_price_usd: Option<f64>,
    pub status: PositionStatus,
    #[serde(skip)]
    pub price_failures: u32,
    #[serde(skip)]
    pub sell_attempts: u32,
}

pub struct PositionStore {
    open: RwLock<HashMap<String, Position>>,
    closed: RwLock<VecDeque<Position>>,
    pub tx: broadcast::Sender<Arc<Position>>,
}

impl Default for PositionStore {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(256);
        PositionStore {
            open: RwLock::new(HashMap::new()),
            closed: RwLock::new(VecDeque::new()),
            tx,
        }
    }
}

impl PositionStore {
    pub fn is_open(&self, mint: &str) -> bool {
        self.open.read().unwrap().contains_key(mint)
    }

    /// Opens a position; returns false when the mint is already held.
    pub fn try_open(&self, pos: Position) -> bool {
        let mut open = self.open.write().unwrap();
        if open.contains_key(&pos.mint) {
            return false;
        }
        let _ = self.tx.send(Arc::new(pos.clone()));
        open.insert(pos.mint.clone(), pos);
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
        let mut closed = self.closed.write().unwrap();
        closed.push_front(p);
        while closed.len() > MAX_CLOSED {
            closed.pop_back();
        }
    }
}
