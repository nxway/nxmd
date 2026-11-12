//! One vault-local watcher, debounced incremental directory snapshots and image index.
//! Filesystem work is isolated from GPUI; snapshots contain names, never image pixels.
use crate::{
    attachments::ImageIndex,
    images,
    vault::{Entry, Vault},
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Default)]
pub struct Snapshot {
    pub directories: BTreeMap<PathBuf, Arc<Vec<Entry>>>,
    pub images: Arc<ImageIndex>,
    pub revision: u64,
    pub image_revision: u64,
    pub ready: bool,
    pub error: Option<String>,
}
enum Message {
    Event(notify::Result<notify::Event>),
    Refresh,
    Stop,
}
pub struct Catalog {
    snapshot: Arc<Mutex<Arc<Snapshot>>>,
    tx: SyncSender<Message>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Catalog {
    pub fn new(vault: Vault) -> Self {
        let snapshot = Arc::new(Mutex::new(Arc::new(Snapshot::default())));
        let output = snapshot.clone();
        let (tx, rx) = mpsc::sync_channel(512);
        let events = tx.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let overflow = Arc::new(AtomicBool::new(false));
        let lost = overflow.clone();
        let worker = thread::spawn(move || {
            let mut state = Snapshot::default();
            let mut watcher = notify::recommended_watcher(move |event| {
                if events.try_send(Message::Event(event)).is_err() {
                    lost.store(true, Ordering::Relaxed);
                }
            })
            .map_err(|e| {
                state.error = Some(format!("File watcher: {e}; using periodic recovery scans"))
            })
            .ok();
            scan(
                &vault,
                Path::new(""),
                &mut state,
                &mut watcher,
                &cancelled,
                Some(&output),
            );
            publish(&mut state, &output, true);
            let mut pending = BTreeSet::new();
            let mut image_change = false;
            let mut last_event = Instant::now();
            let mut fallback = Instant::now();
            while !cancelled.load(Ordering::Relaxed) {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Message::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Ok(Message::Refresh) => {
                        pending.extend(state.directories.keys().cloned());
                        last_event = Instant::now();
                    }
                    Ok(Message::Event(Ok(event))) => {
                        if matches!(event.kind, notify::EventKind::Access(_)) {
                            continue;
                        }
                        for path in event.paths {
                            let Ok(path) = path.strip_prefix(&vault.root) else {
                                continue;
                            };
                            if path
                                .components()
                                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
                            {
                                continue;
                            }
                            image_change |=
                                images::supported(path) || state.directories.contains_key(path);
                            pending.insert(path.parent().unwrap_or(Path::new("")).to_path_buf());
                            if state.directories.contains_key(path) {
                                pending.insert(path.to_path_buf());
                            }
                        }
                        last_event = Instant::now();
                    }
                    Ok(Message::Event(Err(e))) => {
                        state.error = Some(format!("File watcher: {e}; refresh recovery enabled"));
                        pending.extend(state.directories.keys().cloned());
                        image_change = true;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if overflow.swap(false, Ordering::Relaxed)
                    || (state.error.is_some() && fallback.elapsed() > Duration::from_secs(5))
                {
                    pending.extend(state.directories.keys().cloned());
                    image_change = true;
                    fallback = Instant::now();
                }
                if !pending.is_empty() && last_event.elapsed() >= Duration::from_millis(150) {
                    let before = state.directories.clone();
                    for dir in std::mem::take(&mut pending) {
                        scan(&vault, &dir, &mut state, &mut watcher, &cancelled, None);
                    }
                    let changed = state.directories != before;
                    if changed || image_change || state.error.is_some() {
                        publish(&mut state, &output, image_change || changed);
                    }
                    image_change = false;
                }
            }
            // Dropping RecommendedWatcher closes its OS handles and internal thread.
        });
        Self {
            snapshot,
            tx,
            stop,
            worker: Some(worker),
        }
    }
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.lock().unwrap().clone()
    }
    pub fn refresh(&self) {
        let _ = self.tx.try_send(Message::Refresh);
    }
}
impl Drop for Catalog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.tx.try_send(Message::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn scan(
    v: &Vault,
    dir: &Path,
    state: &mut Snapshot,
    watcher: &mut Option<RecommendedWatcher>,
    stop: &AtomicBool,
    initial_output: Option<&Mutex<Arc<Snapshot>>>,
) {
    if stop.load(Ordering::Relaxed) {
        return;
    }
    let Ok(entries) = v.entries(dir) else {
        if dir.as_os_str().is_empty() {
            state.error = Some("Vault is unavailable; press F5 to retry".into());
        }
        remove(v, dir, state, watcher);
        return;
    };
    let new = !state.directories.contains_key(dir);
    if new
        && let Some(w) = watcher
        && let Err(e) = w.watch(&v.root.join(dir), RecursiveMode::NonRecursive)
    {
        state.error = Some(format!("Cannot watch {}: {e}", dir.display()));
    }
    let children: BTreeSet<_> = entries
        .iter()
        .filter(|e| e.directory)
        .map(|e| e.path.clone())
        .collect();
    let stale: Vec<_> = state
        .directories
        .keys()
        .filter(|p| p.parent() == Some(dir) && !children.contains(*p))
        .cloned()
        .collect();
    for path in stale {
        remove(v, &path, state, watcher);
    }
    state
        .directories
        .insert(dir.to_path_buf(), Arc::new(entries));
    if let Some(out) = initial_output {
        // Root rows become available before the background basename index finishes.
        state.revision += 1;
        *out.lock().unwrap() = Arc::new(state.clone());
    }
    for child in children {
        if !state.directories.contains_key(&child) {
            scan(v, &child, state, watcher, stop, None);
        }
    }
}
fn remove(v: &Vault, dir: &Path, state: &mut Snapshot, watcher: &mut Option<RecommendedWatcher>) {
    let removed: Vec<_> = state
        .directories
        .keys()
        .filter(|p| p.starts_with(dir))
        .cloned()
        .collect();
    for path in removed {
        state.directories.remove(&path);
        if let Some(w) = watcher {
            let _ = w.unwatch(&v.root.join(path));
        }
    }
}
fn publish(state: &mut Snapshot, out: &Mutex<Arc<Snapshot>>, images_changed: bool) {
    if images_changed {
        let mut index = ImageIndex::new();
        for entries in state.directories.values() {
            for e in entries
                .iter()
                .filter(|e| !e.directory && images::supported(&e.path))
            {
                index
                    .entry(e.path.file_name().unwrap().to_string_lossy().to_lowercase())
                    .or_default()
                    .push(e.path.clone());
            }
        }
        state.images = Arc::new(index);
        state.image_revision += 1;
    }
    state.ready = true;
    state.revision += 1;
    *out.lock().unwrap() = Arc::new(state.clone());
}
