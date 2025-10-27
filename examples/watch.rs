use booklid_rust::open;

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let dev = open(60.0).await?;
    dev.set_smoothing(0.3);
    println!("Streaming… (Ctrl-C to exit) source={:?}", dev.info().source);
    loop {
        if let Some(s) = dev.latest() {
            match s.source {
                booklid_rust::Source::ALS => {
                    println!("ALS: {:.2}  [{:?}]", s.angle_deg, s.source); // 0.00..1.00
                }
                _ => {
                    println!("{:6.2}°  [{:?}]", s.angle_deg, s.source); // degrees
                }
            }
        } else {
            println!("(waiting for samples…)");
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
