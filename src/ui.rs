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
actions!(
    nxmd,
    [
        Save,
        OpenVault,
        NewNote,
        CloseTab,
        ToggleSidebar,
        DismissMenu
    ]
);
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
        if let Some(t) = app.tabs.get(app.active) {
            t.editor.update(cx, |e, cx| e.focus(window, cx));
        }
        app
    }
    fn persist(&mut self) {
        self.session.vault = self.vault.as_ref().map(|v| v.root.clone());
        self.session.tabs = self.tabs.iter().map(|t| t.doc.path.clone()).collect();
        self.session.active = self.active;
        self.session.sidebar_width = self.sidebar_width;
        self.session.sidebar_visible = self.sidebar;
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
        self.tabs.remove(index);
        if let Some(t) = self.tabs.get(self.active) {
            t.editor.update(cx, |e, cx| e.focus(window, cx));
        } else {
            window.focus(&self.focus, cx);
        }
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
            if self
                .tabs
                .get(self.active)
                .is_some_and(|t| t.doc.path == path)
            {
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
impl Workspace {
    fn tree_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        use crate::design::*;
        let (entry, depth) = &self.entries[index];
        let path = entry.path.clone();
        let directory = entry.directory;
        let expanded = self.expanded.contains(&path);
        let active = self
            .tabs
            .get(self.active)
            .is_some_and(|t| t.doc.path == path);
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
                icon(if directory { "folder" } else { "note" }).text_color(rgb(if directory {
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
                    s.refresh();
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
        let names: Vec<&'static str> = if directory {
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::design::*;
        let vault_name = self
            .vault
            .as_ref()
            .and_then(|v| v.root.file_name())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Your workspace".into());
        let active = self.tabs.get(self.active);
        let active_path = active
            .map(|t| t.doc.path.to_string_lossy().to_string())
            .unwrap_or_else(|| "No note selected".into());
        let save_label = active
            .map(|t| {
                if t.conflict {
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
        let editor = active.map(|t| t.editor.clone());
        let tabs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let selected = i == self.active;
                let title = t
                    .doc
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let tooltip: SharedString = t.doc.path.to_string_lossy().to_string().into();
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
                    .child(icon("note").size(px(14.)).text_color(rgb(if selected {
                        ACCENT
                    } else {
                        MUTED
                    })))
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
                        s.tabs[i].editor.update(cx, |e, cx| e.focus(w, cx));
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
        let main_content = if let Some(editor) = editor {
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
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::dismiss_menu))
            .on_action(cx.listener(Self::paste_image))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::open_vault))
            .on_action(cx.listener(Self::new_note))
            .on_drop(cx.listener(|s, paths: &ExternalPaths, _, cx| {
                for path in paths.paths() {
                    match std::fs::read(path) {
                        Ok(b) => s.insert_image(&b, cx),
                        Err(e) => s.status = e.to_string(),
                    }
                }
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
                                    .child(div().text_color(rgb(ACCENT)).child("LIVE PREVIEW")),
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
                    .child(div().text_color(rgb(MUTED)).child("Markdown  ·  UTF-8")),
            )
            .children(menu)
    }
}
