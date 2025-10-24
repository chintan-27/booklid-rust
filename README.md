# booklid-rust

Simple, cross-platform lid angle readings (degrees) with an easy API.

Quickstart
- Async:
  use booklid_rust::{open, AngleDevice};

  #[tokio::main]
  async fn main() -> booklid_rust::Result<()> {
      let dev = open(60.0).await?;
      dev.set_smoothing(0.3);
      loop {
          if let Some(s) = dev.latest() {
              println!("{:6.2}°  [{:?}]", s.angle_deg, s.source);
          } else {
              println!("(waiting…)"); 
          }
          tokio::time::sleep(std::time::Duration::from_millis(200)).await;
      }
  }

- Blocking:
  use booklid_rust::{open_blocking, AngleDevice};

  fn main() -> Result<(), Box<dyn std::error::Error>> {
      let dev = open_blocking(60.0)?;
      dev.set_smoothing(0.3);
      loop {
          if let Some(s) = dev.latest() { println!("{:6.2}°  [{:?}]", s.angle_deg, s.source); }
          std::thread::sleep(std::time::Duration::from_millis(200));
      }
  }

Subscribe to a stream
- Bring StreamExt into scope and await samples:
  use booklid_rust::{open, AngleDevice};
  use futures_util::StreamExt;

  #[tokio::main]
  async fn main() -> booklid_rust::Result<()> {
      let dev = open(60.0).await?;
      let mut stream = dev.subscribe();
      while let Some(s) = stream.next().await {
          println!("{:6.2}°  [{:?}]", s.angle_deg, s.source);
      }
      Ok(())
  }

Features
- default = ["mac_hid_feature"]
- mock (opt-in, for testing only), diagnostics (opt-in logging)
- Platform features coming next: mac_hid_discovery, mac_iokit_raw, mac_als, win_sensors, linux_iio_proxy, linux_iio_sys

Testing with mock
- Enable the feature and allow mock in code:
  use booklid_rust::{open_with, OpenOptions, AngleDevice};

  #[tokio::main]
  async fn main() -> Result<(), Box<dyn std::error::Error>> {
      let dev = open_with(OpenOptions::new(60.0).allow_mock(true)).await?;
      println!("source={:?}", dev.info().source);
      Ok(())
  }

Notes
- Mock is never used unless both compiled with --features mock and allow_mock(true) is set.
- Diagnostics logs appear only when built with --features diagnostics.

License
MIT