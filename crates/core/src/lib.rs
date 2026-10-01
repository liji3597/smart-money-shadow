//! Shared types for Smart-Money Shadow.
//!
//! Blur returns every fractional value (prices, USD amounts, percentages) as a
//! JSON string holding a plain decimal, and whole-number base-unit amounts as
//! strings too. The `de_*` deserializers below accept both strings and numbers.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

pub fn de_f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(de_opt_f64(d)?.unwrap_or(0.0))
}

pub fn de_opt_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    use serde::de::Error;
    match Value::deserialize(d)? {
        Value::Null => Ok(None),
        Value::Number(n) => Ok(n.as_f64()),
        Value::String(s) => {
            let s = s.trim();
            if s.is_empty() {
                return Ok(None);
            }
            s.parse::<f64>()
                .map(Some)
                .map_err(|_| Error::custom(format!("not a decimal: {s:?}")))
        }
        other => Err(Error::custom(format!("expected decimal, got {other}"))),
    }
}

pub fn de_u64<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    use serde::de::Error;
    match Value::deserialize(d)? {
        Value::Null => Ok(0),
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_f64().map(|f| f as u64))
            .ok_or_else(|| Error::custom("bad integer")),
        Value::String(s) => s
            .trim()
            .parse::<u64>()
            .map_err(|_| Error::custom(format!("not an integer: {s:?}"))),
        other => Err(Error::custom(format!("expected integer, got {other}"))),
    }
}

/// Infer the event type from its field shape when the payload omits `type`.
fn infer_type(v: &Value) -> &'static str {
    let has = |k: &str| v.get(k).is_some_and(|x| !x.is_null());
    if has("side") && has("trader") && has("signature") {
        "swap"
    } else if has("launchpad") && has("progress_pct") {
        "meme"
    } else if has("multiple") && has("baseline_usd") {
        "surge"
    } else if has("creator") && has("uri") {
        "token_create"
    } else if has("pool") && has("quote_mint") {
        "pool_create"
    } else if has("interval") && has("open") && has("close") {
        "candle"
    } else {
        "other"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Swap {
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub slot: u64,
    #[serde(default)]
    pub block_time: i64,
    #[serde(default)]
    pub dex: String,
    #[serde(default)]
    pub pool: String,
    #[serde(default)]
    pub mint: String,
    #[serde(default)]
    pub quote_mint: String,
    #[serde(default)]
    pub trader: String,
    #[serde(default)]
    pub side: String,
    #[serde(default, deserialize_with = "de_u64")]
    pub base_amount: u64,
    #[serde(default, deserialize_with = "de_u64")]
    pub quote_amount: u64,
    #[serde(default, deserialize_with = "de_f64")]
    pub price_usd: f64,
    #[serde(default, deserialize_with = "de_f64")]
    pub volume_usd: f64,
    #[serde(default, deserialize_with = "de_f64")]
    pub price_impact_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCreate {
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub slot: u64,
    #[serde(default)]
    pub block_time: i64,
    #[serde(default)]
    pub dex: String,
    #[serde(default)]
    pub mint: String,
    #[serde(default)]
    pub pool: String,
    #[serde(default)]
    pub quote_mint: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meme {
    #[serde(default)]
    pub mint: String,
    #[serde(default)]
    pub launchpad: String,
    #[serde(default)]
    pub creator: Option<String>,
    #[serde(default)]
    pub created_time: i64,
    #[serde(default)]
    pub graduated: bool,
    #[serde(default, deserialize_with = "de_f64")]
    pub progress_pct: f64,
    #[serde(default, deserialize_with = "de_f64")]
    pub price_usd: f64,
    #[serde(default)]
    pub block_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graduation {
    #[serde(default)]
    pub mint: String,
    #[serde(default)]
    pub launchpad: String,
    #[serde(default)]
    pub pool: String,
    #[serde(default)]
    pub dex: String,
    #[serde(default)]
    pub slot: u64,
    #[serde(default)]
    pub block_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Surge {
    #[serde(default)]
    pub mint: String,
    #[serde(default)]
    pub trigger_time: i64,
    #[serde(default, deserialize_with = "de_f64")]
    pub mcap_at_trigger: f64,
    #[serde(default, deserialize_with = "de_f64")]
    pub price_at_trigger: f64,
    #[serde(default, deserialize_with = "de_f64")]
    pub volume_window_usd: f64,
    #[serde(default, deserialize_with = "de_f64")]
    pub multiple: f64,
    #[serde(default)]
    pub trades: u64,
    #[serde(default)]
    pub window_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(default)]
    pub mint: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub decimals: Option<u32>,
    #[serde(default)]
    pub logo: Option<String>,
}

/// A decoded Blur market-data event.
#[derive(Debug, Clone)]
pub enum DexEvent {
    Swap(Swap),
    TokenCreate(TokenCreate),
    PoolCreate(TokenCreate),
    Meme(Meme),
    Graduation(Graduation),
    Surge(Surge),
    Metadata(Metadata),
    /// Anything else (liquidity, transfer, candle, stats, ...), kept raw.
    Other(Value),
}

impl DexEvent {
    pub fn parse(raw: &str) -> DexEvent {
        match serde_json::from_str::<Value>(raw) {
            Ok(v) => DexEvent::from_value(v),
            Err(_) => DexEvent::Other(Value::Null),
        }
    }

    /// Parse with the event type from the transport envelope. Blur's gRPC
    /// frames carry the type in `SubscribeUpdateBlur.event_type`, but the JSON
    /// payload itself often omits the `type` field (verified on mainnet for
    /// swaps) — inject it when missing.
    pub fn parse_typed(raw: &str, hint: &str) -> DexEvent {
        match serde_json::from_str::<Value>(raw) {
            Ok(mut v) => {
                if let Value::Object(ref mut m) = v {
                    if !m.contains_key("type") && !hint.is_empty() {
                        m.insert("type".into(), Value::String(hint.to_owned()));
                    }
                }
                DexEvent::from_value(v)
            }
            Err(_) => DexEvent::Other(Value::Null),
        }
    }

    pub fn from_value(v: Value) -> DexEvent {
        let ty = {
            let t = v
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .to_owned();
            // Blur's WS/gRPC payloads often omit `type` (verified on mainnet
            // for swaps) — infer it from the field shape.
            if t.is_empty() { infer_type(&v).to_owned() } else { t }
        };
        macro_rules! typed {
            ($variant:ident, $t:ty) => {
                match serde_json::from_value::<$t>(v.clone()) {
                    Ok(x) => DexEvent::$variant(x),
                    Err(_) => DexEvent::Other(v),
                }
            };
        }
        match ty.as_str() {
            "swap" => typed!(Swap, Swap),
            "token_create" => typed!(TokenCreate, TokenCreate),
            "pool_create" => typed!(PoolCreate, TokenCreate),
            "meme" => typed!(Meme, Meme),
            "graduation" => typed!(Graduation, Graduation),
            "surge" | "radar" => typed!(Surge, Surge),
            "metadata" => typed!(Metadata, Metadata),
            _ => DexEvent::Other(v),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            DexEvent::Swap(_) => "swap",
            DexEvent::TokenCreate(_) => "token_create",
            DexEvent::PoolCreate(_) => "pool_create",
            DexEvent::Meme(_) => "meme",
            DexEvent::Graduation(_) => "graduation",
            DexEvent::Surge(_) => "surge",
            DexEvent::Metadata(_) => "metadata",
            DexEvent::Other(_) => "other",
        }
    }

    pub fn mint(&self) -> Option<&str> {
        match self {
            DexEvent::Swap(s) => Some(s.mint.as_str()),
            DexEvent::TokenCreate(t) | DexEvent::PoolCreate(t) => Some(t.mint.as_str()),
            DexEvent::Meme(m) => Some(m.mint.as_str()),
            DexEvent::Graduation(g) => Some(g.mint.as_str()),
            DexEvent::Surge(s) => Some(s.mint.as_str()),
            DexEvent::Metadata(m) => Some(m.mint.as_str()),
            DexEvent::Other(v) => v.get("mint").and_then(|m| m.as_str()),
        }
        .filter(|m| !m.is_empty())
    }

    pub fn block_time(&self) -> i64 {
        match self {
            DexEvent::Swap(s) => s.block_time,
            DexEvent::TokenCreate(t) | DexEvent::PoolCreate(t) => t.block_time,
            DexEvent::Meme(m) => m.block_time,
            DexEvent::Graduation(g) => g.block_time,
            DexEvent::Surge(s) => s.trigger_time,
            DexEvent::Metadata(_) => 0,
            DexEvent::Other(v) => v
                .get("block_time")
                .and_then(|t| t.as_i64())
                .unwrap_or_default(),
        }
    }
}

impl Serialize for DexEvent {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        macro_rules! flat {
            ($inner:expr, $kind:expr) => {{
                let mut v = serde_json::to_value($inner).map_err(serde::ser::Error::custom)?;
                if let Value::Object(ref mut m) = v {
                    m.insert("type".into(), Value::String($kind.into()));
                }
                v.serialize(s)
            }};
        }
        match self {
            DexEvent::Swap(x) => flat!(x, "swap"),
            DexEvent::TokenCreate(x) => flat!(x, "token_create"),
            DexEvent::PoolCreate(x) => flat!(x, "pool_create"),
            DexEvent::Meme(x) => flat!(x, "meme"),
            DexEvent::Graduation(x) => flat!(x, "graduation"),
            DexEvent::Surge(x) => flat!(x, "surge"),
            DexEvent::Metadata(x) => flat!(x, "metadata"),
            DexEvent::Other(v) => v.serialize(s),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Signal {
    pub id: String,
    pub mint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub dex: String,
    pub trigger: String,
    pub trigger_wallets: Vec<String>,
    pub price_usd: f64,
    pub buy_volume_usd: f64,
    pub sell_volume_usd: f64,
    pub buy_count: u32,
    pub sell_count: u32,
    pub unique_traders: usize,
    /// 0 = looks clean, 100 = almost certainly a rug.
    pub risk_score: f64,
    pub risk_factors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top10_holder_pct: Option<f64>,
    pub created_at: i64,
}

/// A smart-money buy detected natively on-chain via the Yellowstone gRPC
/// wallet tracker (token-balance deltas), independent of Blur decoding.
#[derive(Debug, Clone, Serialize)]
pub struct WalletBuy {
    pub wallet: String,
    pub mint: String,
    /// Estimated SOL spent (SOL + WSOL outflow; 0 when the quote was USDC/USDT).
    pub sol_spent: f64,
    pub slot: u64,
    pub signature: String,
    /// DEX name inferred from the transaction's program ids, "" if unknown.
    pub dex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeRecord {
    pub signal_id: String,
    pub mint: String,
    pub mode: String,
    pub sol_amount: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landed_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Realized PnL in SOL — set on sell records (dry_sell / live_sell).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realized_pnl_sol: Option<f64>,
    /// Exit reason for sell records: take_profit | stop_loss | time_stop | price_unknown | sell_failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub at: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub started_at: i64,
    pub events_total: u64,
    pub swaps_total: u64,
    pub token_creates_total: u64,
    pub pool_creates_total: u64,
    pub graduations_total: u64,
    pub volume_usd_total: f64,
    pub signals_total: u64,
    pub trades_total: u64,
    pub smart_money_count: usize,
    pub tracked_tokens: usize,
    pub events_per_min: f64,
    pub stream_connected: bool,
    pub stream_reconnects: u64,
    pub last_event_slot: u64,
    pub beam_last_latency_ms: Option<u64>,
    pub grpc_wallet_buys_total: u64,
    pub grpc_track_connected: bool,
    pub grpc_track_reconnects: u64,
    /// Signals dropped by the volume / size filters before emission.
    pub signals_filtered_total: u64,
}

/// One row of the token board.
#[derive(Debug, Clone, Serialize)]
pub struct TokenInfo {
    pub mint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub dex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    pub price_usd: f64,
    pub buy_volume_usd: f64,
    pub sell_volume_usd: f64,
    pub buy_count: u32,
    pub sell_count: u32,
    #[serde(skip)]
    pub traders: std::collections::HashSet<String>,
    pub graduated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_pct: Option<f64>,
    pub last_activity: i64,
}

impl TokenInfo {
    pub fn new(mint: &str) -> Self {
        TokenInfo {
            mint: mint.to_owned(),
            name: None,
            symbol: None,
            dex: String::new(),
            pool: None,
            creator: None,
            created_at: None,
            price_usd: 0.0,
            buy_volume_usd: 0.0,
            sell_volume_usd: 0.0,
            buy_count: 0,
            sell_count: 0,
            traders: Default::default(),
            graduated: false,
            progress_pct: None,
            last_activity: 0,
        }
    }

    pub fn unique_traders(&self) -> usize {
        self.traders.len()
    }
}

/// Serialize a `TokenInfo` together with its unique trader count.
pub fn token_json(t: &TokenInfo) -> Value {
    let mut v = serde_json::to_value(t).unwrap_or(Value::Null);
    if let Value::Object(ref mut m) = v {
        m.insert(
            "unique_traders".into(),
            Value::Number(t.unique_traders().into()),
        );
    }
    v
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// Helper for building ad-hoc JSON objects without pulling in a macro crate.
pub fn json_map(pairs: impl IntoIterator<Item = (String, Value)>) -> Value {
    Value::Object(Map::from_iter(pairs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn de_f64_accepts_numbers_and_strings() {
        assert_eq!(de_f64(json!(1.5)).unwrap(), 1.5);
        assert_eq!(de_f64(json!(2)).unwrap(), 2.0);
        assert_eq!(de_f64(json!("1.5")).unwrap(), 1.5);
        assert_eq!(de_f64(json!("  2.25  ")).unwrap(), 2.25); // trimmed
    }

    #[test]
    fn de_f64_null_and_empty_are_zero() {
        assert_eq!(de_f64(Value::Null).unwrap(), 0.0);
        assert_eq!(de_f64(json!("")).unwrap(), 0.0);
        assert_eq!(de_f64(json!("   ")).unwrap(), 0.0);

        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "de_f64")]
            v: f64,
        }
        let t: T = serde_json::from_str("{}").unwrap();
        assert_eq!(t.v, 0.0);
    }

    #[test]
    fn de_f64_rejects_non_decimals() {
        assert!(de_f64(json!("abc")).is_err());
        assert!(de_f64(json!(true)).is_err());
        assert!(de_f64(json!([1.5])).is_err());
    }

    #[test]
    fn de_u64_accepts_numbers_and_strings() {
        assert_eq!(de_u64(json!(42)).unwrap(), 42);
        assert_eq!(de_u64(json!("42")).unwrap(), 42);
        assert_eq!(de_u64(json!("  7  ")).unwrap(), 7);
        assert_eq!(de_u64(json!(u64::MAX)).unwrap(), u64::MAX);
    }

    #[test]
    fn de_u64_null_is_zero() {
        assert_eq!(de_u64(Value::Null).unwrap(), 0);

        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "de_u64")]
            v: u64,
        }
        let t: T = serde_json::from_str("{}").unwrap();
        assert_eq!(t.v, 0);
    }

    #[test]
    fn de_u64_rejects_non_integers() {
        assert!(de_u64(json!("abc")).is_err());
        assert!(de_u64(json!(true)).is_err());
        assert!(de_u64(json!([1])).is_err());
    }

    #[test]
    fn infer_type_recognizes_each_event_shape() {
        assert_eq!(
            infer_type(&json!({"side": "buy", "trader": "t", "signature": "s"})),
            "swap"
        );
        assert_eq!(
            infer_type(&json!({"launchpad": "pumpfun", "progress_pct": "50"})),
            "meme"
        );
        assert_eq!(
            infer_type(&json!({"multiple": "4", "baseline_usd": "1000"})),
            "surge"
        );
        assert_eq!(infer_type(&json!({"creator": "c", "uri": "u"})), "token_create");
        assert_eq!(infer_type(&json!({"pool": "p", "quote_mint": "q"})), "pool_create");
        assert_eq!(
            infer_type(&json!({"interval": "1m", "open": "1", "close": "2"})),
            "candle"
        );
        assert_eq!(infer_type(&json!({})), "other");
        assert_eq!(infer_type(&json!({"mint": "m"})), "other");
    }

    #[test]
    fn infer_type_ignores_null_fields_and_prefers_swap() {
        // Null values don't count as present.
        assert_eq!(
            infer_type(&json!({"side": null, "trader": "t", "signature": "s"})),
            "other"
        );
        // Swap wins when several shapes overlap.
        assert_eq!(
            infer_type(&json!({
                "side": "buy", "trader": "t", "signature": "s",
                "launchpad": "pumpfun", "progress_pct": "50"
            })),
            "swap"
        );
    }

    #[test]
    fn parse_typed_uses_hint_and_infers_shape() {
        // Hint injected when the payload omits `type`.
        let ev = DexEvent::parse_typed(r#"{"mint":"m","side":"buy"}"#, "swap");
        assert!(matches!(ev, DexEvent::Swap(_)));
        // No hint: shape inference kicks in.
        let ev = DexEvent::parse(r#"{"side":"buy","trader":"t","signature":"s","mint":"m"}"#);
        assert!(matches!(ev, DexEvent::Swap(_)));
        // Garbage stays garbage.
        assert!(matches!(DexEvent::parse("not json"), DexEvent::Other(Value::Null)));
    }
}
