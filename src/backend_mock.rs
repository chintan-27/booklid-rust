use crate::{AngleDevice, AngleSample, Source};
use futures_core::Stream;
use std::{f32::consts::PI, sync::{Arc, Mutex}, time::Instant};
use tokio::{sync::broadcast, time::{self, Duration}};

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

        let latest_c = latest.clone();
        let tx_c = tx.clone();
        let mut t = 0.0f32;
        let mut interval = time::interval(Duration::from_secs_f32((1.0 / hz.max(1.0))));
        tokio::spawn(async move {
            loop {
                interval.tick().await;
                t += 0.04;
                let angle = 95.0 + 20.0 * (t).sin() + 0.5 * (3.7 * t).sin();
                let sample = AngleSample { angle_deg: angle, timestamp: Instant::now(), source: Source::HingeFeature };
                *latest_c.lock().unwrap() = Some(sample);
                let _ = tx_c.send(sample);
            }
        });

        Ok(Self { latest, tx, alpha })
    }
}

impl AngleDevice for MockAngle {
    fn latest(&self) -> Option<AngleSample> { *self.latest.lock().unwrap() }
    fn stream(&self) -> &dyn Stream<Item = AngleSample> {
        struct Rx(tokio::sync::broadcast::Receiver<AngleSample>);
        impl Stream for Rx {
            type Item = AngleSample;
            fn poll_next(
                mut self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Self::Item>> {
                match self.0.try_recv() {
                    Ok(v) => std::task::Poll::Ready(Some(v)),
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => std::task::Poll::Ready(None),
                    _ => std::task::Poll::Pending,
                }
            }
        }
        Box::leak(Box::new(Rx(self.tx.subscribe())))
    }
    fn set_smoothing(&self, alpha: f32) { let _ = alpha; }
}
