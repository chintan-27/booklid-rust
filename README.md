# booklid-rust

Simple, **stable** API for reading your laptop lid angle (degrees) with async or blocking apps.

Supports **macOS, Windows, and Linux** with automatic backend discovery, confidence gating, and reconnects.  
Ambient Light Sensor (ALS) is used as a safe fallback and publishes a normalized control (0..1) when a true hinge angle is unavailable.

> **Distribution:** This crate is currently distributed via **GitHub releases** (not crates.io).

---

## Why use this

* One call to open a device → read `latest()` or `subscribe()` to a stream.
* Works in async **and** non-async programs (`open` and `open_blocking`).
* Confidence-gated output (configurable; default ≥ **0.70** to “go live”, drops with hysteresis).
* Until confidence passes the threshold, `latest()` returns `None`.
* Automatic reconnect with backoff.
* Quiet by default; optional diagnostics (config or env).
* Backend discovery and fallback are automatic and configurable.
* Mock backend is **opt-in** for testing; **never** used by default.
* **Stable 1.x API** (no breaking changes without a major version bump).

---

## Device support

* **macOS (stable):**
  * Hinge angle via HID Feature (2019+ MacBooks).
  * Fallback: **ALS** publishes a normalized control (0..1), **not** degrees.
* **Windows (stable):**
  * WinRT sensors probe chain: **Hinge → Tilt → ALS**.
* **Linux (stable):**
  * **iio-sensor-proxy (DBus)** for tilt classification + light level.
  * Fallback: **IIO `/sys`** accelerometer / light channels when available.
  * ALS-style fallbacks publish normalized values when degrees are unavailable.

> **Note:** Some Linux devices expose **tilt classes** rather than a true hinge angle.  
> In those cases the value is monotonic but not a physical hinge degree.

---

## Install (GitHub)

Add the dependency directly from GitHub:

```bash
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust
````

### Optional features

```bash
# macOS HID report discovery
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --features mac_hid_discovery

# macOS ALS fallback
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --features mac_als

# Windows sensors backend
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --features win_sensors

# Linux DBus proxy backend
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --features linux_iio_proxy

# Linux /sys IIO backend
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --features linux_iio_sys

# Mock backend (testing only)
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --features mock
```

Alternatively, in `Cargo.toml`:

```toml
[dependencies]
booklid-rust = { git = "https://github.com/chintan-27/booklid-rust", features = ["win_sensors"] }
```

---

## Quickstart (async)

```rust
use booklid_rust::{open, AngleDevice};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = open(60.0).await?;
    loop {
        if let Some(s) = client.latest() {
            println!("conf={:.2} val={:.3}", client.confidence(), s.angle_deg);
        } else {
            println!("waiting… conf={:.2}", client.confidence());
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
```

---

## Quickstart (blocking)

```rust
use booklid_rust::{open_blocking, AngleDevice};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = open_blocking(60.0)?;
    loop {
        if let Some(s) = client.latest() {
            println!("conf={:.2} val={:.3}", client.confidence(), s.angle_deg);
        } else {
            println!("waiting… conf={:.2}", client.confidence());
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}
```

> **Note:** `open_blocking*` creates/uses a global multithreaded Tokio runtime.
> Avoid calling it from async contexts.

---

## Subscribe to a stream

```rust
use booklid_rust::{open, AngleDevice};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let dev = open(60.0).await?;
    let mut stream = dev.subscribe();
    println!("source={:?}", dev.info().source);
    while let Some(s) = stream.next().await {
        println!("{:6.2}  [{:?}]", s.angle_deg, s.source);
    }
    Ok(())
}
```

---

## Configuration (OpenConfig)

`OpenConfig` is the **stable configuration API** in 1.0.

```rust
use booklid_rust::{open_with_config, OpenConfig, AngleDevice};

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let cfg = OpenConfig::new(60.0)
        .smoothing(0.3)
        .min_confidence(0.70)
        .prefer(vec![booklid_rust::Source::HingeFeature])
        .disable(vec![booklid_rust::Source::ALS])
        .diagnostics(true);

    let dev = open_with_config(cfg).await?;
    println!("source={:?} conf={:.2}", dev.info().source, dev.confidence());
    Ok(())
}
```

### What you can configure

* `hz` — sampling frequency (> 0)
* `smoothing_alpha` — EMA alpha [0,1]
* `min_confidence` — go-live threshold (drop uses hysteresis)
* `prefer_sources` / `disable_backends`
* `discovery` — backend discovery (macOS HID)
* `allow_mock` — testing only
* `diagnostics` — one-line init report
* `fail_after` — overall open timeout
* `persistence` — remember last successful backend

---

## Persistence

By default, booklid remembers the last successful backend and tries it first on the next startup.

Clear persisted state:

```rust
booklid_rust::clear_persisted_state()?;
```

---

## Env toggles

* `BOOKLID_DESKTOP=1` — force desktop guard (skip hinge; allow ALS).
* `BOOKLID_DIAGNOSTICS=1` — enable diagnostics line.
* `BOOKLID_CI=1` — examples exit after a short run (used in CI).

---

## Examples

```bash
# Async watch
cargo run --example watch

# Blocking watch
cargo run --example watch_blocking

# Subscribe
cargo run --example subscribe

# Linux ALS / proxy testing
BOOKLID_DESKTOP=1 cargo run --example watch --no-default-features --features linux_iio_proxy

# Mock (testing only)
cargo run --example mock_watch --no-default-features --features mock
```

---

## Troubleshooting

* **macOS HID build issues**

  ```bash
  xcode-select --install
  ```

* **Linux permissions**
  Ensure access to `/sys/bus/iio` (udev rules may be required).

* **“no backend enabled”**
  Enable a platform feature or use `mock` for testing.

---

## License

MIT

```
