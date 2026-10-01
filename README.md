# Smart-Money Shadow

Real-time smart-money tracking and copy-trading engine for Solana, built
entirely on [Solami](https://solami.dev) infrastructure.

Watches every decoded DEX trade the moment it confirms, tracks a living set of
smart-money wallets, scores each new token for rug risk, and — when you flip
the switch — copies their buys through Solami Beam. Ships with a live
terminal-style dashboard.

Built for the Superteam Earn bounty
[Build Something Live on Solana Data](https://superteam.fun/earn/listing/build-something-live-on-solana-data).

## How it uses Solami

| Product | What it does here |
| --- | --- |
| **Blur** (gRPC stream) | `swap` / `token_create` / `pool_create` / `meme` / `graduation` / `surge` events, decoded server-side, consumed through the [`solami`](https://crates.io/crates/solami) Rust SDK |
| **Blur REST** (`api.solami.dev/data`) | trader leaderboard → automatic smart-money discovery |
| **Extended RPC** | `getTokenLargestAccountsV2` + `getTokenSupply` → top-10 holder concentration for every signal |
| **Beam** | transaction landing: self-transfer health check with latency metrics; copy-trade execution path (dry-run by default) |

## Architecture

```
              ┌────────────────────────────────────────────┐
 Blur gRPC ──▶│ ingest   decoded events, reconnect+backoff │──▶ broadcast
              └────────────────────────────────────────────┘            │
                                                                        ▼
 Blur REST ──▶ smart-money discovery            ┌──────────────── engine ────────────────┐
 (leaderboard)                                  │ token board · buy/sell pressure        │──▶ signals
                                                │ signal triggers:                       │      │
 Extended RPC ──▶ top-10 holder concentration ─▶│  · smart-money buy ≥ $MIN_SMART_BUY    │      ▼
                                                │  · Blur surge ≥ SURGE_MIN_MULTIPLE     │   trader
                                                │ rug-risk score 0-100                   │   dry-run / Beam
                                                └────────────────────────────────────────┘      │
                                                                        │                       │
                                                                        ▼                       ▼
                                                              axum HTTP + WebSocket ◀── trades.jsonl
                                                                        │
                                                                        ▼
                                                              Next.js dashboard
```

- `crates/core` — event/signal/metrics types. Blur returns fractional values
  as JSON decimal strings; the `de_*` deserializers handle both forms.
- `crates/ingest` — Blur gRPC stream with capped exponential backoff; Blur REST client.
- `crates/engine` — token board, signal engine, rug-risk scorer.
- `crates/trader` — copy-trade executor. Dry-run by default, daily budget cap,
  JSONL audit log.
- `crates/api` — `shadow` binary: axum REST + WebSocket fanout.
- `web/` — Next.js dashboard.

## Quick start

Prereqs: Rust (recent stable), Node 20+ / pnpm, `protoc` (Protocol Buffers
compiler — on Windows grab the prebuilt binary or `pip install protoc-wheel-0`;
on macOS `brew install protobuf`), and a Solami API key — sign up at
https://solami.dev/signup (needs the **DataApi** permission for Blur streams).

> This repo carries a vendored copy of the `solami` crate under
> `vendor/solami` (enabled via `[patch.crates-io]`). The only change: its
> `build.rs` uses the `protoc` from `PROTOC`/PATH instead of compiling
> protobuf from source, which keeps builds hermetic on machines without cmake.

```bash
cp .env.example .env      # paste your SOLAMI_API_KEY
cargo run -p shadow-api   # backend on http://127.0.0.1:8080

cd web && pnpm install && pnpm dev   # dashboard on http://localhost:3000
```

Point it at your own key: everything is read from `.env`, nothing is
hardcoded. See `.env.example` for every knob.

## Trading modes

**Dry-run (default).** Every signal records the trade it would have taken —
mint, price, size in SOL — to `trades.jsonl` and the dashboard. No funds
required.

**Beam health check.** Set `BEAM_HEALTH_CHECK=true` and provide
`SOLAMI_TRADER_KEYPAIR` (base58). On startup the backend lands a
1000-lamport self-transfer through Beam and reports the landing latency in
the metrics bar — proof the write path works before real size goes through.

**Live copy-trading.** `LIVE_TRADING=true`. Position size per signal is
`TRADE_SOL_PER_SIGNAL`, hard-capped per day by `MAX_DAILY_SOL`. Swap
instruction construction (pump.fun / PumpSwap) is on the Phase-2 roadmap —
until then live mode records dry-run intents with the full budget logic
applied. Never trade with keys you cannot afford to lose.

## API

```
GET /api/health                 liveness + stream state
GET /api/metrics                events/min, volumes, counts, Beam latency
GET /api/signals?limit=50       newest first
GET /api/tokens?limit=100       token board, most recently active first
GET /api/tokens/{mint}
GET /api/smart-money            tracked wallet set
GET /api/trades?limit=50        dry-run / live trade records
WS  /ws                         snapshot on connect, then event/signal/trade frames
```

## Roadmap

See [ROADMAP.md](ROADMAP.md): Phase 2 adds real swap construction through
Beam, native Yellowstone gRPC wallet tracking with slot replay, and PnL
tracking; Phase 3 adds webhooks, Telegram alerts, and Docker packaging.

## License

MIT
