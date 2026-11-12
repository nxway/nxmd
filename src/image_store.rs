//! UI-thread cache; decoding is scheduled one job at a time by Workspace.
use gpui::RenderImage;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};
#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct Key {
    pub note: PathBuf,
    pub source: String,
}
#[derive(Clone)]
pub enum State {
    Queued,
    Loading,
    Ready {
        image: Arc<RenderImage>,
        size: (u32, u32),
        bytes: usize,
    },
    Error(String),
}
pub const BUDGET: usize = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
#[derive(Default)]
pub struct ImageStore {
    entries: HashMap<Key, (State, u64)>,
    paths: HashMap<Key, PathBuf>,
    clock: u64,
    pub generation: u64,
    active: PathBuf,
    pub last_error: Option<String>,
}
impl ImageStore {
    pub fn activate(&mut self, note: PathBuf) {
        self.active = note;
    }
    pub fn request(&mut self, key: Key) -> State {
        self.clock += 1;
        if let Some((state, used)) = self.entries.get_mut(&key) {
            *used = self.clock;
            return state.clone();
        }
        if self.entries.len() >= MAX_ENTRIES && !self.evict() {
            return State::Error("This document exceeds the 128-image reference limit".into());
        }
        self.entries.insert(key, (State::Queued, self.clock));
        State::Queued
    }
    pub fn next(&mut self) -> Option<Key> {
        let key = self
            .entries
            .iter()
            .filter(|(k, (s, _))| k.note == self.active && matches!(s, State::Queued))
            .min_by_key(|(_, (_, used))| *used)
            .map(|(k, _)| k.clone())?;
        self.entries.get_mut(&key)?.0 = State::Loading;
        Some(key)
    }
    fn evict(&mut self) -> bool {
        let key = self
            .entries
            .iter()
            .filter(|(k, _)| k.note != self.active)
            .min_by_key(|(_, (_, used))| *used)
            .map(|(k, _)| k.clone());
        if let Some(k) = key {
            self.entries.remove(&k);
            self.paths.remove(&k);
            true
        } else {
            false
        }
    }
    pub fn complete(&mut self, key: Key, result: anyhow::Result<nxmd::images::Decoded>) {
        if !self.entries.contains_key(&key) {
            return;
        } // closed while decoding
        let state = match result {
            Ok(decoded) => {
                let bytes = decoded.rgba.len();
                while self.bytes() + bytes > BUDGET && self.evict() {}
                if self.bytes() + bytes > BUDGET {
                    State::Error("Image preview budget reached (32 MiB); split this note to display more images".into())
                } else {
                    State::Ready {
                        image: Arc::new(RenderImage::new([image::Frame::new(decoded.rgba)])),
                        size: decoded.original,
                        bytes,
                    }
                }
            }
            Err(e) => State::Error(e.to_string()),
        };
        if let State::Error(e) = &state {
            self.last_error = Some(e.clone());
        }
        self.entries.insert(key, (state, self.clock));
    }
    pub fn known_paths(&self) -> HashSet<PathBuf> {
        self.paths
            .iter()
            .filter(|(k, _)| matches!(self.entries.get(*k), Some((State::Ready { .. }, _))))
            .map(|(_, p)| p.clone())
            .collect()
    }
    pub fn complete_loaded(
        &mut self,
        key: Key,
        path: PathBuf,
        decoded: Option<nxmd::images::Decoded>,
    ) {
        if !self.entries.contains_key(&key) {
            return;
        }
        if let Some(decoded) = decoded {
            self.complete(key.clone(), Ok(decoded));
        } else {
            let existing = self
                .paths
                .iter()
                .filter(|(_, p)| *p == &path)
                .find_map(|(k, _)| {
                    self.entries
                        .get(k)
                        .and_then(|(s, _)| matches!(s, State::Ready { .. }).then(|| s.clone()))
                });
            self.entries
                .insert(key.clone(), (existing.unwrap_or(State::Queued), self.clock));
        }
        self.paths.insert(key, path);
    }
    fn bytes(&self) -> usize {
        let mut seen = HashSet::new();
        self.entries
            .values()
            .map(|(s, _)| match s {
                State::Ready { image, bytes, .. } if seen.insert(Arc::as_ptr(image)) => *bytes,
                _ => 0,
            })
            .sum()
    }
    pub fn invalidate(&mut self) {
        self.entries.clear();
        self.paths.clear();
        self.last_error = None;
        self.generation += 1;
    }
    pub fn close(&mut self, note: &std::path::Path) {
        self.entries.retain(|k, _| k.note != note);
        self.paths.retain(|k, _| k.note != note);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(note: &str, n: usize) -> Key {
        Key {
            note: note.into(),
            source: format!("{n}.png"),
        }
    }
    fn image() -> nxmd::images::Decoded {
        nxmd::images::Decoded {
            rgba: image::RgbaImage::new(2048, 2048),
            original: (2048, 2048),
        }
    }
    #[test]
    fn cache_is_bounded_and_evicts_inactive_owners() {
        let mut cache = ImageStore::default();
        cache.activate("note.md".into());
        for n in 0..3 {
            cache.request(key("note.md", n));
            cache.complete(key("note.md", n), Ok(image()));
        }
        assert_eq!(cache.bytes(), BUDGET);
        assert!(matches!(cache.request(key("note.md", 2)), State::Error(_)));
        cache.activate("viewer.png".into());
        cache.request(key("viewer.png", 0));
        cache.complete(key("viewer.png", 0), Ok(image()));
        assert!(cache.bytes() <= BUDGET);
        assert!(matches!(
            cache.request(key("viewer.png", 0)),
            State::Ready { .. }
        ));
        cache.invalidate();
        assert_eq!(cache.bytes(), 0);
        assert_eq!(cache.generation, 1);
    }
    #[test]
    fn canonical_aliases_share_decoded_pixels() {
        let mut cache = ImageStore::default();
        cache.activate("note.md".into());
        let a = key("note.md", 0);
        let b = key("viewer.png", 0);
        cache.request(a.clone());
        cache.complete_loaded(a, "/vault/a.png".into(), Some(image()));
        cache.activate("viewer.png".into());
        cache.request(b.clone());
        cache.complete_loaded(b.clone(), "/vault/a.png".into(), None);
        assert_eq!(cache.bytes(), 16 * 1024 * 1024);
        assert!(matches!(cache.request(b), State::Ready { .. }));
        assert_eq!(cache.known_paths().len(), 1);
    }
    #[test]
    fn closed_inflight_request_does_not_retain_pixels() {
        let mut cache = ImageStore::default();
        cache.activate("a.png".into());
        cache.request(key("a.png", 0));
        assert_eq!(cache.next(), Some(key("a.png", 0)));
        cache.close(std::path::Path::new("a.png"));
        cache.complete(key("a.png", 0), Ok(image()));
        assert_eq!(cache.bytes(), 0);
    }
}
