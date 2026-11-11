use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub vault: Option<PathBuf>,
    pub recent: Vec<PathBuf>,
    pub tabs: Vec<PathBuf>,
    pub active: usize,
    pub sidebar_width: f32,
    pub sidebar_visible: bool,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            vault: None,
            recent: vec![],
            tabs: vec![],
            active: 0,
            sidebar_width: 256.,
            sidebar_visible: true,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_sessions_keep_sidebar_defaults() {
        let s: Session = serde_json::from_str(r#"{"tabs":["n.md"],"active":0}"#).unwrap();
        assert!(s.sidebar_visible);
        assert_eq!(s.sidebar_width, 256.);
        let restored: Session = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(restored.tabs, s.tabs);
    }
}
