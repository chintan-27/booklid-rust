# booklid-rust

Simple API for reading your laptop lid angle (degrees) with async or blocking apps.
Only works on MacBooks bought after 2019; support for other platforms is coming.

Why use this
- One call to open a device, then read latest() or subscribe() to a stream.
- Works in async and non-async programs (open and open_blocking).
- Auto-reconnect on sensor hiccups; quiet by default, optional diagnostics.
- Discovery finds the right HID report on more Mac models; ALS provides a fallback “bellows” signal when hinge isn’t available.
- Mock backend is opt-in for testing; never used by default.

Device support
- macOS: Hinge angle via HID Feature (post-2019 MacBooks).
- Fallback: Ambient Light Sensor (ALS) normalized 0..1, not true degrees.
- Windows/Linux backends are planned.

Install
- Requires cargo-edit: `cargo install cargo-edit`
- GitHub dependency (tagged):
  ```
  cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.4.0
  ```
- Optional features:
  ```
  # Diagnostics logging
  cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.4.0 --features diagnostics

  # Mac HID discovery (auto-pick report ID)
  cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.4.0 --features mac_hid_discovery

  # Mac ALS fallback (normalized 0..1 control)
  cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.4.0 --features mac_als

  # Mock backend (testing only)
  cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.4.0 --features mock
  ```

Quickstart (async)
```rust
use booklid_rust::{open, AngleDevice};

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let dev = open(60.0).await?;
    dev.set_smoothing(0.3);
    println!("source={:?}", dev.info().source);
    loop {
        if let Some(s) = dev.latest() {
            match s.source {
                booklid_rust::Source::ALS => println!("bellows: {:.2}  [{:?}]", s.angle_deg, s.source),
                _ => println!("{:6.2}°  [{:?}]", s.angle_deg, s.source),
            }
        } else {
            println!("(waiting…)");
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
```

Quickstart (blocking)
```rust
use booklid_rust::{open_blocking, AngleDevice};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = open_blocking(60.0)?;
    dev.set_smoothing(0.3);
    println!("source={:?}", dev.info().source);
    loop {
        if let Some(s) = dev.latest() {
            match s.source {
                booklid_rust::Source::ALS => println!("bellows: {:.2}  [{:?}]", s.angle_deg, s.source),
                _ => println!("{:6.2}°  [{:?}]", s.angle_deg, s.source),
            }
        } else {
            println!("(waiting…)");
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
```

Subscribe to a stream
```rust
use booklid_rust::{open, AngleDevice};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let dev = open(60.0).await?;
    let mut stream = dev.subscribe();
    println!("source={:?}", dev.info().source);
    while let Some(s) = stream.next().await {
        match s.source {
            booklid_rust::Source::ALS => println!("bellows: {:.2}  [{:?}]", s.angle_deg, s.source),
            _ => println!("{:6.2}°  [{:?}]", s.angle_deg, s.source),
        }
    }
    Ok(())
}
```

Options (allow mock, toggle discovery, set smoothing)
```rust
use booklid_rust::{open_with, OpenOptions, AngleDevice};

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let opts = OpenOptions::new(60.0)
        .smoothing(0.3)
        .discovery(true)     // enable HID report ID discovery (mac)
        .allow_mock(true);   // testing only; requires --features mock
    let dev = open_with(opts).await?;
    println!("source={:?}", dev.info().source);
    Ok(())
}
```

Features
- default = ["mac_hid_feature"]
- mac_hid_discovery (probe Feature Report IDs 1..8 at startup)
- mac_als (Ambient Light fallback; publishes 0..1 control signal)
- mock (opt-in; never used unless allow_mock is true)
- diagnostics (opt-in logging)

Examples
- Async watch: `cargo run --example watch`
- Blocking watch: `cargo run --example watch_blocking`
- Subscribe: `cargo run --example subscribe`
- ALS fallback: `cargo run --example watch --no-default-features --features mac_als`
- Discovery: `cargo run --example watch --features mac_hid_discovery,diagnostics`
- Mock (testing): `cargo run --example mock_watch --no-default-features --features mock`

Troubleshooting
- macOS HID build issues → install Xcode Command Line Tools:
  `xcode-select --install`
- Code gray in editor (“inactive due to #[cfg]”) → build with the feature:
  `cargo run --example watch --features mac_hid_discovery`
- “backend error: no backend enabled” → you built with `--no-default-features`; enable a platform feature or use mock for testing.

License
MIT