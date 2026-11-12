use crate::image_store::{ImageStore, Key as ImageKey, State as ImageState};
use gpui::*;
use nxmd::{
    config::Session,
    vault::{Document, Entry, Vault},
};
use std::{cell::RefCell, rc::Rc};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zorite_editor::{EditorEvent, EditorState};
actions!(
    nxmd,
    [
        Save,
        OpenVault,
        NewNote,
        CloseTab,
        ToggleSidebar,
        DismissMenu,
        RefreshExplorer
    ]
);
enum TabContent {
    Markdown {
        doc: Document,
        editor: Entity<EditorState>,
        _subscription: Subscription,
    },
    Image,
}
struct Tab {
    path: PathBuf,
    content: TabContent,
    dirty: Option<Instant>,
    conflict: bool,
}
impl Tab {
    fn editor(&self) -> Option<&Entity<EditorState>> {
        match &self.content {
            TabContent::Markdown { editor, .. } => Some(editor),
            TabContent::Image => None,
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum NewPath {
    Note,
    Folder,
    Copy,
    Move,
}
pub struct Workspace {
    catalog: Option<nxmd::catalog::Catalog>,
    catalog_revision: u64,
    image_revision: u64,
    images: Rc<RefCell<ImageStore>>,
    image_task: Option<Task<()>>,
    import_busy: bool,
    focus: FocusHandle,
    menu: Option<(PathBuf, bool, Point<Pixels>)>,
    creation_dir: Option<PathBuf>,
    sidebar_width: f32,
    resizing: bool,
    next_external_check: Instant,
    tree_scroll: UniformListScrollHandle,
    dialog_busy: bool,
    sidebar: bool,
    vault: Option<Vault>,
    tabs: Vec<Tab>,
    active: usize,
    expanded: HashSet<PathBuf>,
    entries: Vec<(Entry, usize)>,
    status: String,
    session: Session,
    _timer: Task<()>,
}
impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            weak.update(cx, |this: &mut Self, cx| {
                for i in 0..this.tabs.len() {
                    if this.tabs[i].dirty.is_some() && !this.save_tab(i, cx) {
                        cx.notify();
                        return false;
                    }
                }
                this.persist();
                true
            })
            .unwrap_or(true)
        });
        let session = Session::load();
        let vault = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .or(session.vault.clone())
            .and_then(|p| Vault::open(p).ok());
        let timer = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });
        let paths = session.tabs.clone();
        let active = session.active;
        let catalog = vault.clone().map(nxmd::catalog::Catalog::new);
        let mut app = Self {
            catalog,
            catalog_revision: 0,
            image_revision: 0,
            images: Rc::new(RefCell::new(ImageStore::default())),
            image_task: None,
            import_busy: false,
            focus: cx.focus_handle(),
            menu: None,
            creation_dir: None,
            sidebar_width: session.sidebar_width.clamp(190., 380.),
            resizing: false,
            next_external_check: Instant::now(),
            tree_scroll: UniformListScrollHandle::default(),
            dialog_busy: false,
            sidebar: session.sidebar_visible,
            vault,
            tabs: vec![],
            active: 0,
            expanded: HashSet::new(),
            entries: vec![],
            status: "Local Markdown · Live Preview".into(),
            session,
            _timer: timer,
        };
        app.refresh();
        for path in paths {
            app.open(path, window, cx);
        }
        app.active = active.min(app.tabs.len().saturating_sub(1));
        if app.tabs.is_empty() {
            window.focus(&app.focus, cx);
        }
        app.focus_active(window, cx);
        if app.vault.is_some() {
            app.persist();
        }
        app
    }
    fn persist(&mut self) {
        self.session.vault = self.vault.as_ref().map(|v| v.root.clone());
        self.session.tabs = self.tabs.iter().map(|t| t.path.clone()).collect();
        self.session.active = self.active;
        self.session.sidebar_width = self.sidebar_width;
        self.session.sidebar_visible = self.sidebar;
        if let Err(e) = self.session.save() {
            self.status = format!("Session: {e}");
        }
    }
    fn focus_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(t) = self.tabs.get(self.active) {
            self.images.borrow_mut().activate(t.path.clone());
            if let Some(e) = t.editor() {
                e.update(cx, |e, cx| e.focus(window, cx));
            } else {
                window.focus(&self.focus, cx);
            }
        } else {
            window.focus(&self.focus, cx);
        }
    }
    fn refresh(&mut self) {
        if let Some(catalog) = &self.catalog {
            catalog.refresh();
        }
        self.rebuild_tree();
    }
    fn rebuild_tree(&mut self) {
        self.entries.clear();
        fn visit(
            snapshot: &nxmd::catalog::Snapshot,
            dir: &Path,
            expanded: &HashSet<PathBuf>,
            depth: usize,
            rows: &mut Vec<(Entry, usize)>,
        ) {
            if let Some(entries) = snapshot.directories.get(dir) {
                for e in entries.iter() {
                    rows.push((e.clone(), depth));
                    if e.directory && expanded.contains(&e.path) {
                        visit(snapshot, &e.path, expanded, depth + 1, rows);
                    }
                }
            }
        }
        if let Some(catalog) = &self.catalog {
            visit(
                &catalog.snapshot(),
                Path::new(""),
                &self.expanded,
                0,
                &mut self.entries,
            );
        }
    }
    fn refresh_explorer(&mut self, _: &RefreshExplorer, _: &mut Window, cx: &mut Context<Self>) {
        self.images.borrow_mut().invalidate();
        self.refresh();
        cx.notify();
    }
    fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.tabs.iter().position(|t| t.path == path) {
            self.active = i;
            self.focus_active(window, cx);
            return;
        }
        let Some(v) = &self.vault else {
            return;
        };
        if nxmd::images::supported(&path) {
            if let Err(e) = v.resolve(&path) {
                self.status = e.to_string();
                return;
            }
            self.tabs.push(Tab {
                path,
                content: TabContent::Image,
                dirty: None,
                conflict: false,
            });
            self.active = self.tabs.len() - 1;
            self.focus_active(window, cx);
            self.persist();
            cx.notify();
            return;
        }
        match v.load(&path) {
            Ok(doc) => {
                let editor = cx.new(|cx| {
                    let mut e = EditorState::new(window, cx).with_text(doc.text.clone());
                    e.set_markdown_style(crate::theme::markdown_style(), cx);
                    let images = self.images.clone();
                    let note = path.clone();
                    e.set_block_image_provider(move |src| {
                        match images.borrow_mut().request(ImageKey {
                            note: note.clone(),
                            source: src.into(),
                        }) {
                            ImageState::Ready { image, .. } => Some(image),
                            _ => None,
                        }
                    });
                    let images = self.images.clone();
                    let note = path.clone();
                    e.set_block_chip_provider(move |src| {
                        match images.borrow_mut().request(ImageKey {
                            note: note.clone(),
                            source: src.into(),
                        }) {
                            ImageState::Error(error) => {
                                Some(format!("Image unavailable: {error}").into())
                            }
                            _ => None,
                        }
                    });
                    e
                });
                let subscription = cx.subscribe(&editor, |this, ed, event: &EditorEvent, cx| {
                    if matches!(event, EditorEvent::Changed) {
                        if let Some(tab) = this
                            .tabs
                            .iter_mut()
                            .find(|t| t.editor().is_some_and(|e| *e == ed))
                        {
                            tab.dirty = Some(Instant::now());
                        }
                        cx.notify();
                    }
                });
                editor.update(cx, |e, cx| e.focus(window, cx));
                self.tabs.push(Tab {
                    path,
                    content: TabContent::Markdown {
                        doc,
                        editor,
                        _subscription: subscription,
                    },
                    dirty: None,
                    conflict: false,
                });
                self.active = self.tabs.len() - 1;
                self.focus_active(window, cx);
                self.persist();
                cx.notify();
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    fn save_tab(&mut self, i: usize, cx: &mut Context<Self>) -> bool {
        let Some(v) = &self.vault else {
            return false;
        };
        let t = &mut self.tabs[i];
        let TabContent::Markdown { doc, editor, .. } = &mut t.content else {
            return true;
        };
        doc.text = editor.read(cx).text().to_owned();
        match v.save(doc) {
            Ok(()) => {
                t.dirty = None;
                t.conflict = false;
                self.status = "Saved locally".into();
                true
            }
            Err(e) => {
                t.conflict = true;
                self.status = e.to_string();
                false
            }
        }
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        for image in self.images.borrow_mut().take_retired() {
            cx.drop_image(image, None);
        }
        self.poll_images(cx);
        let check_external = Instant::now() >= self.next_external_check;
        if check_external {
            self.next_external_check = Instant::now() + Duration::from_secs(2);
        }
        for i in 0..self.tabs.len() {
            if check_external
                && !self.tabs[i].conflict
                && self
                    .vault
                    .as_ref()
                    .is_some_and(|v| match &self.tabs[i].content {
                        TabContent::Markdown { doc, .. } => v.changed(doc),
                        _ => false,
                    })
            {
                self.tabs[i].conflict = true;
                self.status =
                    "External change detected. Save a copy to preserve your version.".into();
                cx.notify();
            }
            if self.tabs[i]
                .dirty
                .is_some_and(|t| t.elapsed() >= Duration::from_millis(900))
                && !self.tabs[i].conflict
            {
                self.save_tab(i, cx);
                cx.notify();
            }
        }
    }
    fn paste_image(&mut self, _: &zorite_editor::Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(clip) = cx.read_from_clipboard() {
            for entry in clip.entries() {
                if let ClipboardEntry::Image(image) = entry {
                    if image.bytes().len() > nxmd::images::MAX_ENCODED {
                        self.status = "Clipboard image exceeds 32 MiB".into();
                        cx.notify();
                        return;
                    }
                    self.insert_images(vec![(image.bytes().to_vec(), None)], cx);
                    return;
                }
            }
        }
        cx.propagate();
    }
    fn insert_images(&mut self, inputs: Vec<(Vec<u8>, Option<PathBuf>)>, cx: &mut Context<Self>) {
        if inputs.len() > 16 {
            self.status = "Import at most 16 images at a time".into();
            cx.notify();
            return;
        }
        if self.import_busy {
            self.status = "An image import is already running".into();
            cx.notify();
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let Some(editor) = tab.editor().cloned() else {
            self.status = "Select a Markdown note before inserting an image".into();
            cx.notify();
            return;
        };
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let note = tab.path.clone();
        let original = editor.read(cx).text().to_owned();
        let at = editor.read(cx).cursor();
        self.import_busy = true;
        let task = cx.background_executor().spawn({
            let vault = vault.clone();
            let note = note.clone();
            async move {
                inputs
                    .into_iter()
                    .map(|(bytes, source)| {
                        let bytes = match source {
                            Some(p) => nxmd::images::read(&p)?,
                            None => nxmd::images::clipboard_bytes(&bytes)?,
                        };
                        nxmd::attachments::import(&vault, &note, &bytes)
                    })
                    .collect::<Vec<anyhow::Result<String>>>()
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.import_busy = false;
                if this.vault.as_ref().map(|v| &v.root) != Some(&vault.root) {
                    return;
                }
                let mut references = Vec::new();
                let mut errors = Vec::new();
                for result in result {
                    match result {
                        Ok(reference) => references.push(reference),
                        Err(e) => errors.push(e.to_string()),
                    }
                }
                if !references.is_empty() {
                    let reference = references.join("\n");
                    if let Some(tab) = this
                        .tabs
                        .iter()
                        .find(|t| t.editor() == Some(&editor) && t.path == note)
                    {
                        let editor = tab.editor().unwrap();
                        // Do not insert at a stale byte offset if typing continued during decode.
                        editor.update(cx, |e, cx| {
                            let at = if e.text() == original { at } else { e.cursor() };
                            e.replace_range(at..at, &format!("\n{reference}\n"), cx);
                            // Zorite's host edit API records undo and redraws, but
                            // does not emit the event used by our autosave subscription.
                            cx.emit(EditorEvent::Changed);
                        });
                        log::info!("image import completed: {reference}");
                    } else {
                        this.status = format!(
                            "Attachment imported; note closed before insertion: {reference}"
                        );
                    }
                    this.refresh();
                }
                if !errors.is_empty() {
                    this.status = format!("Image import: {}", errors.join("; "));
                    log::error!("{}", this.status);
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn poll_images(&mut self, cx: &mut Context<Self>) {
        let Some(catalog) = &self.catalog else {
            return;
        };
        let snapshot = catalog.snapshot();
        if snapshot.revision != self.catalog_revision {
            self.catalog_revision = snapshot.revision;
            if snapshot.image_revision != self.image_revision {
                self.image_revision = snapshot.image_revision;
                self.images.borrow_mut().invalidate();
                for tab in &self.tabs {
                    if let Some(editor) = tab.editor() {
                        editor.update(cx, |e, cx| e.refresh_images(cx));
                    }
                }
            }
            if let Some(error) = &snapshot.error {
                self.status = error.clone();
            } else if !snapshot.ready {
                self.status = "Indexing local image filenames…".into();
            } else if self.status == "Indexing local image filenames…" {
                self.status = "Local Markdown · Live Preview".into();
            }
            self.rebuild_tree();
            cx.notify();
        }
        if !snapshot.ready || self.image_task.is_some() {
            return;
        }
        let Some(key) = self.images.borrow_mut().next() else {
            return;
        };
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let generation = self.images.borrow().generation;
        let job_key = key.clone();
        let index = snapshot.images.clone();
        let known = self.images.borrow().known_paths();
        let job = cx.background_executor().spawn(async move {
            let path = if job_key.source.is_empty() {
                vault.resolve(&job_key.note)?
            } else {
                nxmd::attachments::resolve_indexed(&vault, &job_key.note, &job_key.source, &index)?
            };
            let path = dunce::canonicalize(path)?;
            if known.contains(&path) {
                return Ok((path, None));
            }
            let mut decoded = nxmd::images::decode(&nxmd::images::read(&path)?)?;
            for pixel in decoded.rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            Ok((path, Some(decoded)))
        });
        self.image_task = Some(cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                this.image_task = None;
                if this.images.borrow().generation != generation {
                    return;
                }
                if let Ok((_, Some(d))) = &result {
                    log::info!(
                        "image decoded: {} ({} x {})",
                        if key.source.is_empty() {
                            key.note.display().to_string()
                        } else {
                            key.source.clone()
                        },
                        d.original.0,
                        d.original.1
                    );
                }
                if let Err(error) = &result {
                    log::warn!("image unavailable: {}: {error}", key.source);
                }
                match result {
                    Ok((path, decoded)) => {
                        this.images.borrow_mut().complete_loaded(key, path, decoded)
                    }
                    Err(e) => this.images.borrow_mut().complete(key, Err(e)),
                }
                if let Some(error) = this.images.borrow().last_error.clone() {
                    this.status = error;
                }
                if let Some(editor) = this.tabs.get(this.active).and_then(Tab::editor) {
                    editor.update(cx, |e, cx| e.refresh_images(cx));
                }
                cx.notify();
            });
        }));
    }
    fn save(&mut self, _: &Save, _: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.is_empty() {
            self.save_tab(self.active, cx);
            cx.notify();
        }
    }
    fn open_vault(&mut self, _: &OpenVault, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_busy {
            return;
        }
        for i in 0..self.tabs.len() {
            if self.tabs[i].dirty.is_some() && !self.save_tab(i, cx) {
                return;
            }
        }
        self.dialog_busy = true;
        log::info!("dialog open-vault requested");
        let result = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open vault".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = result.await.map_err(anyhow::Error::from).and_then(|r| r);
            let _ = this.update_in(cx, |this, window, cx| {
                this.dialog_busy = false;
                log::info!("dialog open-vault completed");
                match result {
                    Ok(Some(paths)) => {
                        if let Some(p) = paths.first() {
                            for i in 0..this.tabs.len() {
                                if this.tabs[i].dirty.is_some() && !this.save_tab(i, cx) {
                                    return;
                                }
                            }
                            match Vault::open(p) {
                                Ok(v) => {
                                    this.catalog = Some(nxmd::catalog::Catalog::new(v.clone()));
                                    this.catalog_revision = 0;
                                    this.image_revision = 0;
                                    this.images.borrow_mut().invalidate();
                                    this.vault = Some(v);
                                    this.tabs.clear();
                                    window.focus(&this.focus, cx);
                                    this.active = 0;
                                    this.expanded.clear();
                                    this.session.recent.retain(|r| r != p);
                                    this.session.recent.insert(0, p.clone());
                                    this.session.recent.truncate(10);
                                    this.refresh();
                                    this.persist();
                                }
                                Err(e) => this.status = e.to_string(),
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::error!("open-vault dialog: {e}");
                        this.status = e.to_string();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        self.close_index(self.active, window, cx);
    }
    fn close_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_busy || index >= self.tabs.len() {
            return;
        }
        if self.tabs[index].dirty.is_some() && !self.save_tab(index, cx) {
            cx.notify();
            return;
        }
        self.active = nxmd::tabs::active_after_close(self.active, index, self.tabs.len());
        let closed = self.tabs.remove(index);
        self.images.borrow_mut().close(&closed.path);
        self.focus_active(window, cx);
        self.menu = None;
        self.persist();
        cx.notify();
    }
    fn dismiss_menu(&mut self, _: &DismissMenu, _: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        cx.notify();
    }
    fn context_action(
        &mut self,
        action: usize,
        path: PathBuf,
        directory: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        if directory {
            self.creation_dir = Some(path);
            if action == 0 {
                self.new_note(&NewNote, window, cx);
            } else {
                self.new_folder(window, cx);
            }
        } else {
            self.open(path.clone(), window, cx);
            if nxmd::images::supported(&path) {
                self.persist();
                cx.notify();
                return;
            }
            self.persist();
            if self.tabs.get(self.active).is_some_and(|t| t.path == path) {
                match action {
                    0 => self.move_note(window, cx),
                    1 => self.save_copy(window, cx),
                    _ => self.trash_note(window, cx),
                }
            }
        }
        cx.notify();
    }
    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.sidebar = !self.sidebar;
        self.persist();
        cx.notify();
    }
    fn new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_path(NewPath::Folder, window, cx);
    }
    fn save_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_path(NewPath::Copy, window, cx);
    }
    fn move_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_path(NewPath::Move, window, cx);
    }
    fn new_note(&mut self, _: &NewNote, window: &mut Window, cx: &mut Context<Self>) {
        self.new_path(NewPath::Note, window, cx);
    }
    fn new_path(&mut self, kind: NewPath, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_busy {
            return;
        }
        if matches!(kind, NewPath::Move)
            && (self.tabs.is_empty() || !self.save_tab(self.active, cx))
        {
            return;
        }
        let Some(vault) = self.vault.clone() else {
            self.status = "Open a vault first (Ctrl+O)".into();
            cx.notify();
            return;
        };
        let tab = self.tabs.get(self.active);
        if matches!(kind, NewPath::Move | NewPath::Copy) && tab.and_then(Tab::editor).is_none() {
            return;
        }
        let old = tab.map(|t| t.path.clone());
        let text = tab
            .and_then(Tab::editor)
            .map(|e| e.read(cx).text().to_owned())
            .unwrap_or_default();
        let suggested = match kind {
            NewPath::Folder => "New folder".into(),
            NewPath::Note => "Untitled.md".into(),
            NewPath::Copy => "Recovered.md".into(),
            NewPath::Move => old
                .as_ref()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        };
        self.dialog_busy = true;
        log::info!("dialog {kind:?} requested");
        let directory = self
            .creation_dir
            .take()
            .and_then(|p| vault.resolve(&p).ok())
            .unwrap_or_else(|| vault.root.clone());
        let result = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let result = result.await.map_err(anyhow::Error::from).and_then(|r| r);
            let _ = this.update_in(cx, |this, window, cx| {
                this.dialog_busy = false;
                log::info!("dialog {kind:?} completed");
                let result = (|| -> anyhow::Result<()> {
                    let Some(path) = result? else {
                        return Ok(());
                    };
                    if this.vault.as_ref().map(|v| &v.root) != Some(&vault.root) {
                        anyhow::bail!("Vault changed while dialog was open");
                    }
                    let rel = path.strip_prefix(&vault.root)?.to_path_buf();
                    match kind {
                        NewPath::Folder => vault.create(&rel, true)?,
                        NewPath::Note => {
                            vault.create(&rel, false)?;
                            this.open(rel, window, cx);
                        }
                        NewPath::Copy => {
                            vault.create(&rel, false)?;
                            let mut doc = vault.load(&rel)?;
                            doc.text = text;
                            vault.save(&mut doc)?;
                            this.open(rel, window, cx);
                        }
                        NewPath::Move => {
                            let old = old.unwrap();
                            let i = this
                                .tabs
                                .iter()
                                .position(|t| t.path == old)
                                .ok_or_else(|| anyhow::anyhow!("Source tab closed"))?;
                            if !this.save_tab(i, cx) {
                                return Ok(());
                            }
                            vault.move_note(&old, &rel)?;
                            this.tabs.remove(i);
                            this.active = 0;
                            this.open(rel, window, cx);
                        }
                    }
                    this.refresh();
                    this.persist();
                    Ok(())
                })();
                if let Err(e) = result {
                    log::error!("dialog operation failed: {e}");
                    this.status = e.to_string();
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn trash_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_busy || self.tabs.is_empty() || !self.save_tab(self.active, cx) {
            return;
        }
        let path = self.tabs[self.active].path.clone();
        let vault = self.vault.clone().unwrap();
        self.dialog_busy = true;
        log::info!("dialog trash requested");
        let result = window.prompt(
            PromptLevel::Warning,
            "Move note to recovery folder?",
            Some("The file will be retained in .nxmd-trash inside your vault."),
            &[
                PromptButton::ok("Move to trash"),
                PromptButton::cancel("Cancel"),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let result = result.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.dialog_busy = false;
                log::info!("dialog trash completed");
                if matches!(result, Ok(0))
                    && this.vault.as_ref().map(|v| &v.root) == Some(&vault.root)
                {
                    if let Some(i) = this.tabs.iter().position(|t| t.path == path) {
                        if !this.save_tab(i, cx) {
                            return;
                        }
                        match vault.trash(&path) {
                            Ok(_) => {
                                this.tabs.remove(i);
                                this.active = this.active.min(this.tabs.len().saturating_sub(1));
                                this.refresh();
                                this.persist();
                            }
                            Err(e) => this.status = e.to_string(),
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
impl Workspace {
    fn tree_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        use crate::design::*;
        let (entry, depth) = &self.entries[index];
        let path = entry.path.clone();
        let directory = entry.directory;
        let expanded = self.expanded.contains(&path);
        let active = self.tabs.get(self.active).is_some_and(|t| t.path == path);
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let tooltip: SharedString = path.to_string_lossy().to_string().into();
        let right = path.clone();
        div()
            .id(SharedString::from(format!("tree:{}", path.display())))
            .h(px(30.))
            .w(px(self.sidebar_width - 16.))
            .px(px(8.))
            .mx(px(8.))
            .rounded(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(8. + *depth as f32 * 14.))
            .cursor_pointer()
            .bg(rgb(if active { SURFACE } else { SIDEBAR }))
            .text_color(rgb(if active { ACCENT } else { TEXT }))
            .hover(|s| s.bg(rgb(HOVER)))
            .tooltip(move |_, cx| tip(tooltip.clone(), cx))
            .child(if directory {
                icon(if expanded { "down" } else { "chevron" })
                    .size(px(12.))
                    .into_any_element()
            } else {
                div().w(px(12.)).into_any_element()
            })
            .child(
                icon(if directory {
                    "folder"
                } else if nxmd::images::supported(&path) {
                    "image"
                } else {
                    "note"
                })
                .text_color(rgb(if directory {
                    MUTED
                } else if active {
                    ACCENT
                } else {
                    MUTED
                })),
            )
            .child(div().flex_1().min_w_0().text_ellipsis().child(name))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |s, e: &MouseDownEvent, w, cx| {
                    let size = w.viewport_size();
                    s.menu = Some((
                        right.clone(),
                        directory,
                        point(
                            e.position.x.min((size.width - px(240.)).max(px(0.))),
                            e.position.y.min((size.height - px(180.)).max(px(0.))),
                        ),
                    ));
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |s, _, w, cx| {
                s.menu = None;
                if directory {
                    if !s.expanded.remove(&path) {
                        s.expanded.insert(path.clone());
                    }
                    let began = Instant::now();
                    s.rebuild_tree();
                    log::info!(
                        "explorer toggle: {} rows={} elapsed_us={}",
                        path.display(),
                        s.entries.len(),
                        began.elapsed().as_micros()
                    );
                } else {
                    s.open(path.clone(), w, cx);
                    s.persist();
                }
                cx.notify();
            }))
            .into_any_element()
    }
    fn menu_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::design::*;
        let (path, directory, position) = self.menu.clone()?;
        let names: Vec<&'static str> = if nxmd::images::supported(&path) && !directory {
            vec!["Open image"]
        } else if directory {
            vec!["New note here", "New folder here"]
        } else {
            vec!["Rename / move…", "Save as copy…", "Move to trash…"]
        };
        let items = names
            .into_iter()
            .enumerate()
            .map(|(i, label)| {
                let target = path.clone();
                div()
                    .id(("menu-action", i))
                    .h(px(32.))
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_color(rgb(if !directory && i == 2 { DANGER } else { TEXT }))
                    .hover(|s| s.bg(rgb(HOVER)))
                    .child(label)
                    .on_click(cx.listener(move |s, _, w, cx| {
                        cx.stop_propagation();
                        s.context_action(i, target.clone(), directory, w, cx);
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        Some(
            div()
                .id("context-menu")
                .absolute()
                .left(position.x)
                .top(position.y)
                .w(px(228.))
                .p(px(5.))
                .rounded(px(7.))
                .bg(rgb(SURFACE))
                .border_1()
                .border_color(rgb(BORDER))
                .shadow_md()
                .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                    s.menu = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .text_ellipsis()
                        .child(
                            path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                        ),
                )
                .children(items)
                .into_any_element(),
        )
    }
}
impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::design::*;
        let vault_name = self
            .vault
            .as_ref()
            .and_then(|v| v.root.file_name())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Your workspace".into());
        let active = self.tabs.get(self.active);
        let active_path = active
            .map(|t| t.path.to_string_lossy().to_string())
            .unwrap_or_else(|| "No note selected".into());
        let save_label = active
            .map(|t| {
                if matches!(t.content, TabContent::Image) {
                    "Read-only image"
                } else if t.conflict {
                    "External change"
                } else if t.dirty.is_some() {
                    "Saving…"
                } else {
                    "All changes saved"
                }
            })
            .unwrap_or("Local files · No account");
        let status_color = if active.is_some_and(|t| t.conflict) {
            DANGER
        } else {
            MUTED
        };
        let editor = active.and_then(Tab::editor).cloned();
        let image_path = active
            .filter(|t| matches!(t.content, TabContent::Image))
            .map(|t| t.path.clone());
        let tabs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let selected = i == self.active;
                let title = t
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let tooltip: SharedString = t.path.to_string_lossy().to_string().into();
                div()
                    .id(("tab", i))
                    .h(px(44.))
                    .w(px(184.))
                    .flex_shrink_0()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .border_b_2()
                    .border_color(rgb(if selected { ACCENT } else { SIDEBAR }))
                    .bg(rgb(if selected { BG } else { SIDEBAR }))
                    .text_color(rgb(if selected { TEXT } else { MUTED }))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(SURFACE)))
                    .tooltip(move |_, cx| tip(tooltip.clone(), cx))
                    .child(
                        icon(if matches!(t.content, TabContent::Image) {
                            "image"
                        } else {
                            "note"
                        })
                        .size(px(14.))
                        .text_color(rgb(if selected {
                            ACCENT
                        } else {
                            MUTED
                        })),
                    )
                    .child(div().flex_1().min_w_0().text_ellipsis().child(title))
                    .child(
                        div()
                            .text_color(rgb(if t.conflict { DANGER } else { ACCENT }))
                            .child(if t.conflict {
                                "!"
                            } else if t.dirty.is_some() {
                                "•"
                            } else {
                                ""
                            }),
                    )
                    .child(
                        button(("tab-close", i), "close", "Close tab · Ctrl+W")
                            .size(px(22.))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |s, _, w, cx| {
                                    cx.stop_propagation();
                                    s.close_index(i, w, cx);
                                }),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |s, _, w, cx| {
                            cx.stop_propagation();
                            s.close_index(i, w, cx);
                        }),
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.active = i;
                        s.menu = None;
                        s.focus_active(w, cx);
                        s.persist();
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let menu = self.menu_view(cx);
        let tree = uniform_list(
            "file-tree",
            self.entries.len(),
            cx.processor(|s, range: std::ops::Range<usize>, _, cx| {
                range.map(|i| s.tree_row(i, cx)).collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.tree_scroll)
        .flex_1()
        .min_h_0();
        let sidebar = div()
            .w(px(self.sidebar_width))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SIDEBAR))
            .child(
                div()
                    .h(px(64.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .size(px(30.))
                            .rounded(px(7.))
                            .bg(rgb(SURFACE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("book").text_color(rgb(ACCENT))),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_ellipsis()
                                    .child(vault_name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(MUTED))
                                    .child("LOCAL WORKSPACE"),
                            ),
                    )
                    .child(
                        button("open-vault", "down", "Open another vault · Ctrl+O")
                            .on_click(cx.listener(|s, _, w, cx| s.open_vault(&OpenVault, w, cx))),
                    ),
            )
            .child(
                div()
                    .h(px(40.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(MUTED))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("EXPLORER"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(2.))
                            .child(button("new-note", "plus", "New note · Ctrl+N").on_click(
                                cx.listener(|s, _, w, cx| {
                                    s.creation_dir = None;
                                    s.new_note(&NewNote, w, cx);
                                }),
                            ))
                            .child(button("new-folder", "folder-plus", "New folder").on_click(
                                cx.listener(|s, _, w, cx| {
                                    s.creation_dir = None;
                                    s.new_folder(w, cx);
                                }),
                            )),
                    ),
            )
            .child(tree)
            .child(
                div()
                    .h(px(38.))
                    .px(px(20.))
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child("nxmd  /  a space for your words"),
            );
        let is_image = image_path.is_some();
        let main_content = if let Some(path) = image_path {
            let state = self.images.borrow_mut().request(ImageKey {
                note: path.clone(),
                source: String::new(),
            });
            let content = match state {
                ImageState::Ready { image, size, .. } => {
                    let viewport = window.viewport_size();
                    let available = (
                        f32::from(viewport.width)
                            - if self.sidebar {
                                self.sidebar_width + 67.
                            } else {
                                64.
                            },
                        f32::from(viewport.height) - 174.,
                    );
                    let (w, h) = nxmd::images::fit(size, available);
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .child(img(image).w(px(w)).h(px(h)).object_fit(ObjectFit::Contain))
                        .child(
                            div()
                                .text_color(rgb(MUTED))
                                .child(format!("{} × {} · Read only", size.0, size.1)),
                        )
                        .into_any_element()
                }
                ImageState::Error(error) => div()
                    .text_color(rgb(DANGER))
                    .child(error)
                    .into_any_element(),
                _ => div()
                    .text_color(rgb(MUTED))
                    .child("Loading image…")
                    .into_any_element(),
            };
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .child(content)
                .into_any_element()
        } else if let Some(editor) = editor {
            div()
                .id("document")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(40.))
                .py(px(40.))
                .child(
                    div()
                        .max_w(px(760.))
                        .mx_auto()
                        .w_full()
                        .font_family("Segoe UI")
                        .text_size(px(18.))
                        .line_height(relative(1.65))
                        .text_color(rgb(TEXT))
                        .child(editor),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(14.))
                .child(icon("book").size(px(40.)).text_color(rgb(ACCENT)))
                .child(
                    div()
                        .text_size(px(28.))
                        .font_weight(FontWeight::LIGHT)
                        .child("Room to think."),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(rgb(MUTED))
                        .child("Your notes, on your disk. Pick a note or start something new."),
                )
                .child(
                    div()
                        .id("welcome-open")
                        .mt(px(10.))
                        .px(px(18.))
                        .py(px(9.))
                        .rounded(px(6.))
                        .bg(rgb(SURFACE))
                        .text_color(rgb(ACCENT))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(HOVER)))
                        .child(if self.vault.is_some() {
                            "Create a note   Ctrl+N"
                        } else {
                            "Open a vault   Ctrl+O"
                        })
                        .on_click(cx.listener(|s, _, w, cx| {
                            if s.vault.is_some() {
                                s.new_note(&NewNote, w, cx);
                            } else {
                                s.open_vault(&OpenVault, w, cx);
                            }
                        })),
                )
                .into_any_element()
        };
        div()
            .relative()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family("Segoe UI")
            .text_size(px(13.))
            .on_action(cx.listener(Self::refresh_explorer))
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::dismiss_menu))
            .on_action(cx.listener(Self::paste_image))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::open_vault))
            .on_action(cx.listener(Self::new_note))
            .on_drop(cx.listener(|s, paths: &ExternalPaths, _, cx| {
                s.insert_images(
                    paths
                        .paths()
                        .iter()
                        .map(|p| (Vec::new(), Some(p.clone())))
                        .collect(),
                    cx,
                );
            }))
            .on_mouse_move(cx.listener(|s, e: &MouseMoveEvent, _, cx| {
                if s.resizing && e.pressed_button != Some(MouseButton::Left) {
                    s.resizing = false;
                    s.persist();
                }
                if s.resizing {
                    s.sidebar_width = f32::from(e.position.x).clamp(190., 380.);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    if s.resizing {
                        s.resizing = false;
                        s.persist();
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .children(self.sidebar.then_some(sidebar))
                    .children(self.sidebar.then(|| {
                        div()
                            .id("sidebar-resize")
                            .w(px(3.))
                            .h_full()
                            .bg(rgb(BORDER))
                            .cursor(CursorStyle::ResizeLeftRight)
                            .hover(|s| s.bg(rgb(ACCENT)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|s, _, _, cx| {
                                    s.resizing = true;
                                    cx.stop_propagation();
                                }),
                            )
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(44.))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .bg(rgb(SIDEBAR))
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        div().px(px(8.)).child(
                                            button(
                                                "toggle-sidebar",
                                                "panel",
                                                "Toggle explorer · Ctrl+Shift+L",
                                            )
                                            .on_click(
                                                cx.listener(|s, _, w, cx| {
                                                    s.toggle_sidebar(&ToggleSidebar, w, cx)
                                                }),
                                            ),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .id("tabs")
                                            .flex_1()
                                            .min_w_0()
                                            .h_full()
                                            .overflow_x_scroll()
                                            .flex()
                                            .children(tabs),
                                    ),
                            )
                            .child(
                                div()
                                    .h(px(38.))
                                    .flex_shrink_0()
                                    .px(px(32.))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_size(px(11.))
                                    .text_color(rgb(MUTED))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_ellipsis()
                                            .child(active_path.clone()),
                                    )
                                    .child(div().text_color(rgb(ACCENT)).child(if is_image {
                                        "IMAGE"
                                    } else {
                                        "LIVE PREVIEW"
                                    })),
                            )
                            .child(main_content),
                    ),
            )
            .child(
                div()
                    .h(px(28.))
                    .flex_shrink_0()
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(SIDEBAR))
                    .text_size(px(11.))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .min_w_0()
                            .child(div().text_color(rgb(ACCENT)).child("•"))
                            .child(div().text_color(rgb(status_color)).child(save_label)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .mx(px(20.))
                            .text_ellipsis()
                            .text_color(rgb(MUTED))
                            .child(
                                if self.status == "Local Markdown · Live Preview"
                                    || self.status == "Saved locally"
                                {
                                    String::new()
                                } else {
                                    self.status.clone()
                                },
                            ),
                    )
                    .child(div().text_color(rgb(MUTED)).child(if is_image {
                        "Image  ·  Read only"
                    } else {
                        "Markdown  ·  UTF-8"
                    })),
            )
            .children(menu)
    }
}
