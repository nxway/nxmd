//! A locked slot belongs to one live document. Startup may claim abandoned slots.
//! Slot names are generated locally; record paths are descriptive, never write authority.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

#[derive(Serialize, Deserialize, Clone)]
pub struct Draft {
    pub version: u8,
    pub vault: PathBuf,
    pub note: PathBuf,
    pub generation: u64,
    pub baseline: String,
    pub text: String,
    pub updated: SystemTime,
}
#[derive(Serialize, Deserialize)]
struct Record<T> {
    draft: T,
    text_sha256: String,
}

pub fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone)]
pub struct Slot(Arc<Inner>);
struct Inner {
    dir: PathBuf,
    lease: Option<File>,
    state: Mutex<State>,
}
struct State {
    durable: u64,
    retired: u64,
}
impl Drop for Inner {
    fn drop(&mut self) {
        // Delete only an empty, owned slot. Never sweep recovery contents.
        if matches!(self.dir.join("draft.json").try_exists(), Ok(false)) {
            // A separate lease is held through removal. Windows handles opened
            // by Rust allow delete sharing.
            let _ = fs::remove_file(self.dir.join("lease"));
            drop(self.lease.take());
            let _ = fs::remove_dir(&self.dir);
        }
    }
}
impl Slot {
    pub fn new(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let dir = tempfile::Builder::new().prefix("doc-").tempdir_in(root)?;
        let lease = File::create_new(dir.path().join("lease"))?;
        lease.lock()?;
        Ok(Self(Arc::new(Inner {
            dir: dir.keep(),
            lease: Some(lease),
            state: Mutex::new(State {
                durable: 0,
                retired: 0,
            }),
        })))
    }
    pub fn checkpoint(&self, d: &Draft) -> Result<()> {
        let mut s = self
            .0
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Draft writer stopped"))?;
        if d.generation <= s.durable.max(s.retired) {
            return Ok(());
        }
        let mut temp = tempfile::NamedTempFile::new_in(&self.0.dir)?;
        {
            let mut w = BufWriter::new(temp.as_file_mut());
            serde_json::to_writer(
                &mut w,
                &Record {
                    draft: d,
                    text_sha256: fingerprint(d.text.as_bytes()),
                },
            )?;
            w.flush()?;
        }
        temp.as_file().sync_all()?;
        temp.persist(self.0.dir.join("draft.json"))?;
        s.durable = d.generation;
        Ok(())
    }
    pub fn retire(&self, generation: u64) -> Result<()> {
        let mut s = self
            .0
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Draft writer stopped"))?;
        if s.durable > generation {
            bail!("A newer recovery generation exists")
        }
        let path = self.0.dir.join("draft.json");
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        s.retired = s.retired.max(generation);
        Ok(())
    }
}
pub struct Candidate {
    pub slot: Slot,
    pub draft: Draft,
}
pub fn discover(root: &Path) -> Result<(Vec<Candidate>, Vec<String>)> {
    let mut out = vec![];
    let mut errors = vec![];
    let entries = match fs::read_dir(root) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((out, errors)),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        if !entry.file_name().to_string_lossy().starts_with("doc-") {
            continue;
        }
        let dir = entry.path();
        let record = dir.join("draft.json");
        let load = || -> Result<Option<Candidate>> {
            if !entry.file_type()?.is_dir() {
                return Ok(None);
            }
            let lease = File::options()
                .read(true)
                .write(true)
                .open(dir.join("lease"))?;
            match lease.try_lock() {
                Ok(()) => {}
                Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
                Err(e) => return Err(e.into()),
            }
            if !record.try_exists()? {
                // Empty abandoned leases have no user content. Leave unfamiliar
                // staging files in place and surface their location.
                if fs::read_dir(&dir)?.any(|e| e.map_or(true, |e| e.file_name() != "lease")) {
                    bail!("Incomplete recovery checkpoint retained");
                }
                return Ok(None);
            }
            let record: Record<Draft> =
                serde_json::from_reader(BufReader::new(File::open(&record)?))
                    .context("Invalid draft JSON")?;
            let draft = record.draft;
            if fingerprint(draft.text.as_bytes()) != record.text_sha256 {
                bail!("Recovery text checksum mismatch");
            }
            if draft.version != 1
                || draft.generation == 0
                || draft.generation == u64::MAX
                || !draft.vault.is_absolute()
                || draft.note.as_os_str().is_empty()
                || !draft
                    .note
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
            {
                bail!("Unsupported recovery record")
            }
            let durable = draft.generation;
            Ok(Some(Candidate {
                draft,
                slot: Slot(Arc::new(Inner {
                    dir: dir.clone(),
                    lease: Some(lease),
                    state: Mutex::new(State {
                        durable,
                        retired: 0,
                    }),
                })),
            }))
        };
        match load() {
            Ok(Some(c)) => out.push(c),
            Ok(None) => {}
            Err(e) => errors.push(format!("{}: {e:#}", record.display())),
        }
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.draft.updated));
    Ok((out, errors))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_crash_restore_and_stale_completion() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        let a = Slot::new(root)?;
        let mut d = Draft {
            version: 1,
            vault: root.into(),
            note: "A.md".into(),
            generation: 1,
            baseline: fingerprint(b"base"),
            text: "\u{feff}editor\r\n".into(),
            updated: SystemTime::now(),
        };
        a.checkpoint(&d)?;
        assert!(discover(root)?.0.is_empty());
        drop(a);
        for _ in 0..2 {
            let items = discover(root)?.0;
            assert_eq!(items[0].draft.text, d.text);
        }
        let c = discover(root)?.0.pop().unwrap();
        d.generation = 2;
        d.text.push('2');
        c.slot.checkpoint(&d)?;
        assert!(c.slot.retire(1).is_err());
        drop(c);
        let c = discover(root)?.0.pop().unwrap();
        assert_eq!(c.draft.text, d.text);
        c.slot.retire(2)?;
        c.slot.checkpoint(&d)?;
        drop(c);
        assert!(discover(root)?.0.is_empty());
        Ok(())
    }
}

#[cfg(test)]
#[test]
fn corrupt_record_does_not_hide_a_valid_draft() -> Result<()> {
    let root = tempfile::tempdir()?;
    let bad = root.path().join("doc-incomplete");
    fs::create_dir(&bad)?;
    File::create(bad.join("lease"))?;
    fs::write(bad.join("draft.json"), b"{")?;
    let slot = Slot::new(root.path())?;
    let d = Draft {
        version: 1,
        vault: root.path().into(),
        note: "B.md".into(),
        generation: 1,
        baseline: fingerprint(b""),
        text: "valid".into(),
        updated: SystemTime::now(),
    };
    slot.checkpoint(&d)?;
    drop(slot);
    let (available, warnings) = discover(root.path())?;
    assert_eq!(available[0].draft.text, "valid");
    assert_eq!(warnings.len(), 1);
    assert_eq!(fs::read(bad.join("draft.json"))?, b"{");
    Ok(())
}
