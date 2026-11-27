//! A recovered document has no save-to-original action. Closing the review
//! retains the checkpoint and releases its lease for a later review.
use crate::{design::*, ui::Save};
use gpui::*;
use nxmd::{
    recovery::{Candidate, Draft, Slot},
    vault::Vault,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};
use zorite_editor::{EditorEvent, EditorState};
pub fn action(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_sm()
        .bg(rgb(SURFACE))
        .text_color(rgb(ACCENT))
        .cursor_pointer()
        .child(label)
}
pub fn copy_to(path: &Path, text: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md")),
        "Choose a new .md filename"
    );
    let parent = dunce::canonicalize(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Missing directory"))?,
    )?;
    anyhow::ensure!(
        !parent
            .components()
            .any(|c| c.as_os_str().eq_ignore_ascii_case(".obsidian")),
        "Obsidian configuration is protected"
    );
    Vault::open(parent)?.save_copy(
        Path::new(
            path.file_name()
                .ok_or_else(|| anyhow::anyhow!("Missing name"))?,
        ),
        text,
    )
}
pub fn inspect(text: String, cx: &mut App) -> anyhow::Result<()> {
    cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("nxmd — Disk version".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |_, cx| cx.new(|_| Disk(text)),
    )?;
    Ok(())
}
struct Disk(String);
impl Render for Disk {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("disk-preview")
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .p_4()
            .overflow_y_scroll()
            .child(self.0.clone())
    }
}
pub fn open(candidate: Candidate, cx: &mut App) -> anyhow::Result<()> {
    cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("nxmd — Recovered draft".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |w, cx| cx.new(|cx| Review::new(candidate, w, cx)),
    )?;
    Ok(())
}
struct Review {
    draft: Draft,
    slot: Slot,
    editor: Entity<EditorState>,
    scroll: ScrollHandle,
    generation: u64,
    last: Instant,
    status: String,
    busy: bool,
    subscription: Option<Subscription>,
    _timer: Task<()>,
}
impl Review {
    fn new(c: Candidate, w: &mut Window, cx: &mut Context<Self>) -> Self {
        let scroll = ScrollHandle::new();
        let ed = cx.new(|cx| {
            EditorState::new(w, cx)
                .with_viewport_scroll(scroll.clone())
                .with_text(c.draft.text.clone())
        });
        let sub = cx.subscribe(&ed, |s: &mut Self, _, event, cx| {
            if matches!(event, EditorEvent::ClipboardWriteFailed) {
                s.status = "Could not copy the selected text to the clipboard.".into();
                cx.notify();
            } else if matches!(event, EditorEvent::Changed) {
                s.generation += 1;
            }
        });
        let weak = cx.entity().downgrade();
        w.on_window_should_close(cx, move |_, cx| {
            weak.update(cx, |s, cx| {
                if !s.flush(cx) {
                    return false;
                }
                // Ignore native input delivered after the final checkpoint.
                s.subscription.take();
                true
            })
            .unwrap_or(true)
        });
        let timer = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this
                    .update(cx, |s, cx| {
                        if s.last.elapsed() >= Duration::from_secs(1) {
                            s.flush(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        ed.update(cx, |e, cx| e.focus(w, cx));
        let disk_status = Vault::open(&c.draft.vault)
            .and_then(|v| v.load(&c.draft.note))
            .map_or_else(
                |e| format!("Original unavailable ({e})"),
                |disk| {
                    if nxmd::recovery::fingerprint(disk.baseline()) == c.draft.baseline {
                        "Original matches the editing baseline".into()
                    } else {
                        "Original changed on disk".into()
                    }
                },
            );
        Self {
            generation: c.draft.generation,
            slot: c.slot,
            draft: c.draft,
            editor: ed,
            scroll,
            last: Instant::now(),
            status: format!("{disk_status}. Save a separate copy or close to keep this draft."),
            busy: false,
            subscription: Some(sub),
            _timer: timer,
        }
    }
    fn flush(&mut self, cx: &mut Context<Self>) -> bool {
        if self.draft.generation == self.generation {
            return true;
        }
        self.last = Instant::now();
        let mut d = self.draft.clone();
        d.generation = self.generation;
        d.updated = SystemTime::now();
        d.text = self.editor.read(cx).committed_text().into();
        match self.slot.checkpoint(&d) {
            Ok(()) => {
                self.draft = d;
                true
            }
            Err(e) => {
                self.status = format!("Draft write failed: {e}");
                cx.notify();
                false
            }
        }
    }
    fn copy(&mut self, _: &Save, w: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.flush(cx) {
            return;
        }
        let snapshot_generation = self.generation;
        let text = self.draft.text.clone();
        let original = self.draft.vault.join(&self.draft.note);
        let start = original
            .parent()
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| Path::new("."));
        let suggestion = format!(
            "{} recovered.md",
            original.file_stem().unwrap_or_default().to_string_lossy()
        );
        let res = cx.prompt_for_new_path(start, Some(&suggestion));
        self.busy = true;
        cx.spawn_in(w, async move |this, cx| {
            let r = res.await;
            let _ = this.update_in(cx, |s, w, cx| {
                s.busy = false;
                let result = (|| -> anyhow::Result<()> {
                    let Some(p) = r?? else { return Ok(()) };
                    anyhow::ensure!(
                        !same_destination(&original, &p),
                        "Choose a different filename"
                    );
                    copy_to(&p, &text)?;
                    if s.generation == snapshot_generation {
                        s.slot.retire(snapshot_generation)?;
                        w.remove_window();
                    } else {
                        s.status = "Copy saved; newer edits remain.".into();
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    s.status = format!("{e:#}")
                }
                cx.notify();
            });
        })
        .detach();
    }
}
fn same_destination(a: &Path, b: &Path) -> bool {
    let normalize = |p: &Path| -> PathBuf {
        p.parent()
            .and_then(|x| dunce::canonicalize(x).ok())
            .and_then(|d| Some(d.join(p.file_name()?)))
            .unwrap_or_else(|| p.into())
    };
    normalize(a).as_os_str().to_string_lossy().to_lowercase()
        == normalize(b).as_os_str().to_string_lossy().to_lowercase()
}
impl Render for Review {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .on_action(cx.listener(Self::copy))
            .child(format!(
                "Recovered · {}",
                self.draft.vault.join(&self.draft.note).display()
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        action("rec-copy", "Save as copy…")
                            .on_click(cx.listener(|s, _, w, cx| s.copy(&Save, w, cx))),
                    )
                    .child(
                        action("rec-discard", "Discard draft…").on_click(cx.listener(
                            |s, _, w, cx| {
                                if s.busy {
                                    return;
                                }
                                s.busy = true;
                                let snapshot_generation = s.generation;
                                let ask = w.prompt(
                                    PromptLevel::Warning,
                                    "Discard this recovery draft?",
                                    None,
                                    &[PromptButton::cancel("Keep"), PromptButton::ok("Discard")],
                                    cx,
                                );
                                cx.spawn_in(w, async move |this, cx| {
                                    let result = ask.await;
                                    let _ = this.update_in(cx, |s, w, cx| {
                                        s.busy = false;
                                        if matches!(result, Ok(1))
                                            && snapshot_generation == s.generation
                                        {
                                            match s.slot.retire(snapshot_generation) {
                                                Ok(()) => w.remove_window(),
                                                Err(e) => s.status = e.to_string(),
                                            }
                                        }
                                        cx.notify()
                                    });
                                })
                                .detach();
                            },
                        )),
                    ),
            )
            .child(self.status.clone())
            .child(
                div()
                    .id("review-text")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(self.editor.clone()),
            )
    }
}
