#![cfg(all(target_os = "windows", feature = "win_sensors"))]

use crate::{AngleDevice, AngleSample, AngleStream, DeviceInfo, Result, Source};
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
use windows::Devices::Sensors::{
    HingeAngleSensor, Inclinometer, LightSensor
};
use windows::Foundation::TypedEventHandler;

pub struct WinAngle {
    latest: Arc<Mutex<Option<AngleSample>>>,
    tx: broadcast::Sender<AngleSample>,
    alpha: Arc<Mutex<f32>>,
    conf: Arc<Mutex<f32>>,
    src: Source,
    note: &'static str,
}

impl WinAngle {
    pub async fn open_hinge(hz: f32) -> Result<Self> {
        unsafe {
            let sensor = HingeAngleSensor::GetDefault().map_err(|e| crate::Error::Backend(format!("win hinge: {e:?}")))?;
            Self::spawn_from_hinge(sensor, hz).await
        }
    }

    pub async fn open_tilt(hz: f32) -> Result<Self> {
        unsafe {
            let incl = Inclinometer::GetDefault().map_err(|e| crate::Error::Backend(format!("win inclinometer: {e:?}")))?;
            Self::spawn_from_tilt(incl, hz).await
        }
    }

    pub async fn open_als(hz: f32) -> Result<Self> {
        unsafe {
            let ls = LightSensor::GetDefault().map_err(|e| crate::Error::Backend(format!("win light: {e:?}")))?;
            Self::spawn_from_als(ls, hz).await
        }
    }

    async fn spawn_from_hinge(sensor: HingeAngleSensor, hz: f32) -> Result<Self> {
        let latest = Arc::new(Mutex::new(None));
        let (tx, _rx) = broadcast::channel::<AngleSample>(256);
        let alpha = Arc::new(Mutex::new(0.25f32));
        let conf = Arc::new(Mutex::new(0.2f32));

        let latest_c = latest.clone();
        let tx_c = tx.clone();
        let alpha_c = alpha.clone();
        let conf_c = conf.clone();

        // Event-based; still sample on a timer to keep a steady rate
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs_f32(1.0 / hz.max(20.0)));
            let mut buf: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(64);

            // Hook WinRT event → shared cell
            let angle_cell = Arc::new(Mutex::new(None::<f32>));
            let angle_cell_c = angle_cell.clone();

            let _token = sensor.ReadingChanged(
                &TypedEventHandler::new(move |_, args| {
                    if let Some(args) = args {
                        if let Ok(reading) = args.Reading() {
                            if let Ok(deg) = reading.AngleInDegrees() {
                                *angle_cell_c.lock().unwrap() = Some(deg as f32);
                            }
                        }
                    }
                    Ok(())
                })
            ).ok();

            let mut last: Option<f32> = None;
            let mut smoothed: Option<f32> = None;

            loop {
                interval.tick().await;

                let raw = *angle_cell.lock().unwrap();
                if let Some(mut deg) = raw {
                    // sanity clamp (0..180 typical, but don’t crash if exotic)
                    if !( -5.0..=365.0 ).contains(&deg) { continue; }
                    // EMA smoothing
                    let a = (*alpha_c.lock().unwrap()).clamp(0.0, 1.0);
                    let s = match smoothed { None => deg, Some(prev) => prev + a*(deg - prev) };
                    smoothed = Some(s);
                    last = Some(s);

                    // confidence from variance
                    if buf.len() == 64 { buf.pop_front(); }
                    buf.push_back(s);
                    let n = buf.len() as f32;
                    let mean = buf.iter().copied().sum::<f32>()/n;
                    let var = buf.iter().map(|v|{let d=*v-mean; d*d}).sum::<f32>()/n;
                    let stability = (1.0 / (1.0 + 0.02 * var)).clamp(0.0, 1.0); // hinge is fairly stable
                    *conf_c.lock().unwrap() = stability;

                    let sample = AngleSample { angle_deg: s, timestamp: Instant::now(), source: Source::WinHinge };
                    *latest_c.lock().unwrap() = Some(sample);
                    let _ = tx_c.send(sample);
                }
            }
        });

        Ok(Self { latest, tx, alpha, conf, src: Source::WinHinge, note: "win_hinge" })
    }

    async fn spawn_from_tilt(incl: Inclinometer, hz: f32) -> Result<Self> {
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
                unsafe {
                    if let Ok(Some(r)) = incl.GetCurrentReading().map(|x| x.and_then(|x| Ok(Some(x))).unwrap_or(None) ) {
                        let pitch = r.PitchDegrees().unwrap_or(0.0) as f32;
                        // Depending on device orientation, roll or pitch may better approximate lid angle;
                        // for now, publish pitch as our angle.
                        let deg = pitch.clamp(-180.0, 180.0);

                        let a = (*alpha_c.lock().unwrap()).clamp(0.0, 1.0);
                        let s = match smoothed { None => deg, Some(prev) => prev + a*(deg - prev) };
                        smoothed = Some(s);

                        if buf.len() == 64 { buf.pop_front(); }
                        buf.push_back(s);
                        let n = buf.len() as f32;
                        let mean = buf.iter().copied().sum::<f32>()/n;
                        let var = buf.iter().map(|v|{let d=*v-mean; d*d}).sum::<f32>()/n;
                        let stability = (1.0 / (1.0 + 0.05 * var)).clamp(0.0, 1.0);
                        *conf_c.lock().unwrap() = stability;

                        let sample = AngleSample { angle_deg: s, timestamp: Instant::now(), source: Source::WinTilt };
                        *latest_c.lock().unwrap() = Some(sample);
                        let _ = tx_c.send(sample);
                    }
                }
            }
        });

        Ok(Self { latest, tx, alpha, conf, src: Source::WinTilt, note: "win_tilt" })
    }

    async fn spawn_from_als(ls: LightSensor, hz: f32) -> Result<Self> {
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
                unsafe {
                    if let Ok(Some(r)) = ls.GetCurrentReading().map(|x| x.and_then(|x| Ok(Some(x))).unwrap_or(None) ) {
                        let lux = r.IlluminanceInLux().unwrap_or(1.0) as f32;
                        baseline = 0.995 * baseline + 0.005 * lux;
                        let mut val = lux - baseline;
                        // normalize to 0..1 “bellows”
                        let mut n = (val * 0.02 + 0.5).clamp(0.0, 1.0);

                        let a = (*alpha_c.lock().unwrap()).clamp(0.0, 1.0);
                        let s = match smoothed { None => n, Some(prev) => prev + a*(n - prev) };
                        smoothed = Some(s);

                        if buf.len() == 64 { buf.pop_front(); }
                        buf.push_back(s);
                        let m = buf.iter().copied().sum::<f32>()/(buf.len() as f32);
                        let v = buf.iter().map(|v|{let d=*v-m; d*d}).sum::<f32>()/(buf.len() as f32);
                        let stability = (1.0 / (1.0 + 20.0 * v)).clamp(0.0, 1.0);
                        *conf_c.lock().unwrap() = stability;

                        let sample = AngleSample { angle_deg: s, timestamp: Instant::now(), source: Source::WinALS };
                        *latest_c.lock().unwrap() = Some(sample);
                        let _ = tx_c.send(sample);
                    }
                }
            }
        });

        Ok(Self { latest, tx, alpha, conf, src: Source::WinALS, note: "win_als" })
    }
}

impl AngleDevice for WinAngle {
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
