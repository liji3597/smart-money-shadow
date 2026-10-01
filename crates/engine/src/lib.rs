//! The signal engine: consumes the Blur event stream, maintains the token
//! board, detects smart-money buys and volume surges, scores rug risk via
//! Solami's extended RPC, and emits `Signal`s.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use dashmap::DashMap;
use tokio::sync::{broadcast, watch};
use tracing::{info, warn};

use shadow_core::{now_unix, DexEvent, Metrics, Signal, TokenInfo, WalletBuy};
use shadow_ingest::BlurRest;

const MAX_SIGNALS: usize = 500;
const RATE_WINDOW_CAP: usize = 5000;

/// Quote currencies and blue chips: a smart-money wallet "buying" SOL/USDC
/// is usually just the exit leg of a swap, not alpha. Never signal on these.
const NO_SIGNAL_MINTS: &[&str] = &[
    "So11111111111111111111111111111111111111112",  // WSOL
    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", // USDC
    "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB", // USDT
    "USDSwr9ApdHk5bvJKMjzff41FfuX8bSxdKcR81vTwcA",  // USDS
    "cbbtcf3aa214zXHbiAZQwf4122FBYbraNdFqgw4iMij",  // cbBTC
    "3NZ9JMVBmGAqocybic2c7LQCJScmgsAZ6vQqTDzcqmJh", // WBTC
    "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs", // WETH (Wormhole)
];

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub min_smart_buy_usd: f64,
    /// Minimum cumulative buy volume (USD) on the token before any signal
    /// fires; also applied to a surge's `volume_window_usd`.
    pub min_signal_volume_usd: f64,
    /// Minimum SOL spent for a gRPC wallet-track buy to signal (that path has
    /// no USD valuation at detection time).
    pub min_smart_buy_sol: f64,
    pub signal_cooldown_secs: i64,
    pub surge_min_multiple: f64,
    pub smart_discovery: bool,
    pub discovery_interval_secs: u64,
    pub max_tracked_tokens: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            min_smart_buy_usd: 500.0,
            min_signal_volume_usd: 5_000.0,
            min_smart_buy_sol: 0.5,
            signal_cooldown_secs: 900,
            surge_min_multiple: 4.0,
            smart_discovery: true,
            discovery_interval_secs: 1800,
            max_tracked_tokens: 5000,
        }
    }
}

pub struct EngineState {
    pub metrics: RwLock<Metrics>,
    pub tokens: DashMap<String, TokenInfo>,
    pub signals: RwLock<VecDeque<Arc<Signal>>>,
    pub smart: RwLock<HashSet<String>>,
    /// Publishes the current smart-money set to the gRPC wallet tracker.
    pub smart_tx: watch::Sender<Arc<HashSet<String>>>,
    /// block_times of recent events, for the events/min rate.
    pub recent_event_times: RwLock<VecDeque<i64>>,
    pub last_signal_at: DashMap<String, i64>,
    pub signal_tx: broadcast::Sender<Arc<Signal>>,
    /// Last Beam landing latency in ms (0 = never measured). Owned by the trader.
    pub beam_latency_ms: Arc<AtomicU64>,
}

pub struct Engine {
    pub state: Arc<EngineState>,
    cfg: EngineConfig,
    /// Solami RPC kit — standard methods via deref plus the `*V2` index calls.
    rpc: solami::Solami<solami::On<solami::RpcKit>, solami::Off, solami::Off>,
    rest: BlurRest,
    signal_seq: AtomicU64,
}

impl Engine {
    pub async fn new(
        cfg: EngineConfig,
        api_key: &str,
        smart_seeds: Vec<String>,
        rest: BlurRest,
    ) -> Result<Arc<Self>> {
        let rpc = solami::builder().with_rpc(api_key).build().await?;
        let (signal_tx, _) = broadcast::channel(512);
        let mut smart = HashSet::new();
        for s in smart_seeds {
            let s = s.trim();
            if !s.is_empty() {
                smart.insert(s.to_owned());
            }
        }
        let (smart_tx, _) = watch::channel(Arc::new(smart.clone()));
        let state = Arc::new(EngineState {
            metrics: RwLock::new(Metrics {
                started_at: now_unix(),
                ..Default::default()
            }),
            tokens: DashMap::new(),
            signals: RwLock::new(VecDeque::new()),
            smart: RwLock::new(smart),
            smart_tx,
            recent_event_times: RwLock::new(VecDeque::new()),
            last_signal_at: DashMap::new(),
            signal_tx,
            beam_latency_ms: Arc::new(AtomicU64::new(0)),
        });
        Ok(Arc::new(Engine {
            state,
            cfg,
            rpc,
            rest,
            signal_seq: AtomicU64::new(0),
        }))
    }

    /// Consume events forever. Also spawns the smart-money discovery loop.
    pub async fn run(self: Arc<Self>, mut rx: broadcast::Receiver<Arc<DexEvent>>) {
        if self.cfg.smart_discovery {
            tokio::spawn(self.clone().discovery_loop());
        }
        loop {
            match rx.recv().await {
                Ok(ev) => self.handle_event(&ev),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "engine lagged behind ingest")
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }

    /// Consume gRPC wallet-track buys forever.
    pub async fn run_track(self: Arc<Self>, mut rx: broadcast::Receiver<Arc<WalletBuy>>) {
        loop {
            match rx.recv().await {
                Ok(buy) => self.handle_wallet_buy(&buy),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "engine lagged behind wallet-track")
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }

    fn handle_wallet_buy(self: &Arc<Self>, buy: &WalletBuy) {
        self.state.metrics.write().unwrap().grpc_wallet_buys_total += 1;
        let price = {
            let mut t = self
                .state
                .tokens
                .entry(buy.mint.clone())
                .or_insert_with(|| TokenInfo::new(&buy.mint));
            t.last_activity = now_unix();
            t.traders.insert(buy.wallet.clone());
            if !buy.dex.is_empty() {
                t.dex = buy.dex.clone();
            }
            t.price_usd
        };
        self.evict_tokens_if_full();
        // Same weight as the Blur path; cooldown dedup happens in maybe_signal.
        if self.state.smart.read().unwrap().contains(&buy.wallet) {
            if buy.sol_spent < self.cfg.min_smart_buy_sol {
                self.state.metrics.write().unwrap().signals_filtered_total += 1;
                return;
            }
            self.maybe_signal(
                &buy.mint,
                "smart_money_buy",
                vec![buy.wallet.clone()],
                price,
                &buy.dex,
            );
        }
    }

    async fn discovery_loop(self: Arc<Self>) {
        loop {
            match self.rest.top_trader_wallets(50).await {
                Ok(wallets) if !wallets.is_empty() => {
                    let snapshot = {
                        let mut smart = self.state.smart.write().unwrap();
                        for w in wallets {
                            smart.insert(w);
                        }
                        info!(total = smart.len(), "smart-money set refreshed");
                        Arc::new(smart.clone())
                    };
                    let _ = self.state.smart_tx.send(snapshot);
                }
                Ok(_) => warn!("trader leaderboard returned no wallets"),
                Err(e) => warn!(error = %e, "smart-money discovery failed"),
            }
            tokio::time::sleep(Duration::from_secs(self.cfg.discovery_interval_secs)).await;
        }
    }

    fn handle_event(self: &Arc<Self>, ev: &DexEvent) {
        {
            let mut m = self.state.metrics.write().unwrap();
            m.events_total += 1;
            match ev {
                DexEvent::Swap(_) => m.swaps_total += 1,
                DexEvent::TokenCreate(_) => m.token_creates_total += 1,
                DexEvent::PoolCreate(_) => m.pool_creates_total += 1,
                DexEvent::Graduation(_) => m.graduations_total += 1,
                _ => {}
            }
        }
        {
            let mut times = self.state.recent_event_times.write().unwrap();
            times.push_back(now_unix());
            while times.len() > RATE_WINDOW_CAP {
                times.pop_front();
            }
        }

        match ev {
            DexEvent::Swap(s) => {
                if s.mint.is_empty() {
                    return;
                }
                {
                    let mut m = self.state.metrics.write().unwrap();
                    m.volume_usd_total += s.volume_usd;
                }
                {
                    let mut t = self
                        .state
                        .tokens
                        .entry(s.mint.clone())
                        .or_insert_with(|| TokenInfo::new(&s.mint));
                    if !s.dex.is_empty() {
                        t.dex = s.dex.clone();
                    }
                    if !s.pool.is_empty() {
                        t.pool = Some(s.pool.clone());
                    }
                    t.price_usd = s.price_usd;
                    t.last_activity = s.block_time;
                    if !s.trader.is_empty() {
                        t.traders.insert(s.trader.clone());
                    }
                    if s.side == "buy" {
                        t.buy_volume_usd += s.volume_usd;
                        t.buy_count += 1;
                    } else {
                        t.sell_volume_usd += s.volume_usd;
                        t.sell_count += 1;
                    }
                }
                self.evict_tokens_if_full();

                let is_smart = self.state.smart.read().unwrap().contains(&s.trader);
                if is_smart && s.side == "buy" && s.volume_usd >= self.cfg.min_smart_buy_usd {
                    let window_buy_usd = self
                        .state
                        .tokens
                        .get(&s.mint)
                        .map(|t| t.buy_volume_usd)
                        .unwrap_or(0.0);
                    if window_buy_usd < self.cfg.min_signal_volume_usd {
                        self.state.metrics.write().unwrap().signals_filtered_total += 1;
                        return;
                    }
                    self.maybe_signal(
                        &s.mint,
                        "smart_money_buy",
                        vec![s.trader.clone()],
                        s.price_usd,
                        &s.dex,
                    );
                }
            }
            DexEvent::TokenCreate(t) | DexEvent::PoolCreate(t) => {
                let mut e = self
                    .state
                    .tokens
                    .entry(t.mint.clone())
                    .or_insert_with(|| TokenInfo::new(&t.mint));
                e.dex = t.dex.clone();
                if !t.pool.is_empty() {
                    e.pool = Some(t.pool.clone());
                }
                if e.created_at.is_none() {
                    e.created_at = Some(t.block_time);
                }
                if let DexEvent::TokenCreate(_) = ev {
                    if t.name.is_some() {
                        e.name = t.name.clone();
                    }
                    if t.symbol.is_some() {
                        e.symbol = t.symbol.clone();
                    }
                    if t.creator.is_some() {
                        e.creator = t.creator.clone();
                    }
                }
            }
            DexEvent::Meme(m) => {
                let mut e = self
                    .state
                    .tokens
                    .entry(m.mint.clone())
                    .or_insert_with(|| TokenInfo::new(&m.mint));
                e.progress_pct = Some(m.progress_pct);
                if m.price_usd > 0.0 {
                    e.price_usd = m.price_usd;
                }
                e.graduated = m.graduated;
                e.last_activity = m.block_time;
            }
            DexEvent::Graduation(g) => {
                let mut e = self
                    .state
                    .tokens
                    .entry(g.mint.clone())
                    .or_insert_with(|| TokenInfo::new(&g.mint));
                e.graduated = true;
                if !g.pool.is_empty() {
                    e.pool = Some(g.pool.clone());
                }
                if !g.dex.is_empty() {
                    e.dex = g.dex.clone();
                }
            }
            DexEvent::Surge(s) => {
                if s.multiple >= self.cfg.surge_min_multiple {
                    if s.volume_window_usd < self.cfg.min_signal_volume_usd {
                        self.state.metrics.write().unwrap().signals_filtered_total += 1;
                        return;
                    }
                    self.maybe_signal(
                        &s.mint,
                        "volume_surge",
                        vec![],
                        s.price_at_trigger,
                        "",
                    );
                }
            }
            DexEvent::Metadata(m) => {
                if m.mint.is_empty() {
                    return;
                }
                let mut t = self
                    .state
                    .tokens
                    .entry(m.mint.clone())
                    .or_insert_with(|| TokenInfo::new(&m.mint));
                if m.name.is_some() {
                    t.name = m.name.clone();
                }
                if m.symbol.is_some() {
                    t.symbol = m.symbol.clone();
                }
            }
            DexEvent::Other(_) => {}
        }
    }

    fn evict_tokens_if_full(&self) {
        if self.state.tokens.len() <= self.cfg.max_tracked_tokens {
            return;
        }
        let oldest = self
            .state
            .tokens
            .iter()
            .min_by_key(|e| e.last_activity)
            .map(|e| e.key().clone());
        if let Some(k) = oldest {
            self.state.tokens.remove(&k);
        }
    }

    fn maybe_signal(
        self: &Arc<Self>,
        mint: &str,
        trigger: &str,
        wallets: Vec<String>,
        price: f64,
        dex: &str,
    ) {
        if NO_SIGNAL_MINTS.contains(&mint) {
            return;
        }
        let now = now_unix();
        if let Some(last) = self.state.last_signal_at.get(mint) {
            if now - *last < self.cfg.signal_cooldown_secs {
                return;
            }
        }
        self.state.last_signal_at.insert(mint.to_owned(), now);

        let engine = Arc::clone(self);
        let mint = mint.to_owned();
        let trigger = trigger.to_owned();
        let dex = dex.to_owned();
        tokio::spawn(async move {
            engine.emit_signal(mint, trigger, wallets, price, dex).await;
        });
    }

    async fn emit_signal(
        self: &Arc<Self>,
        mint: String,
        trigger: String,
        wallets: Vec<String>,
        price: f64,
        dex: String,
    ) {
        // The gRPC wallet-track path has no USD price; fall back to Blur REST
        // so the signal (and its PnL tracking) still gets an entry price.
        let price = if price > 0.0 {
            price
        } else {
            match self.rest.token_price_usd(&mint).await {
                Ok(Some(p)) if p > 0.0 => p,
                _ => price,
            }
        };
        let concentration = self.top10_concentration(&mint).await;

        let (symbol, name, buy_v, sell_v, buy_c, sell_c, traders, created_at) = {
            match self.state.tokens.get(&mint) {
                Some(t) => (
                    t.symbol.clone(),
                    t.name.clone(),
                    t.buy_volume_usd,
                    t.sell_volume_usd,
                    t.buy_count,
                    t.sell_count,
                    t.unique_traders(),
                    t.created_at,
                ),
                None => (None, None, 0.0, 0.0, 0, 0, 0, None),
            }
        };

        // The Blur metadata event only covers new tokens; for established ones
        // backfill symbol/name once via REST so signals show real names.
        let (symbol, name) = if symbol.is_none() {
            match self.rest.token_metadata(&mint).await {
                Ok(Some((sym, nam))) => {
                    if let Some(mut t) = self.state.tokens.get_mut(&mint) {
                        if t.symbol.is_none() {
                            t.symbol = sym.clone();
                        }
                        if t.name.is_none() {
                            t.name = nam.clone();
                        }
                    }
                    (sym, nam)
                }
                _ => (symbol, name),
            }
        } else {
            (symbol, name)
        };

        let mut score = 0.0_f64;
        let mut factors: Vec<String> = Vec::new();

        match concentration {
            Ok(Some(pct)) => {
                if pct > 50.0 {
                    score += 40.0;
                    factors.push(format!("top-10 holders own {pct:.1}% of supply"));
                } else if pct > 30.0 {
                    score += 25.0;
                    factors.push(format!("top-10 holders own {pct:.1}% of supply"));
                } else if pct > 15.0 {
                    score += 10.0;
                    factors.push(format!("top-10 holders own {pct:.1}% of supply"));
                }
            }
            Ok(None) => factors.push("holder concentration unavailable".into()),
            Err(ref e) => {
                warn!(mint = %mint, error = %e, "concentration lookup failed");
                factors.push("holder concentration lookup failed".into());
            }
        }
        if sell_v > buy_v && sell_v > 0.0 {
            score += 20.0;
            factors.push("sell pressure exceeds buy pressure".into());
        }
        if let Some(created) = created_at {
            let age = now_unix() - created;
            if age < 600 {
                score += 10.0;
                factors.push(format!("token launched {age}s ago"));
            }
        }
        if traders > 0 && traders < 10 {
            score += 10.0;
            factors.push(format!("only {traders} unique traders observed"));
        }
        let score = score.min(100.0);

        let id = format!(
            "sig-{}-{}",
            now_unix(),
            self.signal_seq.fetch_add(1, Ordering::Relaxed)
        );
        let top10 = concentration.ok().flatten();
        let signal = Signal {
            id,
            mint: mint.clone(),
            symbol,
            name,
            dex,
            trigger,
            trigger_wallets: wallets,
            price_usd: price,
            buy_volume_usd: buy_v,
            sell_volume_usd: sell_v,
            buy_count: buy_c,
            sell_count: sell_c,
            unique_traders: traders,
            risk_score: score,
            risk_factors: factors,
            top10_holder_pct: top10,
            created_at: now_unix(),
        };
        info!(
            mint = %mint,
            risk = score,
            price = price,
            "signal: {}",
            signal.trigger
        );
        let signal = Arc::new(signal);
        {
            let mut signals = self.state.signals.write().unwrap();
            signals.push_front(signal.clone());
            while signals.len() > MAX_SIGNALS {
                signals.pop_back();
            }
        }
        self.state.metrics.write().unwrap().signals_total += 1;
        let _ = self.state.signal_tx.send(signal);
    }

    /// Top-10 holder concentration (% of supply) via Solami's
    /// `getTokenLargestAccountsV2` index plus `getTokenSupply`.
    ///
    /// Note: the server answers the V2 method with the classic
    /// largest-accounts shape (`result.value` is an array of
    /// `{address, amount, ...}`), not keyed accounts, so we call it raw
    /// through the deref'd solana client instead of the SDK's pager.
    async fn top10_concentration(&self, mint: &str) -> Result<Option<f64>> {
        let pk: solami::Pubkey = mint.parse().context("invalid mint pubkey")?;
        let supply = self.rpc.get_token_supply(&pk).await?;
        let supply_raw: f64 = supply
            .amount
            .parse()
            .context("unparsable supply amount")?;
        if supply_raw <= 0.0 {
            return Ok(None);
        }
        let resp: serde_json::Value = self
            .rpc
            .send(
                solana_client::rpc_request::RpcRequest::Custom {
                    method: "getTokenLargestAccountsV2",
                },
                serde_json::json!([mint, { "limit": 10 }]),
            )
            .await?;
        let accounts = resp["value"].as_array().cloned().unwrap_or_default();
        let top10: u64 = accounts
            .iter()
            .take(10)
            .filter_map(|a| a["amount"].as_str()?.parse::<u64>().ok())
            .fold(0u64, u64::saturating_add);
        if accounts.is_empty() {
            return Ok(None);
        }
        Ok(Some(top10 as f64 / supply_raw * 100.0))
    }
}
