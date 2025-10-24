use booklid_rust::{open_with, OpenOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Allow mock explicitly; run with: --no-default-features --features mock
    let opts = OpenOptions::new(60.0).smoothing(0.3).allow_mock(true);
    let dev = open_with(opts).await?;
    println!("Mock/watch source={:?}", dev.info().source);
    loop {
        if let Some(s) = dev.latest() {
            println!("{:6.2}°  [{:?}]", s.angle_deg, s.source);
        } else {
            println!("(waiting for samples…)");
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}