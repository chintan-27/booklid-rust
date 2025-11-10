use crate::{AngleDevice, AngleSample, AngleStream, DeviceInfo, Result, Source};
use futures_util::StreamExt;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::{
    sync::broadcast,
    time::{self, Duration},
};
use tokio_stream::wrappers::BroadcastStream;

/// Ambient Light fallback (placeholder signal).
/// - Streams a normalized “bellows” value in [0.0, 1.0] tagged as ALS.
/// - `AngleSample.angle_deg` carries the normalized value (NOT degrees).
/// - Confidence grows as the signal stabilizes (simple rolling-variance heuristic).
pub struct AlsAngle {
    latest: Arc<Mutex<Option<AngleSample>>>,
    tx: broadcast::Sender<AngleSample>,
    alpha: Arc<Mutex<f32>>,
    conf: Arc<Mutex<f32>>,
}

impl AlsAngle {
    pub async fn open(hz: f32) -> Result<Self> {
        let latest = Arc::new(Mutex::new(None));
        let (tx, _rx) = broadcast::channel::<AngleSample>(256);
        let alpha: Arc<Mutex<f32>> = Arc::new(Mutex::new(0.25));
        let conf: Arc<Mutex<f32>> = Arc::new(Mutex::new(0.2));

        // clones for task
        let latest_c = Arc::clone(&latest);
        let tx_c = tx.clone();
        let alpha_c = Arc::clone(&alpha);
        let conf_c = Arc::clone(&conf);

        // Target rate and simple high-pass + normalization model.
        let target_hz: f32 = hz.max(10.0); // ALS is fine around 10–60 Hz

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs_f32(1.0 / target_hz));
            let mut t = 0.0f32;
            let mut baseline = 0.5f32; // slow baseline
            let mut smoothed: Option<f32> = None;

            // Confidence via rolling variance on last N samples
            const CAP: usize = 64;
            let mut buf: VecDeque<f32> = VecDeque::with_capacity(CAP);

            loop {
                interval.tick().await;
                t += 0.03;

                // Placeholder signal: smoothly varying value in [0,1].
                // Later: replace with real ALS Δlux and normalization.
                let raw = 0.5 + 0.45 * t.sin() * (1.0 + 0.2 * (0.6 * t).sin());

                // Slow LPF baseline to simulate drift removal (high-pass-ish)
                baseline = 0.995 * baseline + 0.005 * raw;
                let mut val = raw - baseline;

                // Normalize to [0,1]
                val = (val * 3.0 + 0.5).clamp(0.0, 1.0);

                // Apply user EMA smoothing
                let a = (*alpha_c.lock().unwrap()).clamp(0.0, 1.0);
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

                // Update latest & broadcast
                *latest_c.lock().unwrap() = Some(sample);
                let _ = tx_c.send(sample);

                // Update confidence from rolling variance (stable => high)
                if buf.len() == CAP {
                    buf.pop_front();
                }
                buf.push_back(s);
                let n = buf.len() as f32;
                let mean = buf.iter().copied().sum::<f32>() / n;
                let var = buf
                    .iter()
                    .map(|v| {
                        let d = *v - mean;
                        d * d
                    })
                    .sum::<f32>()
                    / n;

                // Tunable mapping: 1 / (1 + k * var)
                let stability = 1.0 / (1.0 + 20.0 * var);
                *conf_c.lock().unwrap() = stability.clamp(0.0, 1.0);
            }
        });

        Ok(Self {
            latest,
            tx,
            alpha,
            conf,
        })
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
        *self.conf.lock().unwrap()
    }

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            source: Source::ALS,
            note: "mac_als",
        }
    }
}
