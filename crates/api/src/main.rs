//! `shadow` — Smart-Money Shadow backend.
//!
//! Wires the four crates together: ingest (Blur gRPC stream) → engine
//! (signals + risk scoring) → trader (dry-run / Beam) → this HTTP+WS API.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::broadcast;
use tracing::info;
use tracing_subscriber::EnvFilter;

use shadow_core::{DexEvent, WalletBuy};
use shadow_engine::{Engine, EngineConfig};
use shadow_ingest::{run_blur, BlurRest, StreamHealth};
use shadow_trader::{TradeStore, TraderConfig};

mod config;
mod pnl;
mod routes;

use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = Config::from_env().context("configuration")?;
    info!(bind = %cfg.bind, live = cfg.live_trading, "smart-money shadow starting");

    // ingest → engine
    let (event_tx, _) = broadcast::channel::<Arc<DexEvent>>(8192);
    let health = StreamHealth::default();
    {
        let (key, types, tx, health) = (
            cfg.api_key.clone(),
            cfg.blur_event_types.clone(),
            event_tx.clone(),
            health.clone(),
        );
        if cfg.stream_transport == "grpc" {
            tokio::spawn(run_blur(key, types, tx, health));
        } else {
            tokio::spawn(shadow_ingest::run_blur_ws(key, types, tx, health));
        }
    }

    // engine
    let rest = BlurRest::new(cfg.api_key.clone(), Some(cfg.blur_rest_base.clone()));
    let engine = Engine::new(
        EngineConfig {
            min_smart_buy_usd: cfg.min_smart_buy_usd,
            min_signal_volume_usd: cfg.min_signal_volume_usd,
            min_smart_buy_sol: cfg.min_smart_buy_sol,
            signal_cooldown_secs: cfg.signal_cooldown_secs,
            surge_min_multiple: cfg.surge_min_multiple,
            smart_discovery: cfg.smart_discovery,
            ..Default::default()
        },
        &cfg.api_key,
        cfg.smart_money_seeds.clone(),
        rest.clone(),
    )
    .await
    .context("engine init")?;
    {
        let engine = engine.clone();
        let rx = event_tx.subscribe();
        tokio::spawn(async move { engine.run(rx).await });
    }

    // signal pnl backfill
    let pnl = pnl::PnlTracker::default();
    if cfg.pnl_track {
        tokio::spawn(pnl::run(
            pnl.clone(),
            engine.state.signal_tx.subscribe(),
            rest.clone(),
        ));
    }

    // gRPC wallet track → engine
    let (track_tx, _) = broadcast::channel::<Arc<WalletBuy>>(1024);
    let track_health = StreamHealth::default();
    if cfg.grpc_wallet_track {
        tokio::spawn(shadow_ingest::run_wallet_track(
            cfg.api_key.clone(),
            engine.state.smart_tx.subscribe(),
            track_tx.clone(),
            track_health.clone(),
        ));
        let engine = engine.clone();
        let rx = track_tx.subscribe();
        tokio::spawn(async move { engine.run_track(rx).await });
    }

    // trader
    let trade_store = Arc::new(TradeStore::default());
    let position_store = Arc::new(shadow_trader::position::PositionStore::with_path(
        cfg.positions_path.clone(),
    ));
    let beam_latency: Arc<AtomicU64> = engine.state.beam_latency_ms.clone();
    let wallet_balance: Arc<AtomicU64> = engine.state.wallet_balance_lamports.clone();
    tokio::spawn(shadow_trader::run(
        TraderConfig {
            live: cfg.live_trading,
            sol_per_trade: cfg.trade_sol_per_signal,
            max_daily_sol: cfg.max_daily_sol,
            keypair_b58: cfg.trader_keypair.clone(),
            swqos_keypair_b58: cfg.swqos_keypair.clone(),
            beam_health_check: cfg.beam_health_check,
            api_key: cfg.api_key.clone(),
            log_path: cfg.trade_log_path.clone(),
            slippage_bps: cfg.slippage_bps,
            priority_fee_microlamports: cfg.priority_fee_microlamports,
            dry_run_quote: cfg.dry_run_quote,
            take_profit_pct: cfg.take_profit_pct,
            stop_loss_pct: cfg.stop_loss_pct,
            max_hold_secs: cfg.max_hold_secs,
            position_check_secs: cfg.position_check_secs,
            blur_rest_base: cfg.blur_rest_base.clone(),
        },
        engine.state.signal_tx.subscribe(),
        trade_store.clone(),
        beam_latency,
        wallet_balance,
        position_store.clone(),
    ));

    // api
    routes::serve(
        cfg.bind.clone(),
        engine,
        health,
        track_health,
        trade_store,
        position_store,
        event_tx,
        pnl,
    )
    .await
}

/// Shared read handle handed to the HTTP/WS layer.
pub struct AppState {
    pub engine: Arc<Engine>,
    pub health: StreamHealth,
    pub track_health: StreamHealth,
    pub trades: Arc<TradeStore>,
    pub positions: Arc<shadow_trader::position::PositionStore>,
    pub event_tx: broadcast::Sender<Arc<DexEvent>>,
    pub pnl: pnl::PnlTracker,
}

impl AppState {
    pub fn metrics(&self) -> shadow_core::Metrics {
        let st = &self.engine.state;
        let mut m = st.metrics.read().unwrap().clone();
        m.smart_money_count = st.smart.read().unwrap().len();
        m.tracked_tokens = st.tokens.len();
        m.stream_connected = self.health.connected.load(Ordering::Relaxed);
        m.stream_reconnects = self.health.reconnects.load(Ordering::Relaxed);
        m.last_event_slot = self.health.last_slot.load(Ordering::Relaxed);
        m.grpc_track_connected = self.track_health.connected.load(Ordering::Relaxed);
        m.grpc_track_reconnects = self.track_health.reconnects.load(Ordering::Relaxed);
        let latency = st.beam_latency_ms.load(Ordering::Relaxed);
        m.beam_last_latency_ms = (latency > 0).then_some(latency);
        m.wallet_balance_sol =
            st.wallet_balance_lamports.load(Ordering::Relaxed) as f64 / 1e9;
        m.trades_total = self.trades.records.read().unwrap().len() as u64;
        let now = shadow_core::now_unix();
        let times = st.recent_event_times.read().unwrap();
        let recent = times.iter().filter(|t| now - *t < 60).count();
        m.events_per_min = recent as f64;
        m
    }
}
