use crate::{Error, Result, Source};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedState {
    pub last_source: Option<Source>,
}

fn state_path() -> Option<PathBuf> {
    // com/booklid/booklid-rust
    let proj = ProjectDirs::from("com", "booklid", "booklid-rust")?;
    let dir = proj.state_dir()?.to_path_buf();
    Some(dir.join("state.json"))
}

pub fn load() -> PersistedState {
    let Some(p) = state_path() else {
        return PersistedState::default();
    };
    let Ok(s) = fs::read_to_string(p) else {
        return PersistedState::default();
    };
    serde_json::from_str(&s).unwrap_or_default()
}

pub fn store(st: &PersistedState) -> Result<()> {
    let Some(p) = state_path() else {
        return Ok(());
    };
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let s = serde_json::to_string_pretty(st).map_err(|e| Error::Other(e.to_string()))?;
    fs::write(p, s)?;
    Ok(())
}

pub fn clear() -> Result<()> {
    let Some(p) = state_path() else {
        return Ok(());
    };
    if p.exists() {
        fs::remove_file(p)?;
    }
    Ok(())
}
