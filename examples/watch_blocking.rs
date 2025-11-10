use booklid_rust::open_blocking;
use std::thread::sleep;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hz = 60.0;
    let client = open_blocking(hz)?;

    let ci = std::env::var("BOOKLID_CI").ok().as_deref() == Some("1");
    let mut ticks = 0usize;
    let mut printed_waiting = false;

    loop {
        let c = client.confidence();
        if let Some(sample) = client.latest() {
            println!(
                "src={:?} conf={:.2} v={:.3}",
                client.info().source,
                c,
                sample.angle_deg
            );
        } else if !printed_waiting {
            println!(
                "(waiting for confidence ≥ 0.70) src={:?} conf={:.2}",
                client.info().source,
                c
            );
            printed_waiting = true;
        }

        if ci {
            ticks += 1;
            if ticks > 120 {
                break;
            }
        }

        sleep(Duration::from_millis(25));
    }
    Ok(())
}
