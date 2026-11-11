use gpui::*;
use nxmd::{
    config::Session,
    vault::{Document, Entry, Vault},
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zorite_editor::{EditorEvent, EditorState};
actions!(nxmd, [Save, OpenVault, NewNote, CloseTab, ToggleSidebar]);
struct Tab {
    doc: Document,
    editor: Entity<EditorState>,
    dirty: Option<Instant>,
    conflict: bool,
    _subscription: Subscription,
}
#[derive(Clone, Copy, Debug)]
enum NewPath {
    Note,
    Folder,
    Copy,
    Move,
}
pub struct Workspace {
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
                    .timer(Duration::from_millis(500))
                    .await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });
        let paths = session.tabs.clone();
        let active = session.active;
        let mut app = Self {
            dialog_busy: false,
            sidebar: true,
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
        if let Some(t) = app.tabs.get(app.active) {
            t.editor.update(cx, |e, cx| e.focus(window, cx));
        }
        app
    }
    fn persist(&mut self) {
        self.session.vault = self.vault.as_ref().map(|v| v.root.clone());
        self.session.tabs = self.tabs.iter().map(|t| t.doc.path.clone()).collect();
        self.session.active = self.active;
        if let Err(e) = self.session.save() {
            self.status = format!("Session: {e}");
        }
    }
    fn refresh(&mut self) {
        self.entries.clear();
        fn visit(
            v: &Vault,
            dir: &Path,
            expanded: &HashSet<PathBuf>,
            depth: usize,
            rows: &mut Vec<(Entry, usize)>,
        ) -> anyhow::Result<()> {
            for e in v.entries(dir)? {
                let descend = e.directory && expanded.contains(&e.path);
                let path = e.path.clone();
                rows.push((e, depth));
                if descend {
                    visit(v, &path, expanded, depth + 1, rows)?;
                }
            }
            Ok(())
        }
        if let Some(v) = &self.vault {
            if let Err(e) = visit(v, Path::new(""), &self.expanded, 0, &mut self.entries) {
                self.status = e.to_string();
            }
        }
    }
    fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.tabs.iter().position(|t| t.doc.path == path) {
            self.active = i;
            self.tabs[i].editor.update(cx, |e, cx| e.focus(window, cx));
            return;
        }
        let Some(v) = &self.vault else {
            return;
        };
        match v.load(&path) {
            Ok(doc) => {
                let editor = cx.new(|cx| {
                    let mut e = EditorState::new(window, cx).with_text(doc.text.clone());
                    e.set_markdown_style(crate::theme::markdown_style(), cx);
                    let vault = v.clone();
                    let note = path.clone();
                    let cache = std::cell::RefCell::new(std::collections::HashMap::new());
                    e.set_block_image_provider(move |src| {
                        if let Some(img) = cache.borrow().get(src) {
                            return Some(std::sync::Arc::clone(img));
                        }
                        let p = nxmd::attachments::resolve(&vault, &note, src).ok()?;
                        if std::fs::metadata(&p).ok()?.len() > 32 * 1024 * 1024 {
                            return None;
                        }
                        let mut reader = image::ImageReader::open(p)
                            .ok()?
                            .with_guessed_format()
                            .ok()?;
                        let mut limits = image::Limits::default();
                        limits.max_image_width = Some(8192);
                        limits.max_image_height = Some(8192);
                        reader.limits(limits);
                        let mut rgba = reader.decode().ok()?.into_rgba8();
                        for pixel in rgba.chunks_exact_mut(4) {
                            pixel.swap(0, 2);
                        }
                        let img = std::sync::Arc::new(RenderImage::new([image::Frame::new(rgba)]));
                        cache.borrow_mut().insert(src.to_string(), img.clone());
                        Some(img)
                    });
                    e
                });
                let subscription = cx.subscribe(&editor, |this, ed, event: &EditorEvent, cx| {
                    if matches!(event, EditorEvent::Changed) {
                        if let Some(tab) = this.tabs.iter_mut().find(|t| t.editor == ed) {
                            tab.dirty = Some(Instant::now());
                        }
                        cx.notify();
                    }
                });
                editor.update(cx, |e, cx| e.focus(window, cx));
                self.tabs.push(Tab {
                    doc,
                    editor,
                    dirty: None,
                    conflict: false,
                    _subscription: subscription,
                });
                self.active = self.tabs.len() - 1;
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
        t.doc.text = t.editor.read(cx).text().to_owned();
        match v.save(&mut t.doc) {
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
        for i in 0..self.tabs.len() {
            if !self.tabs[i].conflict
                && self
                    .vault
                    .as_ref()
                    .is_some_and(|v| v.changed(&self.tabs[i].doc))
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
                    self.insert_image(image.bytes(), cx);
                    return;
                }
            }
        }
        cx.propagate();
    }
    fn insert_image(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let Some(v) = &self.vault else {
            return;
        };
        match nxmd::attachments::import(v, &tab.doc.path, bytes) {
            Ok(reference) => tab.editor.update(cx, |e, cx| {
                let p = e.cursor();
                e.replace_range(p..p, &format!("\n{reference}\n"), cx);
            }),
            Err(e) => self.status = e.to_string(),
        }
        cx.notify();
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
            let _ = this.update_in(cx, |this, _, cx| {
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
                                    this.vault = Some(v);
                                    this.tabs.clear();
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
        if self.tabs.is_empty() {
            return;
        }
        if self.tabs[self.active].dirty.is_some() && !self.save_tab(self.active, cx) {
            return;
        }
        self.tabs.remove(self.active);
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        if let Some(t) = self.tabs.get(self.active) {
            t.editor.update(cx, |e, cx| e.focus(window, cx));
        }
        self.persist();
        cx.notify();
    }
    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.sidebar = !self.sidebar;
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
        if matches!(kind, NewPath::Move | NewPath::Copy) && tab.is_none() {
            return;
        }
        let old = tab.map(|t| t.doc.path.clone());
        let text = tab
            .map(|t| t.editor.read(cx).text().to_owned())
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
        let result = cx.prompt_for_new_path(&vault.root, Some(&suggested));
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
                                .position(|t| t.doc.path == old)
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
        let path = self.tabs[self.active].doc.path.clone();
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
                    if let Some(i) = this.tabs.iter().position(|t| t.doc.path == path) {
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
impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<_> = self
            .entries
            .iter()
            .map(|(e, depth)| {
                let path = e.path.clone();
                let directory = e.directory;
                let expanded = self.expanded.contains(&path);
                let label = format!(
                    "{} {}",
                    if directory {
                        if expanded { "▾" } else { "▸" }
                    } else {
                        "·"
                    },
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
                div()
                    .id(SharedString::from(path.to_string_lossy().to_string()))
                    .px_3()
                    .py_1()
                    .pl(px(12. + *depth as f32 * 14.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0x282d38)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, w, cx| {
                        if directory {
                            if !this.expanded.remove(&path) {
                                this.expanded.insert(path.clone());
                            }
                            this.refresh();
                        } else {
                            this.open(path.clone(), w, cx);
                        }
                        cx.notify();
                    }))
            })
            .collect();
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let name = format!(
                    "{}{}",
                    t.doc.path.file_name().unwrap_or_default().to_string_lossy(),
                    if t.conflict {
                        " !"
                    } else if t.dirty.is_some() {
                        " •"
                    } else {
                        ""
                    }
                );
                div()
                    .id(i)
                    .px_4()
                    .py_3()
                    .bg(rgb(if i == self.active { 0x232833 } else { 0x191d25 }))
                    .cursor_pointer()
                    .child(name)
                    .on_click(cx.listener(move |this, _, w, cx| {
                        this.active = i;
                        this.tabs[i].editor.update(cx, |e, cx| e.focus(w, cx));
                        this.persist();
                        cx.notify();
                    }))
            })
            .collect();
        let editor = self.tabs.get(self.active).map(|t| t.editor.clone());
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1d212a))
            .text_color(rgb(0xd8dce5))
            .font_family("Segoe UI")
            .text_size(px(14.))
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::paste_image))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                for path in paths.paths() {
                    match std::fs::read(path) {
                        Ok(b) => this.insert_image(&b, cx),
                        Err(e) => this.status = e.to_string(),
                    }
                }
            }))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::open_vault))
            .on_action(cx.listener(Self::new_note))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .w(px(if self.sidebar { 240. } else { 0. }))
                            .overflow_hidden()
                            .h_full()
                            .flex()
                            .flex_col()
                            .bg(rgb(0x171b23))
                            .border_r_1()
                            .border_color(rgb(0x303541))
                            .child(div().p_4().text_color(rgb(0x94b5ef)).child("nxmd"))
                            .child(
                                div()
                                    .id("open")
                                    .px_3()
                                    .py_2()
                                    .cursor_pointer()
                                    .child("Open vault   Ctrl+O")
                                    .on_click(
                                        cx.listener(|s, _, w, cx| s.open_vault(&OpenVault, w, cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("new")
                                    .px_3()
                                    .py_2()
                                    .cursor_pointer()
                                    .child("New note    Ctrl+N")
                                    .on_click(
                                        cx.listener(|s, _, w, cx| s.new_note(&NewNote, w, cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("folder")
                                    .px_3()
                                    .py_2()
                                    .cursor_pointer()
                                    .child("New folder")
                                    .on_click(cx.listener(|s, _, w, cx| s.new_folder(w, cx))),
                            )
                            .child(
                                div()
                                    .id("copy")
                                    .px_3()
                                    .py_2()
                                    .cursor_pointer()
                                    .child("Save active note as copy")
                                    .on_click(cx.listener(|s, _, w, cx| s.save_copy(w, cx))),
                            )
                            .child(
                                div()
                                    .id("move")
                                    .px_3()
                                    .py_2()
                                    .cursor_pointer()
                                    .child("Rename / move note")
                                    .on_click(cx.listener(|s, _, w, cx| s.move_note(w, cx))),
                            )
                            .child(
                                div()
                                    .id("trash")
                                    .px_3()
                                    .py_2()
                                    .cursor_pointer()
                                    .child("Move note to trash")
                                    .on_click(cx.listener(|s, _, w, cx| s.trash_note(w, cx))),
                            )
                            .child(
                                div()
                                    .id("files")
                                    .flex_1()
                                    .overflow_y_scroll()
                                    .children(rows),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().flex().children(tabs))
                            .child(
                                div()
                                    .id("document")
                                    .flex_1()
                                    .overflow_y_scroll()
                                    .p(px(40.))
                                    .text_size(px(18.))
                                    .children(editor),
                            ),
                    ),
            )
            .child(
                div()
                    .px_4()
                    .py_2()
                    .text_size(px(12.))
                    .border_t_1()
                    .border_color(rgb(0x303541))
                    .child(self.status.clone()),
            )
    }
}
