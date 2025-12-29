//! Public API surface, backend selection, and blocking helpers.

#[cfg(feature = "mac_hid_feature")]
mod backend_hidapi;
#[cfg(all(
    target_os = "linux",
    any(feature = "linux_iio_proxy", feature = "linux_iio_sys")
))]
mod backend_linux;
#[cfg(feature = "mac_als")]
mod backend_mac_als;
#[cfg(feature = "mock")]
mod backend_mock;
#[cfg(all(target_os = "windows", feature = "win_sensors"))]
mod backend_win;

mod persist;

pub mod types;
pub use crate::types::{AngleSample, Error, Result, Source};

use futures_util::stream::BoxStream;
use once_cell::sync::Lazy;
use std::time::Duration;

pub type AngleStream = BoxStream<'static, AngleSample>;
pub type AngleClient = Box<dyn AngleDevice + Send + Sync>;

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

// ===== Device info =====

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub source: Source,
    pub note: &'static str,
}

// ===== Trait =====

pub trait AngleDevice: Send + Sync {
    fn latest(&self) -> Option<AngleSample>;
    fn subscribe(&self) -> AngleStream;
    fn set_smoothing(&self, alpha: f32);
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

// ===== OpenConfig (1.0) =====

#[derive(Clone, Debug)]
pub struct OpenConfig {
    pub hz: f32,
    pub smoothing_alpha: f32,
    pub min_confidence: f32,
    pub prefer_sources: Vec<Source>,
    pub disable_backends: Vec<Source>,
    pub discovery: bool,
    pub allow_mock: bool,
    pub diagnostics: bool,
    pub fail_after: Duration,
    pub persistence: bool,
}

impl OpenConfig {
    pub fn new(hz: f32) -> Self {
        Self {
            hz,
            smoothing_alpha: 0.25,
            min_confidence: 0.70,
            prefer_sources: vec![],
            disable_backends: vec![],
            discovery: true,
            allow_mock: false,
            diagnostics: false,
            fail_after: Duration::from_secs(3),
            persistence: true,
        }
    }

    pub fn smoothing(mut self, a: f32) -> Self {
        self.smoothing_alpha = a;
        self
    }
    pub fn min_confidence(mut self, m: f32) -> Self {
        self.min_confidence = m;
        self
    }
    pub fn prefer(mut self, v: Vec<Source>) -> Self {
        self.prefer_sources = v;
        self
    }
    pub fn disable(mut self, v: Vec<Source>) -> Self {
        self.disable_backends = v;
        self
    }
    pub fn discovery(mut self, on: bool) -> Self {
        self.discovery = on;
        self
    }
    pub fn allow_mock(mut self, ok: bool) -> Self {
        self.allow_mock = ok;
        self
    }
    pub fn diagnostics(mut self, on: bool) -> Self {
        self.diagnostics = on;
        self
    }
    pub fn fail_after(mut self, d: Duration) -> Self {
        self.fail_after = d;
        self
    }
    pub fn persistence(mut self, on: bool) -> Self {
        self.persistence = on;
        self
    }

    pub fn validate(mut self) -> Result<Self> {
        if self.hz <= 0.0 {
            return Err(Error::Other("hz must be > 0".into()));
        }
        self.smoothing_alpha = self.smoothing_alpha.clamp(0.0, 1.0);
        self.min_confidence = self.min_confidence.clamp(0.0, 1.0);
        if self
            .prefer_sources
            .iter()
            .any(|s| self.disable_backends.contains(s))
        {
            return Err(Error::Other(
                "prefer_sources intersects disable_backends".into(),
            ));
        }
        Ok(self)
    }
}

// ===== Internal init config =====

struct InitConfig {
    hz: f32,
    smoothing_alpha: f32,
    min_confidence: f32,
    prefer_sources: Vec<Source>,
    disable_backends: Vec<Source>,

    #[cfg_attr(not(feature = "mac_hid_feature"), allow(dead_code))]
    discovery: bool,

    #[cfg_attr(not(feature = "mock"), allow(dead_code))]
    allow_mock: bool,

    diagnostics: bool,
    persistence: bool,
}

impl InitConfig {
    fn from_open(cfg: OpenConfig) -> Result<Self> {
        let cfg = cfg.validate()?;
        Ok(Self {
            hz: cfg.hz,
            smoothing_alpha: cfg.smoothing_alpha,
            min_confidence: cfg.min_confidence,
            prefer_sources: cfg.prefer_sources,
            disable_backends: cfg.disable_backends,
            discovery: cfg.discovery,
            allow_mock: cfg.allow_mock && cfg!(feature = "mock"),
            diagnostics: cfg.diagnostics
                || std::env::var("BOOKLID_DIAGNOSTICS").ok().as_deref() == Some("1"),
            persistence: cfg.persistence,
        })
    }
}

// ===== Desktop guard =====

fn desktop_guard() -> bool {
    std::env::var("BOOKLID_DESKTOP").ok().as_deref() == Some("1")
}

// ===== Confidence gate =====

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
        pub fn wrap(inner: AngleClient, min: f32) -> AngleClient {
            let drop = (min - 0.05).clamp(0.0, 1.0);
            Box::new(Self {
                inner,
                live: AtomicBool::new(false),
                min,
                drop,
            })
        }

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
        fn subscribe(&self) -> AngleStream {
            self.inner.subscribe()
        }
        fn set_smoothing(&self, a: f32) {
            self.inner.set_smoothing(a)
        }
        fn confidence(&self) -> f32 {
            self.inner.confidence()
        }
        fn info(&self) -> DeviceInfo {
            self.inner.info()
        }
    }
}

use gating::Gated;

// ===== Unified init =====

async fn init_all(cfg: InitConfig) -> Result<AngleClient> {
    let InitConfig {
        #[cfg_attr(
            not(any(
                feature = "mac_hid_feature",
                feature = "mac_als",
                feature = "mock",
                all(target_os = "windows", feature = "win_sensors"),
                all(
                    target_os = "linux",
                    any(feature = "linux_iio_proxy", feature = "linux_iio_sys")
                )
            )),
            allow(unused_variables)
        )]
        hz,
        smoothing_alpha,
        min_confidence,
        prefer_sources,
        disable_backends,
        #[cfg_attr(not(feature = "mac_hid_feature"), allow(unused_variables))]
        discovery,
        #[cfg_attr(not(feature = "mock"), allow(unused_variables))]
        allow_mock,
        diagnostics,
        persistence,
    } = cfg;

    if !HAS_BACKENDS {
        return Err(Error::Backend(
            "no backends enabled; enable platform features".into(),
        ));
    }

    let mut tried = Vec::new();

    // Persistence: try last source first
    let persisted = if persistence {
        persist::load().last_source
    } else {
        None
    };

    let mut order: Vec<Source> = vec![
        Source::HingeFeature,
        Source::HingeHid,
        Source::ALS,
        Source::WinHinge,
        Source::WinTilt,
        Source::WinALS,
        Source::LinuxTilt,
        Source::LinuxALS,
        Source::Mock,
    ];

    order.retain(|s| !disable_backends.contains(s));
    if let Some(p) = persisted {
        if order.contains(&p) {
            order.retain(|s| s != &p);
            order.insert(0, p);
        }
    }
    for p in prefer_sources.iter().rev() {
        if order.contains(p) {
            order.retain(|s| s != p);
            order.insert(0, *p);
        }
    }

    let _guard = desktop_guard();

    for src in order {
        tried.push(src);

        // IMPORTANT: unify all backend returns into a single concrete type:
        // Option<AngleClient> (boxed trait object).
        let dev: Option<AngleClient> = match src {
            #[cfg(feature = "mac_hid_feature")]
            Source::HingeFeature if !_guard => backend_hidapi::HidAngle::open(hz)
                .await
                .ok()
                .map(|d| Box::new(d) as AngleClient),

            #[cfg(feature = "mac_hid_feature")]
            Source::HingeHid if !_guard => backend_hidapi::HidAngle::open_with(hz, discovery)
                .await
                .ok()
                .map(|d| Box::new(d) as AngleClient),

            #[cfg(feature = "mac_als")]
            Source::ALS => backend_mac_als::AlsAngle::open(hz)
                .await
                .ok()
                .map(|d| Box::new(d) as AngleClient),

            #[cfg(all(target_os = "windows", feature = "win_sensors"))]
            Source::WinHinge => backend_win::WinAngle::open_hinge(hz)
                .await
                .ok()
                .map(|d| Box::new(d) as AngleClient),

            #[cfg(all(target_os = "windows", feature = "win_sensors"))]
            Source::WinTilt => backend_win::WinAngle::open_tilt(hz)
                .await
                .ok()
                .map(|d| Box::new(d) as AngleClient),

            #[cfg(all(target_os = "windows", feature = "win_sensors"))]
            Source::WinALS => backend_win::WinAngle::open_als(hz)
                .await
                .ok()
                .map(|d| Box::new(d) as AngleClient),

            #[cfg(all(
                target_os = "linux",
                any(feature = "linux_iio_proxy", feature = "linux_iio_sys")
            ))]
            Source::LinuxTilt => backend_linux::LinuxAngle::open_tilt(hz)
                .await
                .ok()
                .map(|d| Box::new(d) as AngleClient),

            #[cfg(all(
                target_os = "linux",
                any(feature = "linux_iio_proxy", feature = "linux_iio_sys")
            ))]
            Source::LinuxALS => backend_linux::LinuxAngle::open_als(hz)
                .await
                .ok()
                .map(|d| Box::new(d) as AngleClient),

            #[cfg(feature = "mock")]
            Source::Mock if allow_mock => backend_mock::MockAngle::open(hz)
                .await
                .ok()
                .map(|d| Box::new(d) as AngleClient),

            _ => None,
        };

        if let Some(dev) = dev {
            dev.set_smoothing(smoothing_alpha);
            let dev = Gated::wrap(dev, min_confidence);

            if persistence {
                persist::store(&persist::PersistedState {
                    last_source: Some(src),
                })
                .ok();
            }

            if diagnostics {
                eprintln!("booklid: chosen={:?} tried={:?}", src, tried);
            }
            return Ok(dev);
        }
    }

    Err(Error::NoBackend { tried })
}

// ===== Public API =====

pub async fn open(hz: f32) -> Result<AngleClient> {
    open_with_config(OpenConfig::new(hz)).await
}

pub async fn open_with_config(cfg: OpenConfig) -> Result<AngleClient> {
    let init = InitConfig::from_open(cfg)?;
    init_all(init).await
}

pub fn open_blocking(hz: f32) -> Result<AngleClient> {
    open_blocking_with_config(OpenConfig::new(hz))
}

pub fn open_blocking_with_config(cfg: OpenConfig) -> Result<AngleClient> {
    let init = InitConfig::from_open(cfg)?;
    RUNTIME.block_on(init_all(init))
}

pub fn clear_persisted_state() -> Result<()> {
    persist::clear()
}
