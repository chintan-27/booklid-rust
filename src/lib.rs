mod types;

#[cfg(feature = "mac_hid_feature")]
mod backend_hidapi;

#[cfg(feature = "mock")]
mod backend_mock;

// Public re-exports
pub use types::{AngleSample, Error, Result, Source};

#[cfg(feature = "mock")]
pub use backend_mock::MockAngle;

use futures_util::stream::BoxStream;
pub type AngleStream = BoxStream<'static, AngleSample>;
pub type AngleClient = Box<dyn AngleDevice + Send + Sync>;

pub trait AngleDevice: Send + Sync {
    fn latest(&self) -> Option<AngleSample>;
    fn subscribe(&self) -> AngleStream;
    fn set_smoothing(&self, alpha: f32);
    fn confidence(&self) -> f32;
}

pub async fn open_default(hz: f32) -> Result<AngleClient> {
    #[cfg(feature = "mac_hid_feature")]
    { return Ok(Box::new(backend_hidapi::HidAngle::open(hz).await?)); }
    #[cfg(not(feature = "mac_hid_feature"))]
    { let _ = hz; return Err(Error::Backend("no backend enabled".into())); }
}

pub struct OpenConfig {
    pub hz: f32,
    pub allow_mock: bool,
    pub min_confidence: f32,
}
impl OpenConfig {
    pub fn new(hz: f32) -> Self {
        Self { hz, allow_mock: false, min_confidence: 0.7 }
    }
}
pub async fn open_with_config(cfg: OpenConfig) -> Result<AngleClient> {
    open_default(cfg.hz).await
}