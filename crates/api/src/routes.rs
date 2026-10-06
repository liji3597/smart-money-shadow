//! HTTP + WebSocket API.
//!
//! REST:
//!   GET /api/health       liveness + stream state
//!   GET /api/metrics      the numbers the bounty asks us to surface
//!   GET /api/signals      recent signals (newest first, PnL merged in)
//!   GET /api/performance  signal win-rate / PnL aggregates
//!   GET /api/tokens       token board, most recently active first
//!   GET /api/tokens/{mint}
//!   GET /api/smart-money  the tracked wallet set
//!   GET /api/trades       dry-run / live trade records
//! WS:
//!   /ws — sends a `snapshot` on connect, then streams `event` / `signal` /
//!   `trade` frames as they happen.

use std::sync::Arc;

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::info;

use shadow_core::{token_json, now_unix};
use shadow_engine::Engine;
use shadow_ingest::StreamHealth;
use shadow_trader::position::PositionStore;
use shadow_trader::TradeStore;

use crate::pnl::PnlTracker;
use crate::AppState;

#[allow(clippy::too_many_arguments)]
pub async fn serve(
    bind: String,
    engine: Arc<Engine>,
    health: StreamHealth,
    track_health: StreamHealth,
    trades: Arc<TradeStore>,
    positions: Arc<PositionStore>,
    event_tx: tokio::sync::broadcast::Sender<Arc<shadow_core::DexEvent>>,
    pnl: PnlTracker,
    rest: shadow_ingest::BlurRest,
) -> Result<()> {
    let state = Arc::new(AppState {
        engine,
        health,
        track_health,
        trades,
        positions,
        event_tx,
        pnl,
        rest,
    });
    let app = Router::new()
        .route("/api/health", get(api_health))
        .route("/api/metrics", get(api_metrics))
        .route("/api/signals", get(api_signals))
        .route("/api/performance", get(api_performance))
        .route("/api/tokens", get(api_tokens))
        .route("/api/tokens/{mint}", get(api_token_detail))
        .route("/api/smart-money", get(api_smart_money))
        .route("/api/trades", get(api_trades))
        .route("/api/positions", get(api_positions))
        .route("/api/ohlcv", get(api_ohlcv))
        .route("/ws", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = TcpListener::bind(&bind).await?;
    info!(%bind, "api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn api_health(State(st): State<Arc<AppState>>) -> Json<Value> {
    let connected = st
        .health
        .connected
        .load(std::sync::atomic::Ordering::Relaxed);
    Json(json!({
        "ok": true,
        "stream_connected": connected,
        "uptime_secs": now_unix() - st.engine.state.metrics.read().unwrap().started_at,
    }))
}

async fn api_metrics(State(st): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "metrics": st.metrics() }))
}

#[derive(Deserialize)]
struct Limit {
    limit: Option<usize>,
}

fn clamp_limit(limit: Option<usize>, default: usize, max: usize) -> usize {
    limit.unwrap_or(default).clamp(1, max)
}

/// Serialize a signal and merge in its backfilled PnL fields, if any.
fn signal_json(st: &AppState, s: &shadow_core::Signal) -> Value {
    let mut v = json!(s);
    if let Some(pnl) = st.pnl.pnl_for(&s.id) {
        if let (Value::Object(m), Ok(Value::Object(pm))) = (&mut v, serde_json::to_value(pnl)) {
            m.extend(pm);
        }
    }
    v
}

async fn api_signals(
    State(st): State<Arc<AppState>>,
    Query(q): Query<Limit>,
) -> Json<Value> {
    let limit = clamp_limit(q.limit, 50, 500);
    let signals = st.engine.state.signals.read().unwrap();
    let out: Vec<Value> = signals
        .iter()
        .take(limit)
        .map(|s| signal_json(&st, s))
        .collect();
    Json(json!({ "signals": out }))
}

async fn api_performance(State(st): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "performance": st.pnl.stats() }))
}

async fn api_tokens(State(st): State<Arc<AppState>>, Query(q): Query<Limit>) -> Json<Value> {
    let limit = clamp_limit(q.limit, 100, 1000);
    let mut rows: Vec<Value> = st
        .engine
        .state
        .tokens
        .iter()
        .map(|e| token_json(e.value()))
        .collect();
    rows.sort_by_key(|r| {
        std::cmp::Reverse(
            r.get("last_activity")
                .and_then(|v| v.as_i64())
                .unwrap_or_default(),
        )
    });
    rows.truncate(limit);
    Json(json!({ "tokens": rows }))
}

async fn api_token_detail(
    State(st): State<Arc<AppState>>,
    Path(mint): Path<String>,
) -> impl IntoResponse {
    match st.engine.state.tokens.get(&mint) {
        Some(t) => (StatusCode::OK, Json(token_json(t.value()))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "unknown mint" })),
        )
            .into_response(),
    }
}

async fn api_smart_money(State(st): State<Arc<AppState>>) -> Json<Value> {
    let smart = st.engine.state.smart.read().unwrap();
    let mut wallets: Vec<&String> = smart.iter().collect();
    wallets.sort();
    Json(json!({ "wallets": wallets }))
}

async fn api_trades(State(st): State<Arc<AppState>>, Query(q): Query<Limit>) -> Json<Value> {
    let limit = clamp_limit(q.limit, 50, 500);
    let trades = st.trades.records.read().unwrap();
    let out: Vec<Value> = trades.iter().take(limit).map(|t| json!(**t)).collect();
    Json(json!({ "trades": out }))
}

async fn api_positions(State(st): State<Arc<AppState>>) -> Json<Value> {
    Json(positions_json(&st.positions))
}

#[derive(Deserialize)]
struct OhlcvQuery {
    mint: String,
    interval: Option<String>,
    limit: Option<usize>,
}

/// Server-side proxy to Blur REST OHLCV — the browser never sees the API key.
/// Forwards the response JSON verbatim; upstream failures surface as 502.
async fn api_ohlcv(
    State(st): State<Arc<AppState>>,
    Query(q): Query<OhlcvQuery>,
) -> impl IntoResponse {
    let interval = q.interval.unwrap_or_else(|| "1m".to_owned());
    let limit = q.limit.unwrap_or(200).clamp(1, 1000).to_string();
    match st
        .rest
        .raw_get(
            "/token/ohlcv",
            &[("address", q.mint), ("interval", interval), ("limit", limit)],
        )
        .await
    {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}

fn positions_json(positions: &PositionStore) -> Value {
    let mut open: Vec<Value> = positions.open_snapshot().iter().map(|p| json!(p)).collect();
    open.sort_by_key(|p| {
        std::cmp::Reverse(p.get("opened_at").and_then(|v| v.as_i64()).unwrap_or_default())
    });
    let closed: Vec<Value> = positions.closed_snapshot().iter().map(|p| json!(p)).collect();
    json!({ "open": open, "closed": closed })
}

async fn ws_handler(ws: WebSocketUpgrade, State(st): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, st))
}

async fn handle_ws(mut socket: WebSocket, st: Arc<AppState>) {
    let snapshot = json!({
        "kind": "snapshot",
        "data": {
            "metrics": st.metrics(),
            "signals": st.engine.state.signals.read().unwrap().iter().take(50).map(|s| signal_json(&st, s)).collect::<Vec<_>>(),
            "trades": st.trades.records.read().unwrap().iter().take(50).map(|t| json!(**t)).collect::<Vec<_>>(),
            "positions": positions_json(&st.positions),
        }
    });
    if socket
        .send(Message::Text(snapshot.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    let mut events = st.event_tx.subscribe();
    let mut signals = st.engine.state.signal_tx.subscribe();
    let mut trades = st.trades.tx.subscribe();
    let mut positions = st.positions.tx.subscribe();

    loop {
        let frame = tokio::select! {
            e = events.recv() => e.ok().map(|e| json!({ "kind": "event", "data": &*e })),
            s = signals.recv() => s.ok().map(|s| json!({ "kind": "signal", "data": &*s })),
            t = trades.recv() => t.ok().map(|t| json!({ "kind": "trade", "data": &*t })),
            p = positions.recv() => p.ok().map(|p| json!({ "kind": "position", "data": &*p })),
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => continue,
                Some(Err(_)) => break,
            },
        };
        let text = match frame {
            Some(f) => f.to_string(),
            None => break,
        };
        if socket.send(Message::Text(text.into())).await.is_err() {
            break;
        }
    }
}
