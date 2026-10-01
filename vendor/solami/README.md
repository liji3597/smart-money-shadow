# solami

Solana RPC, gRPC streams, and transaction landing in Rust.

```toml
[dependencies]
solami = "0.1"
```

## Quick start

Each subsystem is opt-in via the typestate builder. Methods only exist on the resulting client when their subsystem was enabled.

```rust
let mut client = solami::builder()
    .with_rpc(rpc_token)
    .with_grpc(grpc_token)
    .build()
    .await?;

let slot = client.get_slot().await?;
let block = client.rpc().get_block(slot).await?;

let (_sink, mut stream) = client
    .grpc()
    .subscribe_transactions(
        "pumpfun",
        vec!["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".into()],
        solami::CommitmentLevel::Processed,
    )
    .await?;
```

## SWQOS

Pass your SWQOS keypair to land transactions via QUIC. SWQOS works standalone - no RPC required.

```rust
let client = solami::builder()
    .with_swqos("base58-keypair")
    .build()
    .await?;

let tx = Transaction::new_signed_with_payer(
    &[
        system_instruction::transfer(&payer.pubkey(), &payer.pubkey(), 1000),
        solami::build_tip_ix(&payer.pubkey(), 0.0001),
    ],
    Some(&payer.pubkey()),
    &[&payer],
    blockhash,
);

let sig = client.land_transaction(&tx).await?;
```

## Region pinning

Pin RPC, gRPC, and SWQOS to a specific region (`Global`, `Ams`, `Fra`, `Nyc`).

```rust
use solami::Region;

let client = solami::builder()
    .with_rpc(rpc_token)
    .with_grpc(grpc_token)
    .with_swqos(swqos_key)
    .region(Region::Fra)
    .build()
    .await?;
```

Per-subsystem override:

```rust
let client = solami::builder()
    .with_rpc(rpc_token).rpc_region(Region::Ams)
    .with_grpc(grpc_token).grpc_region(Region::Fra)
    .with_swqos(swqos_key).beam_region(Region::Nyc)
    .build()
    .await?;
```

`beam()` first checks locally that the transaction carries a tip to a Solami tip account. That scan
reads only the transaction's static keys, so a tip account (or any earlier system transfer's
destination) loaded from an address lookup table looks like "no tip". Add `.skip_precheck()` to the
builder to skip the local scan and let the Beam server be the judge:

```rust
let client = solami::builder().with_beam(key).skip_precheck().build().await?;
```

## Custom gRPC filters

```rust
use solami::{SubscriptionBuilder, TxFilter, CommitmentLevel};

let request = SubscriptionBuilder::new()
    .commitment(CommitmentLevel::Processed)
    .transactions("my_filter", TxFilter {
        vote: Some(false),
        failed: Some(false),
        account_include: vec!["6EF8r...".into()],
        ..Default::default()
    })
    .build();

let (_sink, mut stream) = client.grpc().subscribe(request).await?;
```

## Deshred (pre-execution transactions)

Subscribe to transactions assembled from shreds before block execution.

```rust
let (_sink, mut stream) = client
    .grpc()
    .subscribe_deshred_transactions(
        "early",
        vec!["6EF8r...".into()],
    )
    .await?;
```

## Blur streams (market data over gRPC)

Every blur event kind rides one stream - `swap`, `liquidity`, `token_create`,
`pool_create`, `transfer`, `candle`, `stats`, `meme`, `graduation`, `metadata`.
Empty filter vectors mean "everything". Blur streams bill exactly like any other
gRPC stream (same plan allowance, same PAYG bandwidth metering).

```rust
use futures::StreamExt;

let mut client = solami::builder().with_grpc(key).build().await?;
let mut stream = client.grpc().subscribe_blur_events(vec!["token_create".into()]).await?;

while let Some(Ok(ev)) = stream.updates.next().await {
    println!("{} {} slot={}", ev.event_type, ev.mint, ev.slot);
    // ev.json is the full event payload, identical to the /data/subscribe websocket
}
```

Narrow by mint, or build the request yourself for pool/dex/trader/volume filters:

```rust
use solami::SubscribeBlurRequest;

let mut stream = client.grpc().subscribe_blur(SubscribeBlurRequest {
    event_type: vec!["swap".into()],
    mint: vec![mint.to_string()],
    min_volume_usd: Some(500.0),
    ..Default::default()
}).await?;
```

Filters can be replaced mid-stream without reconnecting - send a new request on
`stream.filters`.

## Webhook streams

Receive your webhook deliveries as a stream instead of an HTTP callback. Only
webhooks owned by the key are allowed; anything else is rejected with
`PermissionDenied`.

```rust
let mut stream = client.grpc().subscribe_webhooks(vec!["wh_abc123".into()]).await?;
while let Some(Ok(d)) = stream.updates.next().await {
    println!("{} {}", d.webhook_id, d.json);
}
```

## Extended RPC methods

Standard Solana methods come from `solana-client` and are available directly on
`RpcClient` (it derefs to the upstream client). These Solami-specific methods are
added on top:

| Method | SDK call |
|---|---|
| `getProgramAccountsV2` | `get_program_accounts_v2(program, cfg)` |
| `getTokenAccountsByOwnerV2` | `get_token_accounts_by_owner_v2(owner, cfg)` |
| `getTokenAccountsByDelegateV2` | `get_token_accounts_by_delegate_v2(delegate, cfg)` |
| `getTokenAccountsByMint` / `V2` | `get_token_accounts_by_mint_v1(mint, cfg)` / `get_token_accounts_by_mint_v2(mint, cfg)` |
| `getTokenLargestAccountsV2` | `get_token_largest_accounts_v2(mint, cfg)` |
| `getTransactionsForAddress` | `get_transactions_for_address(addr, cfg)` |
| `getTransfersForAddress` | `get_transfers_for_address(addr, cfg)` |
| `getValidatorHealth` | `get_validator_health()` |

The `*V2` calls return an `AccountPages` cursor - `.next()` for a page at a time,
or `.all()` to walk every page:

```rust
let holders = client
    .get_token_accounts_by_mint_v2(&mint, AccountsConfig { limit: Some(1000), ..Default::default() })
    .all()
    .await?;
```

The address-history calls return `AddressHistoryPage { data, pagination_token }`;
feed `pagination_token` back in via `AddressHistoryConfig::pagination_token`.
