use booklid_rust::open;
use tokio::time::{Duration, sleep};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hz = 60.0;
    let client = open(hz).await?;

    // CI mode: exit after a few seconds
    let ci = std::env::var("BOOKLID_CI").ok().as_deref() == Some("1");
    let mut printed_waiting = false;

    loop {
        let c = client.confidence();
        if let Some(sample) = client.latest() {
            // Live
            println!(
                "src={:?} conf={:.2} v={:.3}",
                client.info().source,
                c,
                sample.angle_deg
            );
        } else {
            // Waiting for confidence gate
            if !printed_waiting {
                println!(
                    "(waiting for confidence ≥ 0.70) src={:?} conf={:.2}",
                    client.info().source,
                    c
                );
                printed_waiting = true;
            }
        }

        if ci {
            // keep it short in CI
            static TICKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            if TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 120 {
                break;
            }
        }

        sleep(Duration::from_millis(25)).await;
    }
    Ok(())
}
