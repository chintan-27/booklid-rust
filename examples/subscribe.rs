use booklid_rust::open;
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let dev = open(60.0).await?;
    let mut stream = dev.subscribe();
    println!("Streaming via subscribe(), source={:?}", dev.info().source);
    let mut n = 0u32;
    while let Some(s) = stream.next().await {
        if n % 15 == 0 {
            println!("{:6.2}°  [{:?}] @ {:?}", s.angle_deg, s.source, s.timestamp);
        }
        n += 1;
    }
    Ok(())
}
