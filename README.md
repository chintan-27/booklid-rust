# booklid-rust

Simple API for reading your laptop lid angle (degrees) with async or blocking apps.
Works on modern MacBooks (2019+) via HID; ALS provides a fallback “bellows” control (0..1).
Windows/Linux backends are planned.

## Why use this

* One call to open a device → read `latest()` or `subscribe()` to a stream.
* Works in async **and** non-async programs (`open` and `open_blocking`).
* Confidence-gated output (≥ **0.70** to “go live”, drops < **0.65**), with auto-reconnect.
* Quiet by default; optional diagnostics (env or feature).
* Discovery finds the right HID report on more Mac models; **ALS** is a safe fallback.
* Mock backend is **opt-in** for testing; **never** used by default.

## Device support

* **macOS:** Hinge angle via HID Feature (2019+).
  Fallback: **ALS** publishes a normalized control (0..1), **not** degrees.
* **Windows / Linux:** planned (sensor chains sketched; stubs coming next).

---

## Install

> Requires `cargo-edit`: `cargo install cargo-edit`

**GitHub dependency (tagged):**

```bash
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.5.0
```

**Optional features:**

```bash
# Diagnostics logging (env also supported; see below)
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.5.0 --features diagnostics

# Mac HID discovery (auto-pick Feature Report ID)
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.5.0 --features mac_hid_discovery

# Mac ALS fallback (normalized 0..1 control)
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.5.0 --features mac_als

# Mock backend (testing only)
cargo add booklid-rust --git https://github.com/chintan-27/booklid-rust --tag v0.5.0 --features mock
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

> **Note:** `open_blocking*` creates/uses a global multithreaded Tokio runtime; avoid calling from async contexts.

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
        match s.source {
            booklid_rust::Source::ALS => {
                println!("bellows: {:.2}  [{:?}]", s.angle_deg, s.source);
            }
            _ => println!("{:6.2}°  [{:?}]", s.angle_deg, s.source),
        }
    }
    Ok(())
}
```

---

## Options (allow mock, toggle discovery, set smoothing)

```rust
use booklid_rust::{open_with, OpenOptions, AngleDevice};

#[tokio::main]
async fn main() -> booklid_rust::Result<()> {
    let opts = OpenOptions::new(60.0)
        .smoothing(0.3)   // EMA alpha in [0,1]
        .discovery(true)  // HID report ID discovery (mac)
        .allow_mock(true);// testing only; requires --features mock

    let dev = open_with(opts).await?;
    println!("source={:?} conf={:.2}", dev.info().source, dev.confidence());
    Ok(())
}
```

---

## Features

* `default = ["mac_hid_feature"]`
* `mac_hid_discovery` — probe Feature Report IDs 1..8 at startup
* `mac_als` — Ambient Light fallback; publishes 0..1 control signal
* `mock` — **opt-in**; never used unless `allow_mock(true)` is set
* `diagnostics` — opt-in logging (you can also use the env var below)

---

## Env toggles

* `BOOKLID_DESKTOP=1` — **force desktop guard** (skip hinge; go ALS). Handy for testing ALS on any machine.
* `BOOKLID_DIAGNOSTICS=1` — print a one-line summary when a backend opens:

  ```
  booklid: chosen=ALS tried=[HingeFeature,HingeHid,ALS] guard=false min=0.70 drop=0.65 hz=60.0 smoothing=0.25
  ```
* `BOOKLID_CI=1` — examples exit after a short run (used in CI).

---

## Examples

* Async watch:
  `cargo run --example watch`
* Blocking watch:
  `cargo run --example watch_blocking`
* Subscribe:
  `cargo run --example subscribe`
* ALS fallback (force desktop guard):
  `BOOKLID_DESKTOP=1 cargo run --example watch --no-default-features --features mac_als`
* Discovery w/ diagnostics:
  `BOOKLID_DIAGNOSTICS=1 cargo run --example watch --features mac_hid_discovery`
* Mock (testing only):
  `cargo run --example mock_watch --no-default-features --features mock`

---

## Troubleshooting

* **macOS HID build issues** → install Xcode CLT:
  `xcode-select --install`
* **Code gray in editor (“inactive due to #[cfg]”)** → build with the needed feature, e.g.:
  `cargo run --example watch --features mac_hid_discovery`
* **“no backend enabled”** → you built with `--no-default-features`; enable a platform feature or use `mock` for testing.

---

## License

[MIT]() or [Apache-2.0]().