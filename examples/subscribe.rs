use booklid_rust::{open_default};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let dev = open_default(60.0).await?;
    let mut s = dev.subscribe();
    println!("Streaming via subscribe() …");
    let mut n = 0u32;
    while let Some(sample) = s.next().await {
        if n % 15 == 0 {
            println!("{:6.2}°  [{:?}] @ {:?}", sample.angle_deg, sample.source, sample.timestamp);
        }
        n += 1;
        if n == 600 { // ~10 seconds at 60 Hz
            println!("ok: saw {} samples (~{} Hz)", n, n/10);
            break;
        }
    }
    Ok(())
}