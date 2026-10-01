//! Probe: what does the Blur gRPC stream actually deliver for `swap`?
//! Run: PROTOC=... cargo run -p shadow-ingest --example blur_probe

use futures::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let key = std::env::var("SOLAMI_API_KEY").expect("SOLAMI_API_KEY");
    let mut client = solami::builder().with_grpc(&key).build().await?;

    // swap with a min volume filter, mirroring the docs.rs example
    let mut stream = client
        .grpc()
        .subscribe_blur(solami::SubscribeBlurRequest {
            event_type: vec!["swap".into()],
            min_volume_usd: Some(100.0),
            ..Default::default()
        })
        .await?;
    println!("subscribed: swap min_volume_usd=100");
    let t0 = std::time::Instant::now();
    let mut n = 0;
    while let Some(update) = stream.updates.next().await {
        match update {
            Ok(ev) => {
                n += 1;
                println!(
                    "[{}] type={} slot={} mint={} json={}",
                    n,
                    ev.event_type,
                    ev.slot,
                    ev.mint,
                    ev.json.chars().take(220).collect::<String>()
                );
                if n >= 3 {
                    break;
                }
            }
            Err(e) => {
                println!("stream error: {e}");
                break;
            }
        }
        if t0.elapsed().as_secs() > 30 {
            println!("30s elapsed with {n} events");
            break;
        }
    }
    if n == 0 {
        println!("no swap events received");
    }
    Ok(())
}
