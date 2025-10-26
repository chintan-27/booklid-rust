# booklid-rust

Simple API for reading your laptop lid angle (degrees) with async or blocking apps. **Only works on macbooks bought after 2019**

Why use this
- One call to open a device, then read latest() or subscribe() to a stream.
- Works in async and non-async programs.
- Auto-reconnect on sensor hiccups; quiet by default, optional diagnostics.
- Mock backend is opt-in for testing; never used in production by default.

Device Support
- Only works on MacBooks bought after 2019; support for other devices/platforms is coming in next versions.

Install
- Add the crate:
  ```
  cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.2.0
  ```
- macOS: default feature mac_hid_feature is enabled and should “just work”.
- Testing: enable the mock feature when you want synthetic data:
  ```
  cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.2.0 --features diagnostics 
  # OR
  cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.2.0 --features mock
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
            println!("{:6.2}°  [{:?}]", s.angle_deg, s.source);
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
            println!("{:6.2}°  [{:?}]", s.angle_deg, s.source);
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
        println!("{:6.2}°  [{:?}]", s.angle_deg, s.source);
    }
    Ok(())
}
```

Options (allow mock, set initial smoothing)
```rust
use booklid_rust::{open_with, OpenOptions, AngleDevice};

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let opts = OpenOptions::new(60.0)
        .smoothing(0.3)
        .allow_mock(true); // only takes effect if built with --features mock
    let dev = open_with(opts).await?;
    println!("source={:?}", dev.info().source);
    Ok(())
}
```

Features
- default = ["mac_hid_feature"]
- mock (opt-in, for testing only)
- diagnostics (opt-in logging)
- Roadmap: mac_hid_discovery, mac_iokit_raw, mac_als, win_sensors, linux_iio_proxy, linux_iio_sys

Examples
- Async watch: cargo run --example watch
- Blocking watch: cargo run --example watch_blocking
- Subscribe: cargo run --example subscribe
- Mock watch (testing): cargo run --example mock_watch --no-default-features --features mock.

Troubleshooting
- macOS HID build issues → ensure Xcode Command Line Tools:
  xcode-select --install
- “backend error: no backend enabled” → you built with no-default-features; enable a platform feature or use mock for testing.

License - [MIT](https://github.com/chintan-27/booklid-rust/blob/main/LICENSE)
