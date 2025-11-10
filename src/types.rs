use std::time::Instant;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("backend error: {0}")]
    Backend(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[cfg(feature = "mac_hid_feature")]
    #[error("hid error: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("other: {0}")]
    Other(String),
    #[error("no suitable backend available; tried: {tried:?}")]
    NoBackend { tried: Vec<Source> }, // NEW
}

#[derive(Clone, Copy, Debug)]
pub struct AngleSample {
    pub angle_deg: f32,
    pub timestamp: Instant,
    pub source: Source,
}

#[derive(Clone, Copy, Debug)]
pub enum Source {
    HingeFeature,
    HingeHid,
    HingeIOKit,
    ALS,
    WinHinge,
    WinTilt,
    WinALS,
    LinuxTilt,
    LinuxALS,
    Mock,
}
