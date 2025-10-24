use booklid_rust::open_default;

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let dev = open_default(60.0).await?;
    dev.set_smoothing(0.3);
    println!("Streaming… (Ctrl-C to exit)");
    loop {
        if let Some(s) = dev.latest() {
            println!("{:6.2}°  [{:?}]", s.angle_deg, s.source);
        } else {
            println!("(waiting for hinge samples…)");
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}