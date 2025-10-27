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

pub struct HidAngle {
    latest: Arc<Mutex<Option<AngleSample>>>,
    tx: broadcast::Sender<AngleSample>,
    alpha: Arc<Mutex<f32>>,
}

impl HidAngle {
    // Existing entry point keeps behavior (discovery ON by default).
    pub async fn open(hz: f32) -> Result<Self> {
        Self::open_with(hz, true).await
    }

    // NEW: allow caller to toggle discovery.
    pub async fn open_with(hz: f32, _discovery: bool) -> Result<Self> {
        let latest = Arc::new(Mutex::new(None));
        let (tx, _rx) = broadcast::channel::<AngleSample>(256);
        let alpha: Arc<Mutex<f32>> = Arc::new(Mutex::new(0.25f32));

        let latest_c = Arc::clone(&latest);
        let tx_c = tx.clone();
        let alpha_c = Arc::clone(&alpha);

        tokio::spawn(async move {
            fn open_hinge(api: &hidapi::HidApi) -> Option<hidapi::HidDevice> {
                // 1) Best: Usage Page = Sensor (0x20) + Usage = Orientation (0x008A)
                for dev in api.device_list() {
                    let up = dev.usage_page();
                    let u = dev.usage();
                    if up == 0x20
                        && u == 0x008A
                        && let Ok(h) = dev.open_device(api)
                    {
                        #[cfg(feature = "diagnostics")]
                        eprintln!(
                            "[booklid] matched Sensor/Orientation: vid={:#06x} pid={:#06x}",
                            dev.vendor_id(),
                            dev.product_id()
                        );
                        return Some(h);
                    }
                }

                // 2) Fallback: Apple VID + commonly-seen PID (0x8104)
                for dev in api.device_list() {
                    if dev.vendor_id() == 0x05AC
                        && dev.product_id() == 0x8104
                        && let Ok(h) = dev.open_device(api)
                    {
                        #[cfg(feature = "diagnostics")]
                        eprintln!("[booklid] matched Apple VID/PID 0x05AC/0x8104 (fallback).");
                        return Some(h);
                    }
                }

                // 3) Last resort: any Apple device that responds to Feature Report #1
                for dev in api.device_list() {
                    if dev.vendor_id() == 0x05AC
                        && let Ok(h) = dev.open_device(api)
                    {
                        let mut probe = [0u8; 3];
                        probe[0] = 1;
                        if h.get_feature_report(&mut probe).is_ok() {
                            #[cfg(feature = "diagnostics")]
                            eprintln!(
                                "[booklid] using Apple device responding to Feature#1: pid={:#06x}",
                                dev.product_id()
                            );
                            return Some(h);
                        }
                    }
                }

                None
            }

            // Retry until we have HID and a device.
            let (mut hid, mut api) = loop {
                match hidapi::HidApi::new() {
                    Ok(a) => {
                        if let Some(h) = open_hinge(&a) {
                            #[cfg(feature = "diagnostics")]
                            eprintln!("[booklid] hinge sensor opened.");
                            break (h, a);
                        } else {
                            #[cfg(feature = "diagnostics")]
                            eprintln!("[booklid] hinge not found yet; retrying…");
                        }
                    }
                    Err(_e) => {
                        #[cfg(feature = "diagnostics")]
                        eprintln!("[booklid] hid init failed: {}", _e);
                    }
                }
                tokio::time::sleep(Duration::from_millis(800)).await;
            };

            // Optional discovery: probe feature report IDs 1..=8 quickly.
            #[cfg(feature = "mac_hid_discovery")]
            let report_id: u8 = if discovery {
                match probe_report_id(&mut hid, 1..=8, Duration::from_millis(400)) {
                    Some(best) => best,
                    None => 1,
                }
            } else {
                1
            };

            #[cfg(not(feature = "mac_hid_discovery"))]
            let report_id: u8 = 1;

            #[cfg(feature = "diagnostics")]
            eprintln!("[booklid] using Feature Report ID {}", report_id);

            // Some devices like a first “poke”
            let mut poke = [0u8; 3];
            poke[0] = report_id;
            let _ = hid.get_feature_report(&mut poke);

            let mut smoothed: Option<f32> = None;
            let target_hz = if hz.is_finite() && hz > 0.0 { hz } else { 60.0 };
            let mut interval = time::interval(Duration::from_secs_f32(1.0 / target_hz));

            loop {
                interval.tick().await;

                let mut buf = [0u8; 3];
                buf[0] = report_id;

                match hid.get_feature_report(&mut buf) {
                    Ok(_) => {
                        let raw = u16::from_le_bytes([buf[1], buf[2]]) as f32;
                        let angle_deg = raw; // adjust mapping later if needed

                        // EMA smoothing
                        let a = { (*alpha_c.lock().unwrap()).clamp(0.0, 1.0) };
                        let s = match smoothed {
                            None => angle_deg,
                            Some(prev) => prev + a * (angle_deg - prev),
                        };
                        smoothed = Some(s);

                        let sample = AngleSample {
                            angle_deg: s,
                            timestamp: Instant::now(),
                            source: Source::HingeFeature,
                        };

                        *latest_c.lock().unwrap() = Some(sample);
                        let _ = tx_c.send(sample);
                    }
                    Err(_) => {
                        #[cfg(feature = "diagnostics")]
                        eprintln!("[booklid] read failed; attempting re-open…");
                        if let Some(h) = open_hinge(&api) {
                            hid = h;
                            let mut p = [0u8; 3];
                            p[0] = report_id;
                            let _ = hid.get_feature_report(&mut p);
                        } else if let Ok(a2) = hidapi::HidApi::new() {
                            api = a2;
                        }
                        tokio::time::sleep(Duration::from_millis(300)).await;
                    }
                }
            }
        });

        Ok(Self { latest, tx, alpha })
    }
}

// Simple report ID probe: pick the ID with highest variance in-bounds.
#[cfg(feature = "mac_hid_discovery")]
fn probe_report_id(
    hid: &mut hidapi::HidDevice,
    ids: impl IntoIterator<Item = u8>,
    dur: Duration,
) -> Option<u8> {
    fn score(samples: &[f32]) -> Option<(f32, f32, f32)> {
        if samples.is_empty() {
            return None;
        }
        let min = samples.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = samples.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        // Bounds check (simple hinge-ish range)
        if !(min >= 0.0 && max <= 180.0) {
            return None;
        }
        let range = max - min;
        if range < 10.0 {
            return None;
        } // needs some movement
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        let var = samples.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / samples.len() as f32;
        Some((range, var, mean))
    }

    let mut best: Option<(u8, f32, f32, f32)> = None; // (id, range, var, mean)

    for id in ids {
        let t_end = Instant::now() + dur;
        let mut vals: Vec<f32> = Vec::with_capacity(64);
        while Instant::now() < t_end {
            let mut buf = [0u8; 3];
            buf[0] = id;
            if hid.get_feature_report(&mut buf).is_ok() {
                let raw = u16::from_le_bytes([buf[1], buf[2]]) as f32;
                vals.push(raw);
            }
            // small pause to avoid hammering (no async here)
            std::thread::sleep(Duration::from_millis(8));
        }

        if let Some((range, var, mean)) = score(&vals) {
            #[cfg(feature = "diagnostics")]
            eprintln!(
                "[booklid] discovery id={}: range={:.1} var={:.2} mean={:.1}",
                id, range, var, mean
            );
            match best {
                None => best = Some((id, range, var, mean)),
                Some((_, _, best_var, _)) if var > best_var => best = Some((id, range, var, mean)),
                _ => {}
            }
        } else {
            #[cfg(feature = "diagnostics")]
            eprintln!("[booklid] discovery id={} rejected", id);
        }
    }

    best.map(|(id, _, _, _)| id)
}

impl AngleDevice for HidAngle {
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
            source: Source::HingeFeature,
            note: "mac_hid_feature",
        }
    }
}
