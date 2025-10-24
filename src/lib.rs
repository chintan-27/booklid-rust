mod types;

#[cfg(feature = "mac_hid_feature")]
mod backend_hidapi;

#[cfg(feature = "mock")]
mod backend_mock;

pub use types::{AngleSample, Error, Result, Source};

use futures_util::stream::BoxStream;
use once_cell::sync::Lazy;
use std::time::Duration;

pub type AngleStream = BoxStream<'static, AngleSample>;
pub type AngleClient = Box<dyn AngleDevice + Send + Sync>;

// Device info returned by AngleDevice::info()
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub source: Source,
    pub note: &'static str, // short backend note like "mac_hid_feature" or "mock"
}

pub trait AngleDevice: Send + Sync {
    fn latest(&self) -> Option<AngleSample>;
    fn subscribe(&self) -> AngleStream;
    fn set_smoothing(&self, alpha: f32);
    fn confidence(&self) -> f32;
    fn info(&self) -> DeviceInfo; // NEW
}

// Global Tokio runtime for blocking open
static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to init Tokio runtime")
});

// Simple options for opening a device
#[derive(Clone, Debug)]
pub struct OpenOptions {
    pub hz: f32,
    pub smoothing_init: f32,
    pub allow_mock: bool,
    pub timeout: Duration, // reserved for future, not used yet
}

impl OpenOptions {
    pub fn new(hz: f32) -> Self {
        Self {
            hz,
            smoothing_init: 0.25,
            allow_mock: false,
            timeout: Duration::from_secs(3),
        }
    }
    pub fn smoothing(mut self, alpha: f32) -> Self { self.smoothing_init = alpha; self }
    pub fn allow_mock(mut self, ok: bool) -> Self { self.allow_mock = ok; self }
}

// Internal init config/report you already had (keep as-is if present)
use std::time::Instant;

pub struct InitConfig {
    pub hz: f32,
    pub smoothing_init: f32,
    pub allow_mock: bool,
    pub timeout: Duration,
}
impl InitConfig {
    pub fn new(hz: f32) -> Self {
        Self { hz, smoothing_init: 0.25, allow_mock: false, timeout: Duration::from_secs(3) }
    }
}
#[derive(Debug, Clone)]
pub struct SetupReport {
    pub chosen: Option<Source>,
    pub tried: Vec<Source>,
    pub desktop_guard: bool,
    pub used_mock: bool,
    pub duration: Duration,
}

// Desktop guard stub (env-driven for now)
fn desktop_guard() -> bool {
    std::env::var("BOOKLID_DESKTOP").ok().as_deref() == Some("1")
}

// Unified init that tries mac HID, then optional mock (you may already have this)
pub async fn init(cfg: InitConfig) -> Result<(AngleClient, SetupReport)> {
    let t0 = Instant::now();
    let mut tried = Vec::new();
    let guard = desktop_guard();

    // mac HID feature backend
    #[cfg(feature = "mac_hid_feature")]
    {
        tried.push(Source::HingeFeature);
        if let Ok(dev) = backend_hidapi::HidAngle::open(cfg.hz).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let report = SetupReport {
                chosen: Some(Source::HingeFeature),
                tried,
                desktop_guard: guard,
                used_mock: false,
                duration: t0.elapsed(),
            };
            return Ok((dev, report));
        }
    }

    // Optional mock fallback (explicit opt-in)
    #[cfg(feature = "mock")]
    if cfg.allow_mock {
        tried.push(Source::Mock);
        if let Ok(dev) = backend_mock::MockAngle::open(cfg.hz).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let report = SetupReport {
                chosen: Some(Source::Mock),
                tried,
                desktop_guard: guard,
                used_mock: true,
                duration: t0.elapsed(),
            };
            return Ok((dev, report));
        }
    }

    Err(Error::Backend("no suitable backend available".into()))
}

// SIMPLE PUBLIC API

// Async: open with just a frequency
pub async fn open(hz: f32) -> Result<AngleClient> {
    let (dev, _report) = init(InitConfig::new(hz)).await?;
    Ok(dev)
}

// Async: open with options (allow_mock, smoothing, etc.)
pub async fn open_with(opts: OpenOptions) -> Result<AngleClient> {
    let (dev, _report) = init(InitConfig {
        hz: opts.hz,
        smoothing_init: opts.smoothing_init,
        allow_mock: opts.allow_mock,
        timeout: opts.timeout,
    }).await?;
    Ok(dev)
}

// Blocking variants for non-async apps
pub fn open_blocking(hz: f32) -> Result<AngleClient> {
    let (dev, _report) = RUNTIME.block_on(init(InitConfig::new(hz)))?;
    Ok(dev)
}
pub fn open_blocking_with(opts: OpenOptions) -> Result<AngleClient> {
    let (dev, _report) = RUNTIME.block_on(init(InitConfig {
        hz: opts.hz,
        smoothing_init: opts.smoothing_init,
        allow_mock: opts.allow_mock,
        timeout: opts.timeout,
    }))?;
    Ok(dev)
}

// Back-compat shim (optional): keep old name temporarily
pub async fn open_default(hz: f32) -> Result<AngleClient> {
    open(hz).await
}