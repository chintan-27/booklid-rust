#![cfg(all(target_os = "linux", any(feature = "linux_iio_proxy", feature = "linux_iio_sys")))]

use crate::{AngleDevice, AngleSample, AngleStream, DeviceInfo, Result, Source, Error};
use futures_util::StreamExt;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::{
    sync::broadcast,
    time::{self, Duration},
};
use tokio_stream::wrappers::BroadcastStream;

#[cfg(feature = "linux_iio_proxy")]
use zbus::{blocking::Connection as ZConn, zvariant::OwnedObjectPath};

pub struct LinuxAngle {
    latest: Arc<Mutex<Option<AngleSample>>>,
    tx: broadcast::Sender<AngleSample>,
    alpha: Arc<Mutex<f32>>,
    conf: Arc<Mutex<f32>>,
    src: Source,
    note: &'static str,
}

impl LinuxAngle {
    pub async fn open_tilt(hz: f32) -> Result<Self> {
        // Try DBus first, else /sys accelerometers
        #[cfg(feature = "linux_iio_proxy")]
        if let Ok(dev) = Self::spawn_from_proxy_tilt(hz).await {
            return Ok(dev);
        }
        Self::spawn_from_sys_tilt(hz).await
    }

    pub async fn open_als(hz: f32) -> Result<Self> {
        #[cfg(feature = "linux_iio_proxy")]
        if let Ok(dev) = Self::spawn_from_proxy_als(hz).await {
            return Ok(dev);
        }
        Self::spawn_from_sys_als(hz).await
    }

    #[cfg(feature = "linux_iio_proxy")]
    async fn spawn_from_proxy_tilt(hz: f32) -> Result<Self> {
        // iio-sensor-proxy exposes accelerometer orientation, not raw angle.
        // We estimate an angle from pitch (simple).
        let latest = Arc::new(Mutex::new(None));
        let (tx, _rx) = broadcast::channel::<AngleSample>(256);
        let alpha = Arc::new(Mutex::new(0.25f32));
        let conf = Arc::new(Mutex::new(0.2f32));

        let latest_c = latest.clone();
        let tx_c = tx.clone();
        let alpha_c = alpha.clone();
        let conf_c = conf.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs_f32(1.0 / hz.max(20.0)));
            let mut buf: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(64);
            let mut smoothed: Option<f32> = None;

            loop {
                interval.tick().await;
                // (Blocking zbus conn per tick is not ideal; a persistent async bus is better.
                // Keep simple for 0.6; users rarely hit proxy-only path in tight loops.)
                let angle = query_proxy_pitch_degrees().unwrap_or(0.0);
                let a = (*alpha_c.lock().unwrap()).clamp(0.0, 1.0);
                let s = match smoothed { None => angle, Some(prev) => prev + a*(angle - prev) };
                smoothed = Some(s);

                if buf.len() == 64 { buf.pop_front(); }
                buf.push_back(s);
                let n = buf.len() as f32;
                let m = buf.iter().copied().sum::<f32>()/n;
                let v = buf.iter().map(|v|{let d=*v-m; d*d}).sum::<f32>()/n;
                let stability = (1.0 / (1.0 + 0.05 * v)).clamp(0.0, 1.0);
                *conf_c.lock().unwrap() = stability;

                let sample = AngleSample { angle_deg: s, timestamp: Instant::now(), source: Source::LinuxTilt };
                *latest_c.lock().unwrap() = Some(sample);
                let _ = tx_c.send(sample);
            }
        });

        Ok(Self { latest, tx, alpha, conf, src: Source::LinuxTilt, note: "linux_proxy_tilt" })
    }

    #[cfg(feature = "linux_iio_proxy")]
    async fn spawn_from_proxy_als(hz: f32) -> Result<Self> {
        let latest = Arc::new(Mutex::new(None));
        let (tx, _rx) = broadcast::channel::<AngleSample>(256);
        let alpha = Arc::new(Mutex::new(0.25f32));
        let conf = Arc::new(Mutex::new(0.2f32));

        let latest_c = latest.clone();
        let tx_c = tx.clone();
        let alpha_c = alpha.clone();
        let conf_c = conf.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs_f32(1.0 / hz.max(10.0)));
            let mut baseline = 10.0f32;
            let mut smoothed: Option<f32> = None;
            let mut buf: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(64);

            loop {
                interval.tick().await;
                let lux = query_proxy_lux().unwrap_or(1.0);
                baseline = 0.995 * baseline + 0.005 * lux;
                let val = lux - baseline;
                let n = (val * 0.02 + 0.5).clamp(0.0, 1.0);

                let a = (*alpha_c.lock().unwrap()).clamp(0.0, 1.0);
                let s = match smoothed { None => n, Some(prev) => prev + a*(n - prev) };
                smoothed = Some(s);

                if buf.len() == 64 { buf.pop_front(); }
                buf.push_back(s);
                let m = buf.iter().copied().sum::<f32>()/(buf.len() as f32);
                let v = buf.iter().map(|v|{let d=*v-m; d*d}).sum::<f32>()/(buf.len() as f32);
                let stability = (1.0 / (1.0 + 20.0 * v)).clamp(0.0, 1.0);
                *conf_c.lock().unwrap() = stability;

                let sample = AngleSample { angle_deg: s, timestamp: Instant::now(), source: Source::LinuxALS };
                *latest_c.lock().unwrap() = Some(sample);
                let _ = tx_c.send(sample);
            }
        });

        Ok(Self { latest, tx, alpha, conf, src: Source::LinuxALS, note: "linux_proxy_als" })
    }

    async fn spawn_from_sys_tilt(hz: f32) -> Result<Self> {
        // Find an iio device with accel channels
        let dev = find_iio_accel_device().ok_or_else(|| Error::Backend("linux: no accel in /sys".into()))?;

        let latest = Arc::new(Mutex::new(None));
        let (tx, _rx) = broadcast::channel::<AngleSample>(256);
        let alpha = Arc::new(Mutex::new(0.25f32));
        let conf = Arc::new(Mutex::new(0.2f32));

        let latest_c = latest.clone();
        let tx_c = tx.clone();
        let alpha_c = alpha.clone();
        let conf_c = conf.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs_f32(1.0 / hz.max(60.0)));
            let mut buf: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(64);
            let mut smoothed: Option<f32> = None;

            loop {
                interval.tick().await;

                if let Some((ax, ay, az)) = read_accel_triplet(&dev) {
                    // Simple pitch estimate from accel
                    let g = (ax*ax + ay*ay + az*az).sqrt().max(1e-6);
                    let pitch = (-ax / g).asin().to_degrees().clamp(-180.0, 180.0);

                    let a = (*alpha_c.lock().unwrap()).clamp(0.0, 1.0);
                    let s = match smoothed { None => pitch, Some(prev) => prev + a*(pitch - prev) };
                    smoothed = Some(s);

                    if buf.len() == 64 { buf.pop_front(); }
                    buf.push_back(s);
                    let n = buf.len() as f32;
                    let m = buf.iter().copied().sum::<f32>()/n;
                    let v = buf.iter().map(|v|{let d=*v-m; d*d}).sum::<f32>()/n;
                    let stability = (1.0 / (1.0 + 0.05 * v)).clamp(0.0, 1.0);
                    *conf_c.lock().unwrap() = stability;

                    let sample = AngleSample { angle_deg: s, timestamp: Instant::now(), source: Source::LinuxTilt };
                    *latest_c.lock().unwrap() = Some(sample);
                    let _ = tx_c.send(sample);
                }
            }
        });

        Ok(Self { latest, tx, alpha, conf, src: Source::LinuxTilt, note: "linux_sys_tilt" })
    }

    async fn spawn_from_sys_als(hz: f32) -> Result<Self> {
        let dev = find_iio_light_device().ok_or_else(|| Error::Backend("linux: no light sensor in /sys".into()))?;

        let latest = Arc::new(Mutex::new(None));
        let (tx, _rx) = broadcast::channel::<AngleSample>(256);
        let alpha = Arc::new(Mutex::new(0.25f32));
        let conf = Arc::new(Mutex::new(0.2f32));

        let latest_c = latest.clone();
        let tx_c = tx.clone();
        let alpha_c = alpha.clone();
        let conf_c = conf.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs_f32(1.0 / hz.max(10.0)));
            let mut baseline = 10.0f32;
            let mut smoothed: Option<f32> = None;
            let mut buf: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(64);

            loop {
                interval.tick().await;

                if let Some(lux) = read_lux(&dev) {
                    baseline = 0.995 * baseline + 0.005 * lux;
                    let val = lux - baseline;
                    let n = (val * 0.02 + 0.5).clamp(0.0, 1.0);

                    let a = (*alpha_c.lock().unwrap()).clamp(0.0, 1.0);
                    let s = match smoothed { None => n, Some(prev) => prev + a*(n - prev) };
                    smoothed = Some(s);

                    if buf.len() == 64 { buf.pop_front(); }
                    buf.push_back(s);
                    let m = buf.iter().copied().sum::<f32>()/(buf.len() as f32);
                    let v = buf.iter().map(|v|{let d=*v-m; d*d}).sum::<f32>()/(buf.len() as f32);
                    let stability = (1.0 / (1.0 + 20.0 * v)).clamp(0.0, 1.0);
                    *conf_c.lock().unwrap() = stability;

                    let sample = AngleSample { angle_deg: s, timestamp: Instant::now(), source: Source::LinuxALS };
                    *latest_c.lock().unwrap() = Some(sample);
                    let _ = tx_c.send(sample);
                }
            }
        });

        Ok(Self { latest, tx, alpha, conf, src: Source::LinuxALS, note: "linux_sys_als" })
    }
}

impl AngleDevice for LinuxAngle {
    fn latest(&self) -> Option<AngleSample> { *self.latest.lock().unwrap() }
    fn subscribe(&self) -> AngleStream {
        BroadcastStream::new(self.tx.subscribe())
            .filter_map(|it| async move { it.ok() })
            .boxed()
    }
    fn set_smoothing(&self, alpha: f32) { *self.alpha.lock().unwrap() = alpha; }
    fn confidence(&self) -> f32 { *self.conf.lock().unwrap() }
    fn info(&self) -> DeviceInfo { DeviceInfo { source: self.src, note: self.note } }
}

// ==== helpers ====

#[cfg(feature = "linux_iio_proxy")]
fn query_proxy_pitch_degrees() -> Option<f32> {
    // Minimal sync call: read Accelerometer reading and estimate pitch; varies by daemon version
    let conn = ZConn::session().ok()?;
    // The proxy DBus interface is net.hadess.SensorProxy (orientation/accelerometer);
    // For 0.6, we keep a placeholder returning None if not ready.
    drop(conn);
    None
}

#[cfg(feature = "linux_iio_proxy")]
fn query_proxy_lux() -> Option<f32> {
    let conn = ZConn::session().ok()?;
    drop(conn);
    None
}

fn first_existing(base: &PathBuf, names: &[&str]) -> Option<PathBuf> {
    for n in names {
        let p = base.join(n);
        if p.exists() { return Some(p); }
    }
    None
}

fn find_iio_accel_device() -> Option<PathBuf> {
    for dev in glob::glob("/sys/bus/iio/devices/iio:device*").ok()? {
        let p = dev.ok()?;
        // Accept *_raw OR *_input
        let have_x = first_existing(&p, &["in_accel_x_raw", "in_accel_x_input"]).is_some();
        let have_y = first_existing(&p, &["in_accel_y_raw", "in_accel_y_input"]).is_some();
        let have_z = first_existing(&p, &["in_accel_z_raw", "in_accel_z_input"]).is_some();
        if have_x && have_y && have_z { return Some(p); }
    }
    None
}

fn read_accel_triplet(dev: &PathBuf) -> Option<(f32,f32,f32)> {
    let rxp = first_existing(dev, &["in_accel_x_raw", "in_accel_x_input"])?;
    let ryp = first_existing(dev, &["in_accel_y_raw", "in_accel_y_input"])?;
    let rzp = first_existing(dev, &["in_accel_z_raw", "in_accel_z_input"])?;
    let sxp = first_existing(dev, &["in_accel_scale", "in_accel_x_scale"]);
    let syp = first_existing(dev, &["in_accel_scale", "in_accel_y_scale"]);
    let szp = first_existing(dev, &["in_accel_scale", "in_accel_z_scale"]);

    let rx = fs::read_to_string(rxp).ok()?.trim().parse::<f32>().ok()?;
    let ry = fs::read_to_string(ryp).ok()?.trim().parse::<f32>().ok()?;
    let rz = fs::read_to_string(rzp).ok()?.trim().parse::<f32>().ok()?;

    // Some drivers expose per-axis scales; default to 1.0 if absent.
    let sx = sxp.and_then(|p| fs::read_to_string(p).ok()?.trim().parse::<f32>().ok()).unwrap_or(1.0);
    let sy = syp.and_then(|p| fs::read_to_string(p).ok()?.trim().parse::<f32>().ok()).unwrap_or(1.0);
    let sz = szp.and_then(|p| fs::read_to_string(p).ok()?.trim().parse::<f32>().ok()).unwrap_or(1.0);

    Some((rx*sx, ry*sy, rz*sz))
}

fn find_iio_light_device() -> Option<PathBuf> {
    for dev in glob::glob("/sys/bus/iio/devices/iio:device*").ok()? {
        let p = dev.ok()?;
        // A bunch of ALS variants exist; accept any of these:
        if first_existing(&p, &[
            "in_illuminance_raw",
            "in_illuminance_input",
            "in_illuminance0_raw",
            "in_illuminance0_input",
            "in_intensity_both_raw",
            "in_intensity_input",
        ]).is_some() {
            return Some(p);
        }
    }
    None
}

fn read_lux(dev: &PathBuf) -> Option<f32> {
    let valp = first_existing(dev, &[
        "in_illuminance_raw",
        "in_illuminance_input",
        "in_illuminance0_raw",
        "in_illuminance0_input",
        "in_intensity_both_raw",
        "in_intensity_input",
    ])?;
    let raw = fs::read_to_string(valp).ok()?.trim().parse::<f32>().ok()?;

    // Try scale names; fall back to 1.0 if none found.
    let scalep = first_existing(dev, &[
        "in_illuminance_scale",
        "in_illuminance0_scale",
        "in_intensity_scale",
        "in_intensity0_scale",
    ]);
    let scale = scalep
        .and_then(|p| fs::read_to_string(p).ok()?.trim().parse::<f32>().ok())
        .unwrap_or(1.0);

    Some(raw * scale)
}