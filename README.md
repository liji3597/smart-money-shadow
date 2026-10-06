# Smart-Money Shadow

Real-time smart-money tracking and copy-trading engine for Solana, built
entirely on [Solami](https://solami.dev) infrastructure.

Watches every decoded DEX trade the moment it lands, natively tails the
on-chain transactions of a living set of smart-money wallets, scores each
token for rug risk, filters the noise, and — when you flip the switch —
copies their buys on pump.fun / PumpSwap through Solami Beam. Ships with a
live dashboard that also tells you whether the signals actually made money.

Built for the Superteam Earn bounty
[Build Something Live on Solana Data](https://superteam.fun/earn/listing/build-something-live-on-solana-data).

## How it uses Solami

| Product | What it does here |
| --- | --- |
| **Blur** (WebSocket firehose) | `swap` / `token_create` / `pool_create` / `meme` / `graduation` / `surge` / `metadata` events, decoded server-side (~5,000 events/min on mainnet) |
| **Yellowstone gRPC** | native transaction subscriptions on the tracked wallet set — catches smart-money buys on DEXes Blur doesn't decode; filters are hot-reloaded in place when the wallet set refreshes |
| **Data REST** (`api.solami.dev/data`) | trader leaderboard → smart-money discovery · token prices → signal PnL backfill (1h/24h win rates) · token metadata → signal names |
| **Extended RPC** | `getTokenLargestAccountsV2` + `getTokenSupply` → top-10 holder concentration risk score for every signal · bonding-curve / pool state reads for swap quotes |
| **Beam** | transaction landing: copy-trade execution (pump.fun / PumpSwap buy instructions built from on-chain curve/pool state) and a self-transfer health check with latency metrics |

Notes from building against the platform (documented for fellow builders):

- The screener-style REST routes (`/data/token/launches`, `/graduated`, …)
  treat every query parameter as a filter — authenticate with an
  `Authorization: Bearer` (or `x-api-key`) header there instead of an
  `api_key` query param.
- `getTokenLargestAccountsV2` answers with the classic largest-accounts
  shape (`result.value` is an array), not the keyed-accounts shape the
  SDK pager expects — call it raw.
- Blur WebSocket swap events carry no `type` field; infer the event kind
  from the field shape (or use the gRPC envelope's `event_type`).

## Architecture

```
 Blur WS ──▶ ingest ──▶ decoded market events ──────────────┐
                                                            │
 Yellowstone gRPC ──▶ wallet-track (smart wallets' txs) ────┤
                                                            ▼
 Data REST ──▶ smart-money discovery        ┌──────────── engine ───────────┐
 (leaderboard, 30 min refresh)              │ token board · buy/sell flow   │──▶ signals
                                            │ triggers:                     │      │
 Extended RPC ──▶ top-10 concentration ────▶│  · smart-money buy            │      ▼
               · swap quote state           │  · volume surge               │   trader
                                            │ noise filters:                │   pump.fun /
                                            │  · min window volume          │   PumpSwap buy
                                            │  · min SOL spent (gRPC path)  │   via Beam
                                            │  · quote-mint / blue-chip ban │   (dry-run
                                            │ rug-risk score 0-100          │   by default)
                                            └───────────────────────────────┘      │
                                                            │                      │
 Data REST ──▶ PnL backfill (1h/24h win rates)              ▼                      ▼
                                            axum REST + WebSocket ◀── trades.jsonl
                                                            │
                                                            ▼
                                                  Next.js dashboard
```

- `crates/core` — event/signal/metrics types. Blur returns fractional values
  as JSON decimal strings; the `de_*` deserializers handle both forms.
- `crates/ingest` — Blur WebSocket stream with capped exponential backoff;
  Yellowstone gRPC wallet tracking; Blur REST client.
- `crates/engine` — token board, signal engine, noise filters, rug-risk scorer.
- `crates/trader` — copy-trade executor: bonding-curve / AMM-pool quote math,
  swap instruction construction (pump.fun 18-account & PumpSwap 26-account
  layouts), dry-run by default, daily budget cap, JSONL audit log.
- `crates/api` — `shadow` binary: axum REST + WebSocket fanout, signal PnL
  tracker.
- `web/` — Next.js dashboard: the live trading desk at `/` (signals, token
  board, positions) and a trading-journal account page at `/account` (equity
  curve, win rate, profit factor, round trips with per-trade candle charts
  marking entry/exit).

## Quick start

Prereqs: Rust (1.97+), Node 20+ / pnpm, `protoc` (Protocol Buffers
compiler — on Windows grab the prebuilt binary or `pip install protoc-wheel-0`;
on macOS `brew install protobuf`), and a Solami API key — sign up at
https://solami.dev/signup (needs the **DataApi** permission for Blur streams).

> This repo carries a vendored copy of the `solami` crate under
> `vendor/solami` (enabled via `[patch.crates-io]`). Changes: its `build.rs`
> uses the `protoc` from `PROTOC`/PATH instead of compiling protobuf from
> source (keeps builds hermetic without cmake), and pubsub WebSocket connect
> failures degrade to a warning instead of failing `with_rpc` init.

```bash
cp .env.example .env      # paste your SOLAMI_API_KEY
cargo run -p shadow-api   # backend on http://127.0.0.1:8080

cd web && pnpm install && pnpm dev   # dashboard on http://localhost:3000
```

Point it at your own key: everything is read from `.env`, nothing is
hardcoded. See `.env.example` for every knob.

## Trading modes

**Dry-run (default).** Every signal records the trade it would have taken —
and for pump.fun / PumpSwap tokens it also logs a live quote (expected tokens
out, min out at your slippage setting) computed from the actual on-chain
curve/pool state. No funds required.

**Beam health check.** Set `BEAM_HEALTH_CHECK=true` and provide
`SOLAMI_TRADER_KEYPAIR` (payer, base58) plus `SOLAMI_SWQOS_KEYPAIR` (a
registered swQoS key — create one in the Solami dashboard). On startup the
backend lands a 1000-lamport self-transfer through Beam, waits for on-chain
confirmation, and reports the real landing latency in the status strip —
proof the write path works before real size goes through.

**Live copy-trading.** `LIVE_TRADING=true` + `SOLAMI_TRADER_KEYPAIR`. On each
signal from a pump.fun / PumpSwap token the trader builds the real buy
instruction set (create-ATA, swap with slippage-protected `min_out`,
priority fee, Beam tip) and lands it through Beam. Position size per signal
is `TRADE_SOL_PER_SIGNAL`, hard-capped per day by `MAX_DAILY_SOL`; signals on
other DEXes are recorded as skipped. Never trade with keys you cannot afford
to lose.

**Position management (both modes).** Every buy opens a tracked position.
An exit loop re-prices open positions every `POSITION_CHECK_SECS` and closes
them on take-profit (`TAKE_PROFIT_PCT`, default +50%), stop-loss
(`STOP_LOSS_PCT`, default -30%), or time stop (`MAX_HOLD_SECS`, default 24h).
In dry-run the sell settles at the entry→exit price ratio; live mode builds
the matching sell instruction (pump.fun sell, or PumpSwap sell when the
curve has graduated) sized from the actual ATA balance and lands it through
Beam. One open position per mint — repeat signals on a held token are
skipped. Open and closed positions are on the dashboard and at
`/api/positions`.

## Proven on mainnet

The full loop — signal → real buy → exit sell — ran with real funds on
Solana mainnet (2026-10-03/04), from a fresh burner wallet funded with
0.098 SOL. **Five round trips, 3 wins / 2 losses, net +0.0144 SOL
(+14.4% on 0.10 SOL deployed)** — both exit paths fired for real:

| # | Mint | Buy | Sell | Reason | PnL |
| --- | --- | --- | --- | --- | --- |
| 1 | `9Qmz…pump` | [`3qMg6maf…DTwjz`](https://solscan.io/tx/3qMg6maf6kpMZmT4eydyfTxMM1Mu9LnBS9ZkzgVLN689HfGu2G7cfNLx6nBVCYXrjXDnxxyW2yHgvFptW2nDTwjz) | [`3yCp6N82…bzaz`](https://solscan.io/tx/3yCp6N82jjdBa4CtRUErobLwFza93iwzrcMDSZFHFpRMayGMek7bUaCjJM6Cszuywjwv8jYUPbSk1khGRK8sbzaz) | take_profit | +0.00175 SOL |
| 2 | `fCUB…pump` | [`5trT8T3a…mSbt`](https://solscan.io/tx/5trT8T3at66Q2Ah5NEgSrDWxu85NfeJNjx6BHhav5j6EHUiokTGJQP3Ck9qVPxsYSLAwF3GiSSdGYae6fSmqmSbt) | [`6BWNCb3o…5nt`](https://solscan.io/tx/6BWNCb3oBdAFsiSYQWB4fbXrRCpoeMpsmkzQJTKmKgfCG3x7s1KFA2LRqJWXwKgMv1KZV5sEeMd7fbkJS5fc5nt) | stop_loss | -0.00201 SOL |
| 3 | `CbyT…` | [`3Gst4546…ztn`](https://solscan.io/tx/3Gst4546Ar5JyWcjtA42tXXXJiAyaFYCnJEJZ4JLEAtiUqqL6tJE8ooufCBXSKpjVDZziXwCsCr3exT7BWMMNztn) | [`ttuGx5JX…u3n`](https://solscan.io/tx/ttuGx5JXcY9X11VcDesGm1KRNwTgNjn5Tr8vsWwxZPFmzBX4Bq6u4smeVMZRx5nGUpA448XjQU6mLjhH546zu3n) | take_profit | +0.00071 SOL |
| 4 | `7bC8…` | [`2XVExWVX…rsD`](https://solscan.io/tx/2XVExWVXUUTcQMF5C8V7MQnC6nZnsFkGsMMNke6d1u6YzdaPCyrsgbzuUtJSK8d73YuyiHGbnTTxH4Mg9bXjmrsD) | [`3NJTBgc6…jDm`](https://solscan.io/tx/3NJTBgc6tcntHzAfHGFkoWgU6jGBybwiEDBFLrJABBnm2vNsP1PuvgHU3iqBp95cMBZYZttH8XX21gSd5szHujDm) | take_profit | +0.01591 SOL |
| 5 | `BXoH…` | [`4399483F…6v4`](https://solscan.io/tx/4399483F85M1EC98bsqX6F4RCjQ2hJBpjNGqguAoqKWxQFKP8FnfSgBAT9zMghPHk8jwuUajKe471KCv6PtnQ6v4) | [`5smVL8J5…3Y2`](https://solscan.io/tx/5smVL8J5wP8GxEwRAobgPv8HwhG2np9FjvQPBkuphmH3cisU5sEqPaceMkHSpCrjzHWkvug8NkxZU5AqjghQW3Y2) | stop_loss | -0.00196 SOL |

Every send is independently verifiable through Beam's own tracking endpoint
(`GET https://api.solami.dev/swqos/tx/{signature}` → `is_landed: true`),
and the backend never trusts a send: after pushing bytes over QUIC it polls
the signature status over Solami RPC (resending every 2 s, 30 s ceiling) so
`landed_ms` in the logs is real confirmation time, not relay-ack time.

Two integration lessons this surfaced, both handled in code:

- **Beam's QUIC identity is a registered swQoS key**, not the payer wallet —
  an unregistered keypair gets silently dropped at submission (the tx lookup
  returns 404). Configure `SOLAMI_SWQOS_KEYPAIR` separately from
  `SOLAMI_TRADER_KEYPAIR`.
- **QUIC sends are fire-and-forget** by design; confirmation must come from
  RPC polling.

## Signal quality

Raw firehose in, judgement out. On a typical mainnet hour the engine sees
~70k smart-wallet-adjacent events, filters ~93% of candidate signals
(`signals_filtered_total` in the metrics), and surfaces the rest. Filters:

- window buy volume below `MIN_SIGNAL_VOLUME_USD` → dropped
- gRPC-path buys below `MIN_SMART_BUY_SOL` → dropped
- quote currencies and blue chips (WSOL, USDC, USDT, USDS, cbBTC, WBTC, WETH)
  never signal — a smart wallet "buying" USDC is an exit leg, not alpha
- one signal per token per `SIGNAL_COOLDOWN_SECS`

Every signal is then tracked: 1h/24h PnL is backfilled from Data REST prices
and aggregated into win rates at `/api/performance`.

## API

```
GET /api/health                 liveness + stream state
GET /api/metrics                events/min, volumes, counts, filter stats, Beam latency
GET /api/signals?limit=50       newest first, with PnL badges once measured
GET /api/performance            signal win rates (1h/24h), best/worst
GET /api/tokens?limit=100       token board, most recently active first
GET /api/tokens/{mint}
GET /api/smart-money            tracked wallet set
GET /api/trades?limit=50        dry-run / live trade records (restored from trades.jsonl on boot)
GET /api/positions              open positions (live PnL) + recently closed
GET /api/ohlcv?mint=&interval=&limit=  candle history (server-side Blur REST proxy, key never leaves the backend)
WS  /ws                         snapshot on connect, then event/signal/trade/position frames
```

## Roadmap

See [ROADMAP.md](ROADMAP.md).

## License

MIT
