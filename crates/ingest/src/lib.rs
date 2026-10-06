//! Ingest: pulls decoded market-data events from Solami Blur (over the
//! Yellowstone gRPC transport) and fetches discovery data from the Blur REST
//! API.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::broadcast;
use tracing::{info, warn};

use shadow_core::DexEvent;

mod grpc_track;

pub use grpc_track::run_wallet_track;

/// Liveness of the Blur stream, shared with the API layer for `/api/metrics`.
#[derive(Clone, Default)]
pub struct StreamHealth {
    pub connected: Arc<AtomicBool>,
    pub last_slot: Arc<AtomicU64>,
    pub reconnects: Arc<AtomicU64>,
}

/// Run the Blur ingest loop forever, reconnecting with capped exponential
/// backoff. Events are fanned out on `tx`.
pub async fn run_blur(
    api_key: String,
    event_types: Vec<String>,
    tx: broadcast::Sender<Arc<DexEvent>>,
    health: StreamHealth,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        match stream_once(&api_key, &event_types, &tx, &health).await {
            Ok(()) => warn!("blur stream ended by server, reconnecting"),
            Err(e) => warn!(error = %e, "blur stream error, reconnecting"),
        }
        health.connected.store(false, Ordering::Relaxed);
        let n = health.reconnects.fetch_add(1, Ordering::Relaxed) + 1;
        info!(reconnect = n, backoff_ms = backoff.as_millis() as u64, "blur backoff");
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

async fn stream_once(
    api_key: &str,
    event_types: &[String],
    tx: &broadcast::Sender<Arc<DexEvent>>,
    health: &StreamHealth,
) -> Result<()> {
    let mut client = solami::builder().with_grpc(api_key).build().await?;
    let mut stream = client
        .grpc()
        .subscribe_blur_events(event_types.to_vec())
        .await?;
    health.connected.store(true, Ordering::Relaxed);
    info!(types = ?event_types, "blur stream connected");

    while let Some(update) = stream.updates.next().await {
        let ev = update.map_err(|s| anyhow!("grpc status: {s}"))?;
        health.last_slot.store(ev.slot, Ordering::Relaxed);
        if ev.json.is_empty() {
            continue;
        }
        let _ = tx.send(Arc::new(DexEvent::parse_typed(&ev.json, &ev.event_type)));
    }
    Ok(())
}

/// Run the Blur ingest over the plain WebSocket transport
/// (`wss://ws.solami.dev/data/subscribe`) — same decoded events as gRPC.
///
/// This is the default transport: on mainnet the gRPC Blur stream delivers
/// launch-type events but not `swap`, while the WS delivers everything.
pub async fn run_blur_ws(
    api_key: String,
    event_types: Vec<String>,
    tx: broadcast::Sender<Arc<DexEvent>>,
    health: StreamHealth,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        match ws_once(&api_key, &event_types, &tx, &health).await {
            Ok(()) => warn!("blur ws ended by server, reconnecting"),
            Err(e) => warn!(error = %e, "blur ws error, reconnecting"),
        }
        health.connected.store(false, Ordering::Relaxed);
        let n = health.reconnects.fetch_add(1, Ordering::Relaxed) + 1;
        info!(reconnect = n, backoff_ms = backoff.as_millis() as u64, "blur backoff");
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

async fn ws_once(
    api_key: &str,
    event_types: &[String],
    tx: &broadcast::Sender<Arc<DexEvent>>,
    health: &StreamHealth,
) -> Result<()> {
    use tokio_tungstenite::tungstenite::Message;

    let url = format!(
        "wss://ws.solami.dev/data/subscribe?chain=solana&api_key={}&type={}",
        api_key,
        event_types.join(",")
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| anyhow!("blur ws connect: {e}"))?;
    health.connected.store(true, Ordering::Relaxed);
    info!(types = ?event_types, "blur ws connected");

    while let Some(msg) = ws.next().await {
        let msg = msg.map_err(|e| anyhow!("blur ws: {e}"))?;
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(frame) => {
                // 4002 = out of prepaid bandwidth
                warn!(?frame, "blur ws closed by server");
                return Ok(());
            }
            _ => continue,
        };
        let v: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match v.get("type").and_then(|t| t.as_str()) {
            // handshake / control frames, not market data
            Some("connected") | Some("replay_end") => continue,
            _ => {}
        }
        if let Some(slot) = v.get("slot").and_then(|s| s.as_u64()) {
            health.last_slot.store(slot, Ordering::Relaxed);
        }
        let _ = tx.send(Arc::new(DexEvent::from_value(v)));
    }
    Ok(())
}

/// Minimal client for the Blur REST API (`https://api.solami.dev/data`).
///
/// Used for smart-money discovery (trader leaderboard) and point lookups that
/// would be wasteful to compute from the stream.
#[derive(Clone)]
pub struct BlurRest {
    http: reqwest::Client,
    base: String,
    key: String,
}

impl BlurRest {
    pub fn new(key: impl Into<String>, base: Option<String>) -> Self {
        BlurRest {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            base: base.unwrap_or_else(|| "https://api.solami.dev/data".to_owned()),
            key: key.into(),
        }
    }

    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let mut q: Vec<(&str, String)> = query.to_vec();
        q.push(("chain", "solana".to_owned()));
        q.push(("api_key", self.key.clone()));
        let resp = self.http.get(&url).query(&q).send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("blur rest {path} -> {status}: {body}"));
        }
        serde_json::from_str(&body).with_context(|| format!("blur rest {path}: bad json"))
    }

    /// Raw passthrough for endpoints we don't model yet (e.g. OHLCV candles
    /// for the account page). Same auth and error shape as `get`.
    pub async fn raw_get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        self.get(path, query).await
    }

    /// Wallets from the Blur PnL trader leaderboard.
    pub async fn top_trader_wallets(&self, limit: usize) -> Result<Vec<String>> {
        let v = self
            .get("/pnl/leaderboard", &[("limit", limit.to_string())])
            .await?;
        let rows = v
            .as_array()
            .cloned()
            .or_else(|| v.get("data").and_then(|d| d.as_array()).cloned())
            .or_else(|| v.get("traders").and_then(|d| d.as_array()).cloned())
            .unwrap_or_default();
        Ok(rows
            .iter()
            .filter_map(|t| {
                ["wallet", "trader", "address"]
                    .iter()
                    .find_map(|k| t.get(k).and_then(|w| w.as_str()))
                    .map(String::from)
            })
            .collect())
    }

    /// Point price lookup for a mint (used to sanity-check stream prices).
    /// `GET /data/token/price?chain=solana&address=<mint>` answers with a
    /// one-element array whose `price_usd` is a decimal string.
    pub async fn token_price_usd(&self, mint: &str) -> Result<Option<f64>> {
        let v = self
            .get("/token/price", &[("address", mint.to_owned())])
            .await?;
        let row = v.as_array().and_then(|a| a.first()).cloned().unwrap_or(v);
        let raw = row
            .get("price_usd")
            .or_else(|| row.get("price"))
            .cloned()
            .unwrap_or(Value::Null);
        Ok(match raw {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        })
    }

    /// Symbol/name lookup for a mint.
    /// `GET /data/token/metadata?chain=solana&address=<mint>` answers with a
    /// one-element array; empty strings are treated as missing.
    pub async fn token_metadata(
        &self,
        mint: &str,
    ) -> Result<Option<(Option<String>, Option<String>)>> {
        let v = self
            .get("/token/metadata", &[("address", mint.to_owned())])
            .await?;
        let row = v.as_array().and_then(|a| a.first()).cloned().unwrap_or(v);
        let pick = |k: &str| {
            row.get(k)
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let out = (pick("symbol"), pick("name"));
        Ok(if out.0.is_none() && out.1.is_none() {
            None
        } else {
            Some(out)
        })
    }
}
