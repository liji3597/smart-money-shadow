use anyhow::{bail, Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub bind: String,
    pub blur_rest_base: String,
    pub blur_event_types: Vec<String>,
    pub smart_money_seeds: Vec<String>,
    pub smart_discovery: bool,
    pub min_smart_buy_usd: f64,
    pub min_signal_volume_usd: f64,
    pub min_smart_buy_sol: f64,
    pub signal_cooldown_secs: i64,
    pub surge_min_multiple: f64,
    pub live_trading: bool,
    pub trade_sol_per_signal: f64,
    pub max_daily_sol: f64,
    pub trader_keypair: Option<String>,
    pub beam_health_check: bool,
    pub trade_log_path: String,
    pub slippage_bps: u64,
    pub priority_fee_microlamports: u64,
    pub dry_run_quote: bool,
    pub take_profit_pct: f64,
    pub stop_loss_pct: f64,
    pub max_hold_secs: i64,
    pub position_check_secs: i64,
    /// "ws" (default, delivers every event type) or "grpc".
    pub stream_transport: String,
    /// Track smart-money wallets natively over Yellowstone gRPC.
    pub grpc_wallet_track: bool,
    /// Backfill signal PnL (1h/24h) via Blur REST token prices.
    pub pnl_track: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("SOLAMI_API_KEY")
            .context("SOLAMI_API_KEY is required — get one at https://solami.dev/signup")?;
        if api_key.trim().is_empty() {
            bail!("SOLAMI_API_KEY is empty");
        }
        Ok(Config {
            api_key,
            bind: env_or("BIND", "127.0.0.1:8080"),
            blur_rest_base: env_or("BLUR_REST_BASE", "https://api.solami.dev/data"),
            blur_event_types: env_list(
                "BLUR_EVENT_TYPES",
                &["swap", "token_create", "pool_create", "meme", "graduation", "surge", "metadata"],
            ),
            smart_money_seeds: env_list("SMART_MONEY_SEEDS", &[]),
            smart_discovery: env_bool("SMART_DISCOVERY", true),
            min_smart_buy_usd: env_f64("MIN_SMART_BUY_USD", 500.0),
            min_signal_volume_usd: env_f64("MIN_SIGNAL_VOLUME_USD", 5_000.0),
            min_smart_buy_sol: env_f64("MIN_SMART_BUY_SOL", 0.5),
            signal_cooldown_secs: env_i64("SIGNAL_COOLDOWN_SECS", 900),
            surge_min_multiple: env_f64("SURGE_MIN_MULTIPLE", 4.0),
            live_trading: env_bool("LIVE_TRADING", false),
            trade_sol_per_signal: env_f64("TRADE_SOL_PER_SIGNAL", 0.02),
            max_daily_sol: env_f64("MAX_DAILY_SOL", 0.2),
            trader_keypair: std::env::var("SOLAMI_TRADER_KEYPAIR")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            beam_health_check: env_bool("BEAM_HEALTH_CHECK", false),
            trade_log_path: env_or("TRADE_LOG_PATH", "trades.jsonl"),
            slippage_bps: env_u64("SLIPPAGE_BPS", 1500),
            priority_fee_microlamports: env_u64("PRIORITY_FEE_MICROLAMPORTS", 50_000),
            dry_run_quote: env_bool("DRY_RUN_QUOTE", true),
            take_profit_pct: env_f64("TAKE_PROFIT_PCT", 50.0),
            stop_loss_pct: env_f64("STOP_LOSS_PCT", -30.0),
            max_hold_secs: env_i64("MAX_HOLD_SECS", 86_400),
            position_check_secs: env_i64("POSITION_CHECK_SECS", 60),
            stream_transport: env_or("STREAM_TRANSPORT", "ws"),
            grpc_wallet_track: env_bool("GRPC_WALLET_TRACK", true),
            pnl_track: env_bool("PNL_TRACK", true),
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).ok().filter(|s| !s.is_empty()).unwrap_or_else(|| default.to_owned())
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_list(key: &str, default: &[&str]) -> Vec<String> {
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => {
            v.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect()
        }
        _ => default.iter().map(|s| s.to_string()).collect(),
    }
}
