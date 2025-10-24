#[cfg(feature = "mock")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use booklid_rust::MockAngle; // re-exported from the crate root
    use booklid_rust::AngleDevice;

    let dev = MockAngle::open(60.0).await?;
    dev.set_smoothing(0.3);
    println!("Mock streaming… (Ctrl-C to exit)");
    loop {
        if let Some(s) = dev.latest() {
            println!("{:6.2}°  [{:?}]", s.angle_deg, s.source);
        } else {
            println!("(waiting…)");
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

#[cfg(not(feature = "mock"))]
fn main() {
    eprintln!("Enable the `mock` feature to run this example.");
}