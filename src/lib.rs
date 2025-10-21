mod types;
#[cfg(feature = "hidapi")]
mod backend_hidapi;
#[cfg(feature = "mock")]
mod backend_mock;

pub use types::{AngleSample, Error, Result, Source};

use futures_core::Stream;

pub trait AngleDevice: Send + Sync {
    fn latest(&self) -> Option<AngleSample>;
    fn stream(&self) -> &dyn Stream<Item = AngleSample>;
    fn set_smoothing(&self, alpha: f32);
}

pub async fn open_default(hz: f32) -> Result<Box<dyn AngleDevice>> {
    #[cfg(feature = "hidapi")]
    { return Ok(Box::new(backend_hidapi::HidAngle::open(hz).await?)); }
    #[cfg(not(feature = "hidapi"))]
    { let _ = hz; return Err(Error::Backend("no backend enabled".into())); }
}

