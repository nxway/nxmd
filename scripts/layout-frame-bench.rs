// In a disposable checkout of either revision, append this fixture to
// vendor/zorite-editor/src/lib.rs, then run:
// cargo test --locked -p zorite-editor --lib audit_frame_measurement -- --nocapture
// Scroll setup and highlight assertions are outside the timed window.
#[cfg(test)]
mod audit_frame_measurement {
    use super::*;
    use gpui::AppContext as _;
    use std::time::Instant;
    struct View {
        editor: Entity<EditorState>,
        scroll: ScrollHandle,
    }
    impl Render for View {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("viewport")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .child(self.editor.clone())
        }
    }
    #[gpui::test]
    fn scrolled_frame(cx: &mut gpui::TestAppContext) {
        for (label, line) in [
            ("plain", "one two three needle four five six seven"),
            ("markdown", "## [sample](path.md) **bold** needle end"),
        ] {
            for rows in [1000, 2000, 4000] {
                let text = vec![line; rows].join("\n");
                let view = cx.open_window(size(px(550.), px(400.)), |w, cx| {
                    let editor = cx.new(|cx| {
                        let mut e = EditorState::new(w, cx).with_text(text);
                        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
                        e.set_search(find_in_source(e.text(), "needle"), None, cx);
                        e
                    });
                    View {
                        editor,
                        scroll: ScrollHandle::new(),
                    }
                });
                let draw = |cx: &mut gpui::TestAppContext| {
                    cx.update_window(view.into(), |_, w, cx| {
                        w.refresh();
                        let start = Instant::now();
                        w.draw(cx).clear(cx);
                        let elapsed = start.elapsed();
                        assert!(
                            w.painted_quads()
                                .iter()
                                .any(|q| q.background == gpui::Background::from(rgba(0xFFD60055)))
                        );
                        elapsed
                    })
                    .unwrap()
                };
                draw(cx);
                let mut samples = Vec::new();
                for n in 0..5 {
                    view.update(cx, |v, _, cx| {
                        let top = v.editor.read(cx).row_layout()[rows / 2 + 10 * n].0;
                        v.scroll.set_offset(point(px(0.), -top));
                    })
                    .unwrap();
                    samples.push(draw(cx));
                }
                samples.sort();
                eprintln!("FRAME {label} {rows} median {:?}", samples[2]);
            }
        }
    }
}
