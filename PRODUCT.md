# PRODUCT.md — Smart-Money Shadow

## What it is
Real-time Solana smart-money tracking + copy-trading engine. Rust backend, Next.js dashboard.
Built for the Superteam Earn × Solami bounty "Build Something Live on Solana Data".

## Audience
- Bounty judges (Solami team, Solana builders) evaluating Solami usage depth, build quality, usefulness.
- Crypto-native traders who live in terminals: pump.fun trenches, Dexscreener, Birdeye.

## Purpose of the dashboard
Convince in under 10 seconds that the system is live, dense with real mainnet signal, and
technically serious. It is a *proof instrument* first, a trading tool second.

## Operating context
- Watched on desktop, often for minutes at a time; glanceable from across the room during recording.
- Dark room / night usage is the norm for this audience.
- Data velocity is high (~5k events/min): the UI must absorb motion without becoming noise.

## Voice
Terse, technical, confident. Labels in English, uppercase micro-labels, no marketing copy,
no emojis, no exclamation marks. Numbers do the talking.

## Evidence
Every number on screen is real: Blur WS events, gRPC wallet tracking, on-chain-verified trades.
Never fabricate a metric; if data is missing show an honest placeholder ("—").
