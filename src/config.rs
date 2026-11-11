use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub vault: Option<PathBuf>,
    pub recent: Vec<PathBuf>,
    pub tabs: Vec<PathBuf>,
    pub active: usize,
}
impl Session {
    pub fn path() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            })
            .join("nxmd/session.json")
    }
    pub fn load() -> Self {
        std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> anyhow::Result<()> {
        use std::io::Write;
        let p = Self::path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        let mut temp = tempfile::NamedTempFile::new_in(p.parent().unwrap())?;
        temp.write_all(&serde_json::to_vec_pretty(self)?)?;
        temp.as_file().sync_all()?;
        temp.persist(p).map_err(|e| e.error)?;
        Ok(())
    }
}
