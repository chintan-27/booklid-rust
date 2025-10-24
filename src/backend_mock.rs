// src/backend_mock.rs
use crate::{AngleDevice, AngleSample, AngleStream, Source};
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

pub struct MockAngle {
    latest: Arc<Mutex<Option<AngleSample>>>,
    tx: broadcast::Sender<AngleSample>,
    alpha: Arc<Mutex<f32>>,
}

impl MockAngle {
    pub async fn open(hz: f32) -> anyhow::Result<Self> {
        let latest = Arc::new(Mutex::new(None));
        let (tx, _rx) = broadcast::channel::<AngleSample>(256);
        let alpha = Arc::new(Mutex::new(0.25));

        let latest_c = Arc::clone(&latest);
        let tx_c = tx.clone();
        let alpha_c = Arc::clone(&alpha);

        // Generate a smooth, slightly modulated waveform around ~95–115°
        let target_hz = hz.max(1.0);
        tokio::spawn(async move {
            let mut t = 0.0f32;
            let mut smoothed: Option<f32> = None;
            let mut interval = time::interval(Duration::from_secs_f32(1.0 / target_hz));
            loop {
                interval.tick().await;
                t += 0.04;
                let angle = 95.0 + 20.0 * (t).sin() + 0.5 * (3.7 * t).sin();

                // Apply EMA smoothing like the HID backend
                let a = {
                    let a: f32 = *alpha_c.lock().unwrap();
                    a.clamp(0.0f32, 1.0f32)
                };
                let s = match smoothed {
                    None => angle,
                    Some(prev) => prev + a * (angle - prev),
                };
                smoothed = Some(s);

                let sample = AngleSample {
                    angle_deg: s,
                    timestamp: Instant::now(),
                    source: Source::Mock,
                };
                *latest_c.lock().unwrap() = Some(sample);
                let _ = tx_c.send(sample);
            }
        });

        Ok(Self { latest, tx, alpha })
    }
}

impl AngleDevice for MockAngle {
    fn latest(&self) -> Option<AngleSample> {
        *self.latest.lock().unwrap()
    }

    fn subscribe(&self) -> AngleStream {
        BroadcastStream::new(self.tx.subscribe())
            .filter_map(|it| async move { it.ok() }) // drop lag/closed errors
            .boxed()
    }

    fn set_smoothing(&self, alpha: f32) {
        *self.alpha.lock().unwrap() = alpha;
    }

    fn confidence(&self) -> f32 {
        1.0
    }
}