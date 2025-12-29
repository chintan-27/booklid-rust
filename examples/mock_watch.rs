use booklid_rust::{OpenConfig, open_with_config};

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let cfg = OpenConfig::new(60.0).allow_mock(true).diagnostics(true);

    let dev = open_with_config(cfg).await?;

    loop {
        if let Some(s) = dev.latest() {
            println!("conf={:.2} val={:.3}", dev.confidence(), s.angle_deg);
        } else {
            println!("waiting… conf={:.2}", dev.confidence());
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
