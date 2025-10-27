use crate::{AngleDevice, AngleSample, AngleStream, Result, Source};
use futures_util::StreamExt;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::{
    sync::broadcast,
    time::{self, Duration},
};
use tokio_stream::wrappers::BroadcastStream;

/// Ambient Light fallback.
/// - Streams a normalized “bellows” signal (0..1) tagged as ALS.
/// - AngleSample.angle_deg carries the normalized value (not degrees).
/// - Later, we’ll replace the signal source with the real macOS ALS.
pub struct AlsAngle {
    latest: Arc<Mutex<Option<AngleSample>>>,
    tx: broadcast::Sender<AngleSample>,
    alpha: Arc<Mutex<f32>>,
}

impl AlsAngle {
    pub async fn open(hz: f32) -> Result<Self> {
        let latest = Arc::new(Mutex::new(None));
        let (tx, _rx) = broadcast::channel::<AngleSample>(256);
        let alpha: Arc<Mutex<f32>> = Arc::new(Mutex::new(0.25f32));
        let latest_c = Arc::clone(&latest);
        let tx_c = tx.clone();
        let alpha_c: Arc<Mutex<f32>> = Arc::clone(&alpha);

        // Target rate and simple high-pass + normalization model.
        let target_hz: f32 = hz.max(10.0f32); // ALS is fine at ~10–60 Hz
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs_f32(1.0f32 / target_hz));
            let mut t = 0.0f32;
            let mut baseline = 0.5f32; // slow baseline
            let mut smoothed: Option<f32> = None;

            loop {
                interval.tick().await;
                t += 0.03f32;

                // Placeholder signal: smoothly varying value in 0..1.
                // Later, replace with real ALS Δlux and normalization.
                let raw = 0.5f32 + 0.45f32 * t.sin() * (1.0f32 + 0.2f32 * (0.6f32 * t).sin());

                // Slow LPF baseline to simulate drift removal (high-pass-ish)
                baseline = 0.995f32 * baseline + 0.005f32 * raw;
                let mut val = raw - baseline;

                // Normalize to 0..1
                val = (val * 3.0f32 + 0.5f32).clamp(0.0f32, 1.0f32);

                // Apply user EMA smoothing
                let a = (*alpha_c.lock().unwrap()).clamp(0.0f32, 1.0f32);
                let s = match smoothed {
                    None => val,
                    Some(prev) => prev + a * (val - prev),
                };
                smoothed = Some(s);

                let sample = AngleSample {
                    angle_deg: s, // NOT degrees; normalized 0..1
                    timestamp: Instant::now(),
                    source: Source::ALS,
                };

                *latest_c.lock().unwrap() = Some(sample);
                let _ = tx_c.send(sample);
            }
        });

        Ok(Self { latest, tx, alpha })
    }
}

impl AngleDevice for AlsAngle {
    fn latest(&self) -> Option<AngleSample> {
        *self.latest.lock().unwrap()
    }

    fn subscribe(&self) -> AngleStream {
        BroadcastStream::new(self.tx.subscribe())
            .filter_map(|it| async move { it.ok() })
            .boxed()
    }

    fn set_smoothing(&self, alpha: f32) {
        *self.alpha.lock().unwrap() = alpha;
    }

    fn confidence(&self) -> f32 {
        1.0
    }

    fn info(&self) -> crate::DeviceInfo {
        crate::DeviceInfo {
            source: Source::ALS,
            note: "mac_als",
        }
    }
}
