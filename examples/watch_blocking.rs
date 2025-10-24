use booklid_rust::{open_blocking};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = open_blocking(60.0)?;
    dev.set_smoothing(0.3);
    println!("Streaming (blocking)… Ctrl-C to exit");
    loop {
        if let Some(s) = dev.latest() {
            println!("{:6.2}°  [{:?}]", s.angle_deg, s.source);
        } else {
            println!("(waiting…)");
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}