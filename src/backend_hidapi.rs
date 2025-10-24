use crate::{AngleDevice, AngleSample, AngleStream, Result, Source};
// use futures_util::StreamExt;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::{
    sync::broadcast,
    time::{self, Duration},
};
// use tokio_stream::wrappers::BroadcastStream;

pub struct HidAngle {
    latest: Arc<Mutex<Option<AngleSample>>>,
    tx: broadcast::Sender<AngleSample>,
    alpha: Arc<Mutex<f32>>,
}

impl HidAngle {
    pub async fn open(hz: f32) -> Result<Self> {
        // We open HID inside the task so we don't move non-Send handles across threads.
        let latest = Arc::new(Mutex::new(None));
        let (tx, _rx) = broadcast::channel::<AngleSample>(256);
        let alpha: Arc<Mutex<f32>> = Arc::new(Mutex::new(0.25f32));

        let latest_c = Arc::clone(&latest);
        let tx_c = tx.clone();
        let alpha_c = Arc::clone(&alpha);

        tokio::spawn(async move {
            // Helper that tries to open a hinge-like device.
            fn open_hinge(api: &hidapi::HidApi) -> Option<hidapi::HidDevice> {
                // 1) Best: Usage Page = Sensor (0x20) + Usage = Orientation (0x008A)
                for dev in api.device_list() {
                    let up = dev.usage_page();
                    let u = dev.usage();
                    if up == 0x20 && u == 0x008A {
                        if let Ok(h) = dev.open_device(api) {
                            #[cfg(feature = "diagnostics")]
                            eprintln!(
                                "[booklid] matched Sensor/Orientation: vid={:#06x} pid={:#06x}",
                                dev.vendor_id(),
                                dev.product_id()
                            );
                            return Some(h);
                        }
                    }
                }
                // 2) Fallback: Apple VID + commonly-seen PID (0x8104)
                for dev in api.device_list() {
                    if dev.vendor_id() == 0x05AC && dev.product_id() == 0x8104 {
                        if let Ok(h) = dev.open_device(api) {
                            #[cfg(feature = "diagnostics")]
                            eprintln!(
                                "[booklid] matched Apple VID/PID 0x05AC/0x8104 (fallback)."
                            );
                            return Some(h);
                        }
                    }
                }
                // 3) Last resort: any Apple device that responds to Feature Report #1
                for dev in api.device_list() {
                    if dev.vendor_id() == 0x05AC {
                        if let Ok(h) = dev.open_device(api) {
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
                }
                None
            }

            // Retry loop: keep trying to get a device until we succeed.
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

            // Some devices like a first “poke”
            let mut poke = [0u8; 3];
            poke[0] = 1;
            let _ = hid.get_feature_report(&mut poke);

            let mut smoothed: Option<f32> = None;
            let target_hz = if hz.is_finite() && hz > 0.0 { hz } else { 60.0 };
            let mut interval = time::interval(Duration::from_secs_f32(1.0 / target_hz));

            loop {
                interval.tick().await;

                let mut buf = [0u8; 3];
                buf[0] = 1; // Feature Report ID 1

                match hid.get_feature_report(&mut buf) {
                    Ok(_) => {
                        // buf = [report_id, lo, hi]
                        let raw = u16::from_le_bytes([buf[1], buf[2]]) as f32;
                        // Most firmwares report degrees directly; adjust later if needed.
                        let angle_deg = raw;

                        // Smoothing factor (0..1)
                        let a = {
                            let a: f32 = *alpha_c.lock().unwrap();
                            a.clamp(0.0f32, 1.0f32)
                        };

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
                            p[0] = 1;
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

impl AngleDevice for HidAngle {
    fn latest(&self) -> Option<AngleSample> { *self.latest.lock().unwrap() }

    fn subscribe(&self) -> AngleStream {
        use futures_util::StreamExt;
        use tokio_stream::wrappers::BroadcastStream;
        BroadcastStream::new(self.tx.subscribe())
            .filter_map(|it| async move { it.ok() })
            .boxed()
    }

    fn set_smoothing(&self, alpha: f32) { *self.alpha.lock().unwrap() = alpha; }

    fn confidence(&self) -> f32 { 1.0 }

    fn info(&self) -> crate::DeviceInfo {
        crate::DeviceInfo { source: Source::HingeFeature, note: "mac_hid_feature" }
    }
}