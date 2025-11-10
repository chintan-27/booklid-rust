//! Public API surface, backend selection, and blocking helpers.

#[cfg(feature = "mac_hid_feature")]
mod backend_hidapi;

#[cfg(feature = "mac_als")]
mod backend_mac_als;

#[cfg(feature = "mock")]
mod backend_mock;

#[cfg(all(target_os = "windows", feature = "win_sensors"))]
mod backend_win;

#[cfg(all(
    target_os = "linux",
    any(feature = "linux_iio_proxy", feature = "linux_iio_sys")
))]
mod backend_linux;

pub mod types;
pub use crate::types::{AngleSample, Error, Result, Source};

use futures_util::stream::BoxStream;
use once_cell::sync::Lazy;
use std::time::{Duration, Instant};

const HAS_BACKENDS: bool = cfg!(any(
    feature = "mac_hid_feature",
    feature = "mac_als",
    feature = "mock",
    all(target_os = "windows", feature = "win_sensors"),
    all(
        target_os = "linux",
        any(feature = "linux_iio_proxy", feature = "linux_iio_sys")
    )
));

pub type AngleStream = BoxStream<'static, AngleSample>;
pub type AngleClient = Box<dyn AngleDevice + Send + Sync>;

// ===== Device info =====

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub source: crate::Source,
    /// Short backend note like "mac_hid_feature", "mac_als", or "mock".
    pub note: &'static str,
}

// ===== Trait =====

pub trait AngleDevice: Send + Sync {
    fn latest(&self) -> Option<AngleSample>;
    fn subscribe(&self) -> AngleStream;
    /// Exponential smoothing alpha in [0.0, 1.0]
    fn set_smoothing(&self, alpha: f32);
    /// Confidence in [0.0, 1.0]
    fn confidence(&self) -> f32;
    fn info(&self) -> DeviceInfo;
}

// ===== Global Tokio runtime for blocking variants =====

static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to init Tokio runtime")
});

/// Blocking open functions below create/use a global multithreaded Tokio runtime.
/// Avoid calling them from async contexts.

// ===== Open options =====

#[derive(Clone, Debug)]
pub struct OpenOptions {
    pub hz: f32,
    pub smoothing_init: f32,
    pub allow_mock: bool,
    /// Reserved for future use (per-backend timeouts / fail_after).
    pub timeout: Duration,
    /// macOS-specific: whether to attempt HID "discovery" mode.
    pub discovery: bool,
}

impl OpenOptions {
    pub fn new(hz: f32) -> Self {
        Self {
            hz,
            smoothing_init: 0.25,
            allow_mock: false,
            timeout: Duration::from_secs(3),
            discovery: true,
        }
    }
    pub fn smoothing(mut self, alpha: f32) -> Self {
        self.smoothing_init = alpha;
        self
    }
    pub fn allow_mock(mut self, ok: bool) -> Self {
        self.allow_mock = ok;
        self
    }
    pub fn discovery(mut self, on: bool) -> Self {
        self.discovery = on;
        self
    }
}

// ===== Internal init config & report =====

pub struct InitConfig {
    pub hz: f32,
    pub smoothing_init: f32,
    pub allow_mock: bool,
    pub timeout: Duration,
    pub discovery: bool,
}
impl InitConfig {
    pub fn new(hz: f32) -> Self {
        Self {
            hz,
            smoothing_init: 0.25,
            allow_mock: false,
            timeout: Duration::from_secs(3),
            discovery: true,
        }
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

// ===== Desktop guard =====

fn desktop_guard() -> bool {
    // Simple env-driven guard for now; replace with a chassis/model check later.
    std::env::var("BOOKLID_DESKTOP").ok().as_deref() == Some("1")
}

// ===== Confidence gate wrapper (only used when any backend feature is enabled) =====

#[cfg(any(
    feature = "mac_hid_feature",
    feature = "mac_als",
    feature = "mock",
    all(target_os = "windows", feature = "win_sensors"),
    all(
        target_os = "linux",
        any(feature = "linux_iio_proxy", feature = "linux_iio_sys")
    )
))]
mod gating {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub struct Gated {
        inner: AngleClient,
        live: AtomicBool,
        min: f32,
        drop: f32,
    }

    impl Gated {
        pub fn wrap(inner: AngleClient, min: f32, drop: f32) -> AngleClient {
            Box::new(Self {
                inner,
                live: AtomicBool::new(false),
                min,
                drop,
            })
        }

        #[inline]
        fn bump(&self) {
            let c = self.inner.confidence();
            let live = self.live.load(Ordering::Relaxed);
            if !live && c >= self.min {
                self.live.store(true, Ordering::Relaxed);
            } else if live && c < self.drop {
                self.live.store(false, Ordering::Relaxed);
            }
        }
    }

    impl AngleDevice for Gated {
        fn latest(&self) -> Option<AngleSample> {
            self.bump();
            if self.live.load(Ordering::Relaxed) {
                self.inner.latest()
            } else {
                None
            }
        }

        // Keep the stream pass-through; most UIs use latest() to decide "waiting..."
        fn subscribe(&self) -> AngleStream {
            self.inner.subscribe()
        }

        fn set_smoothing(&self, alpha: f32) {
            self.inner.set_smoothing(alpha)
        }

        fn confidence(&self) -> f32 {
            self.inner.confidence()
        }

        fn info(&self) -> DeviceInfo {
            self.inner.info()
        }
    }
}

#[cfg(any(
    feature = "mac_hid_feature",
    feature = "mac_als",
    feature = "mock",
    all(target_os = "windows", feature = "win_sensors"),
    all(
        target_os = "linux",
        any(feature = "linux_iio_proxy", feature = "linux_iio_sys")
    )
))]
use gating::Gated;

// ===== Unified init (mac-first today) =====

pub async fn init(cfg: InitConfig) -> Result<(AngleClient, SetupReport)> {
    if !HAS_BACKENDS {
        return Err(Error::Backend(
            "no backends enabled; enable one of: mac_hid_feature, mac_als, mock, win_sensors, linux_iio_proxy, linux_iio_sys".into()
        ));
    }
    let t0 = Instant::now();
    #[allow(unused_mut)]
    let mut tried: Vec<Source> = Vec::new();
    let guard = desktop_guard();

    // Desktop → skip hinge entirely and go straight to ALS.
    if guard {
        #[cfg(feature = "mac_als")]
        {
            tried.push(Source::ALS);
            if let Ok(dev) = backend_mac_als::AlsAngle::open(cfg.hz).await {
                let dev: AngleClient = Box::new(dev);
                dev.set_smoothing(cfg.smoothing_init);
                // Confidence gating
                let dev: AngleClient = Gated::wrap(dev, 0.70, 0.65);
                let report = SetupReport {
                    chosen: Some(Source::ALS),
                    tried,
                    desktop_guard: guard,
                    used_mock: false,
                    duration: t0.elapsed(),
                };
                if std::env::var("BOOKLID_DIAGNOSTICS").ok().as_deref() == Some("1") {
                    eprintln!(
                        "booklid: chosen={:?} tried={:?} guard={} min=0.70 drop=0.65 hz={:.1} smoothing={:.2}",
                        report.chosen,
                        report.tried,
                        report.desktop_guard,
                        cfg.hz,
                        cfg.smoothing_init
                    );
                }
                return Ok((dev, report));
            }
        }
        // No ALS available or failed
        let msg = format!("no suitable backend available; tried: {tried:?}");
        return Err(Error::Backend(msg));
    }

    // Laptop path: Hinge Feature first
    #[cfg(feature = "mac_hid_feature")]
    {
        tried.push(Source::HingeFeature);
        if let Ok(dev) = backend_hidapi::HidAngle::open(cfg.hz).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let dev: AngleClient = Gated::wrap(dev, 0.70, 0.65);
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

    // Then Hinge "discovery" path (tag distinctly as HingeHid)
    #[cfg(feature = "mac_hid_feature")]
    {
        tried.push(Source::HingeHid);
        if let Ok(dev) = backend_hidapi::HidAngle::open_with(cfg.hz, cfg.discovery).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let dev: AngleClient = Gated::wrap(dev, 0.70, 0.65);
            let report = SetupReport {
                chosen: Some(Source::HingeHid),
                tried,
                desktop_guard: guard,
                used_mock: false,
                duration: t0.elapsed(),
            };
            return Ok((dev, report));
        }
    }

    // Fallback: ALS (e.g., desktop-like behavior or hinge unavailable)
    #[cfg(feature = "mac_als")]
    {
        tried.push(Source::ALS);
        if let Ok(dev) = backend_mac_als::AlsAngle::open(cfg.hz).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let dev: AngleClient = Gated::wrap(dev, 0.70, 0.65);
            let report = SetupReport {
                chosen: Some(Source::ALS),
                tried,
                desktop_guard: guard,
                used_mock: false,
                duration: t0.elapsed(),
            };
            return Ok((dev, report));
        }
    }

    // Optional mock (strictly opt-in)
    #[cfg(feature = "mock")]
    if cfg.allow_mock {
        tried.push(Source::Mock);
        if let Ok(dev) = backend_mock::MockAngle::open(cfg.hz).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let dev: AngleClient = Gated::wrap(dev, 0.70, 0.65);
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

    // Windows sensors chain
    #[cfg(all(target_os = "windows", feature = "win_sensors"))]
    {
        // Prefer hinge, then tilt, then ALS
        tried.push(Source::WinHinge);
        if let Ok(dev) = backend_win::WinAngle::open_hinge(cfg.hz).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let dev: AngleClient = Gated::wrap(dev, 0.70, 0.65);
            let report = SetupReport {
                chosen: Some(Source::WinHinge),
                tried,
                desktop_guard: guard,
                used_mock: false,
                duration: t0.elapsed(),
            };
            return Ok((dev, report));
        }

        tried.push(Source::WinTilt);
        if let Ok(dev) = backend_win::WinAngle::open_tilt(cfg.hz).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let dev: AngleClient = Gated::wrap(dev, 0.70, 0.65);
            let report = SetupReport {
                chosen: Some(Source::WinTilt),
                tried,
                desktop_guard: guard,
                used_mock: false,
                duration: t0.elapsed(),
            };
            return Ok((dev, report));
        }

        tried.push(Source::WinALS);
        if let Ok(dev) = backend_win::WinAngle::open_als(cfg.hz).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let dev: AngleClient = Gated::wrap(dev, 0.70, 0.65);
            let report = SetupReport {
                chosen: Some(Source::WinALS),
                tried,
                desktop_guard: guard,
                used_mock: false,
                duration: t0.elapsed(),
            };
            return Ok((dev, report));
        }
    }

    // Linux iio chain
    #[cfg(all(
        target_os = "linux",
        any(feature = "linux_iio_proxy", feature = "linux_iio_sys")
    ))]
    {
        // First try dbus (iio-sensor-proxy), then direct /sys
        tried.push(Source::LinuxTilt);
        if let Ok(dev) = backend_linux::LinuxAngle::open_tilt(cfg.hz).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let dev: AngleClient = Gated::wrap(dev, 0.70, 0.65);
            let report = SetupReport {
                chosen: Some(Source::LinuxTilt),
                tried,
                desktop_guard: guard,
                used_mock: false,
                duration: t0.elapsed(),
            };
            return Ok((dev, report));
        }

        tried.push(Source::LinuxALS);
        if let Ok(dev) = backend_linux::LinuxAngle::open_als(cfg.hz).await {
            let dev: AngleClient = Box::new(dev);
            dev.set_smoothing(cfg.smoothing_init);
            let dev: AngleClient = Gated::wrap(dev, 0.70, 0.65);
            let report = SetupReport {
                chosen: Some(Source::LinuxALS),
                tried,
                desktop_guard: guard,
                used_mock: false,
                duration: t0.elapsed(),
            };
            return Ok((dev, report));
        }
    }

    let msg = format!("no suitable backend available; tried: {tried:?}");
    Err(Error::Backend(msg))
}

// ===== Public API (thin) =====

/// Async: open with just a frequency (Hz).
pub async fn open(hz: f32) -> Result<AngleClient> {
    let (dev, _report) = init(InitConfig::new(hz)).await?;
    Ok(dev)
}

/// Async: open with options (allow_mock, smoothing, discovery, etc.)
pub async fn open_with(opts: OpenOptions) -> Result<AngleClient> {
    let (dev, _report) = init(InitConfig {
        hz: opts.hz,
        smoothing_init: opts.smoothing_init,
        allow_mock: opts.allow_mock,
        timeout: opts.timeout,
        discovery: opts.discovery,
    })
    .await?;
    Ok(dev)
}

/// Blocking: open with just a frequency (Hz).
/// Uses a global multithreaded Tokio runtime — avoid calling from async contexts.
pub fn open_blocking(hz: f32) -> Result<AngleClient> {
    let (dev, _report) = RUNTIME.block_on(init(InitConfig::new(hz)))?;
    Ok(dev)
}

/// Blocking: open with options.
/// Uses a global multithreaded Tokio runtime — avoid calling from async contexts.
pub fn open_blocking_with(opts: OpenOptions) -> Result<AngleClient> {
    let (dev, _report) = RUNTIME.block_on(init(InitConfig {
        hz: opts.hz,
        smoothing_init: opts.smoothing_init,
        allow_mock: opts.allow_mock,
        timeout: opts.timeout,
        discovery: opts.discovery,
    }))?;
    Ok(dev)
}

/// Back-compat shim (can be removed in a later release).
pub async fn open_default(hz: f32) -> Result<AngleClient> {
    open(hz).await
}
