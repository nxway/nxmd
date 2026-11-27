//! Exercise geometry through actual GPUI measure/prepaint/paint and input methods.
use super::*;
use gpui::AppContext as _;

fn paint<V: Render>(view: gpui::WindowHandle<V>, cx: &mut gpui::TestAppContext) {
    cx.update_window(view.into(), |_, w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    })
    .unwrap();
}

fn press(point: Point<Pixels>) -> MouseDownEvent {
    MouseDownEvent {
        button: MouseButton::Left,
        position: point,
        click_count: 1,
        first_mouse: false,
        modifiers: Default::default(),
    }
}

#[gpui::test]
fn native_hit_test_waits_for_painted_source(cx: &mut gpui::TestAppContext) {
    let source = "**Ж**\r\n東京\ne\u{301} 👩‍💻";
    let view = cx.open_window(size(px(320.), px(240.)), |w, cx| {
        let mut e = EditorState::new(w, cx).with_text(source);
        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
        e
    });
    paint(view, cx);
    view.update(cx, |e, w, cx| {
        let at = e
            .bounds_for_offset(source.find('東').unwrap())
            .unwrap()
            .origin;
        assert!(e.character_index_for_point(at, w, cx).is_some());
        e.replace_range(0..6, "新", cx);
        assert_eq!(e.character_index_for_point(at, w, cx), None);
        e.right(&Right, w, cx);
        assert_eq!(e.cursor_offset(), "新\r\n".len());
        e.undo(&Undo, w, cx);
        assert_eq!(e.text(), source);
        assert_eq!(e.character_index_for_point(at, w, cx), None);
        e.redo(&Redo, w, cx);
        let cursor = e.cursor_offset();
        e.on_mouse_down(&press(at), w, cx);
        assert_eq!(e.cursor_offset(), cursor);
        e.undo(&Undo, w, cx);
    })
    .unwrap();
    paint(view, cx);
    view.update(cx, |e, w, cx| {
        let pos = source.find('東').unwrap();
        let at = e.bounds_for_offset(pos).unwrap().origin;
        assert_eq!(
            e.character_index_for_point(at, w, cx),
            Some(e.offset_to_utf16(pos))
        );
    })
    .unwrap();
}

#[gpui::test]
fn image_readiness_repositions_pointer_and_selection_without_edit(cx: &mut gpui::TestAppContext) {
    let ready = std::rc::Rc::new(std::cell::RefCell::new(None));
    let source = "![picture](asset.png)\r\nЖ東京 word";
    let view = cx.open_window(size(px(360.), px(300.)), |w, cx| {
        let mut e = EditorState::new(w, cx).with_text(source);
        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
        let ready = ready.clone();
        e.set_block_image_provider(move |_| ready.borrow().clone());
        e
    });
    paint(view, cx);
    let offset = source.find('東').unwrap();
    let old = view.update(cx, |e, w, cx| {
        let at = e.bounds_for_offset(offset).unwrap().origin;
        assert_eq!(e.character_index_for_point(at, w, cx), Some(e.offset_to_utf16(offset)));
        let svg = gpui::Image::from_bytes(gpui::ImageFormat::Svg,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect width="100" height="100"/></svg>"#.to_vec());
        ready.replace(Some(svg.to_image_data(cx.svg_renderer()).unwrap()));
        let revision = e.content_gen;
        e.refresh_images(cx);
        assert_eq!(e.content_gen, revision);
        assert!(e.bounds_for_offset(offset).is_none());
        let selection = e.selected_range.clone();
        e.on_mouse_down(&press(at), w, cx);
        assert_eq!(e.selected_range, selection);
        at
    }).unwrap();
    paint(view, cx);
    view.update(cx, |e, w, cx| {
        let new = e.bounds_for_offset(offset).unwrap().origin;
        assert!(new.y > old.y + px(20.));
        e.on_mouse_down(&press(new), w, cx);
        assert_eq!(e.selected_range, offset..offset);
        assert_eq!(e.text(), source);
    })
    .unwrap();
}

#[gpui::test]
fn inherited_metrics_wrap_width_resize_and_scroll_republish(cx: &mut gpui::TestAppContext) {
    struct View {
        editor: Entity<EditorState>,
        scroll: ScrollHandle,
        width: Pixels,
        font_size: Pixels,
    }
    impl Render for View {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("viewport")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .child(
                    div()
                        .w(self.width)
                        .text_size(self.font_size)
                        .child(self.editor.clone()),
                )
        }
    }
    let line = "東京 one two three four five six seven eight nine ten eleven twelve";
    let view = cx.open_window(size(px(480.), px(200.)), |w, cx| {
        let scroll = ScrollHandle::new();
        let editor = cx.new(|cx| {
            EditorState::new(w, cx)
                .with_viewport_scroll(scroll.clone())
                .with_text(vec![line; 30].join("\r\n"))
        });
        View {
            editor,
            scroll,
            width: px(400.),
            font_size: px(14.),
        }
    });
    paint(view, cx);
    let wraps = view
        .update(cx, |v, w, cx| {
            let wraps = v.editor.read(cx).layout.wrap_rows[0];
            v.width = px(132.);
            v.font_size = px(22.);
            v.editor.update(cx, |e, cx| {
                e.refresh_layout(cx);
                assert_eq!(e.character_index_for_point(Point::default(), w, cx), None);
            });
            cx.notify();
            wraps
        })
        .unwrap();
    paint(view, cx);
    view.update(cx, |v, w, cx| {
        assert!(v.editor.read(cx).layout.wrap_rows[0] > wraps);
        assert_eq!(v.editor.read(cx).layout.font_size, px(22.));
        v.scroll.set_offset(point(px(0.), -px(60.)));
        v.editor.update(cx, |e, cx| {
            assert_eq!(e.character_index_for_point(Point::default(), w, cx), None)
        });
    })
    .unwrap();
    paint(view, cx);
    cx.simulate_window_resize(view.into(), size(px(360.), px(260.)));
    view.update(cx, |v, w, cx| {
        // Deliver the platform bounds observation before asking GPUI for another frame.
        w.bounds_changed(cx);
        assert_eq!(w.viewport_size(), size(px(360.), px(260.)));
        v.editor.update(cx, |e, cx| {
            assert_eq!(e.character_index_for_point(Point::default(), w, cx), None)
        });
    })
    .unwrap();
    paint(view, cx);
    view.update(cx, |v, w, cx| {
        v.editor.update(cx, |e, cx| {
            let pos = e.line_starts()[1];
            let at = e.bounds_for_offset(pos).unwrap().origin;
            assert_eq!(
                e.character_index_for_point(at, w, cx),
                Some(e.offset_to_utf16(pos))
            );
        });
    })
    .unwrap();
}

#[gpui::test]
fn scrolled_wrapped_table_uses_painted_cell_for_native_input(cx: &mut gpui::TestAppContext) {
    let source = "<!-- table:grid cols=240,90 -->\r\n| A | B |\n| --- | --- |\r\n| one | plain two three four five six seven |";
    let view = cx.open_window(size(px(240.), px(260.)), |w, cx| {
        let mut e = EditorState::new(w, cx).with_text(source);
        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
        e
    });
    paint(view, cx);
    view.update(cx, |e, w, cx| {
        let (zone, _) = e.layout.table_hover_zones[0];
        e.on_scroll_wheel(
            &gpui::ScrollEvent {
                position: zone.center(),
                delta: gpui::ScrollDelta::Pixels(point(-px(150.), px(0.))),
                ..Default::default()
            },
            w,
            cx,
        );
        assert!(e.table_scroll_x.values().any(|v| *v > 0.));
        assert_eq!(e.character_index_for_point(zone.center(), w, cx), None);
    })
    .unwrap();
    paint(view, cx);
    view.update(cx, |e, w, cx| {
        let pos = source.find("six").unwrap();
        let unit = e.offset_to_utf16(pos);
        let bounds = e.layout.last_bounds.unwrap();
        let caret = e.bounds_for_range(unit..unit, bounds, w, cx).unwrap();
        assert!(caret.top() > bounds.top() + e.layout.line_tops[3] + px(6.));
        assert!(caret.left() < bounds.right());
        assert_eq!(e.character_index_for_point(caret.origin, w, cx), Some(unit));
        e.on_mouse_down(&press(caret.origin), w, cx);
        assert_eq!(e.cursor_offset(), pos);
        TableMenuAction::DeleteColumn.apply(e, cx);
        assert!(e.text().is_char_boundary(e.cursor_offset()));
        assert!(e.character_index_for_point(caret.origin, w, cx).is_none());
        e.replace_text_in_range(None, "字", w, cx);
        assert!(e.text().contains('字'));
    })
    .unwrap();
}

#[gpui::test]
fn focused_reveal_and_preedit_withdraw_previous_mapping(cx: &mut gpui::TestAppContext) {
    let source = "start **Ж字** end";
    let view = cx.open_window(size(px(280.), px(160.)), |w, cx| {
        let mut e = EditorState::new(w, cx).with_text(source);
        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
        e.focus(w, cx);
        e
    });
    paint(view, cx);
    let bold = source.find('Ж').unwrap();
    view.update(cx, |e, w, cx| {
        let at = e.bounds_for_offset(bold).unwrap().origin;
        e.set_cursor(bold, cx);
        assert_eq!(e.character_index_for_point(at, w, cx), None);
        e.right(&Right, w, cx);
        assert_eq!(e.cursor_offset(), bold + "Ж".len());
    })
    .unwrap();
    paint(view, cx);
    view.update(cx, |e, w, cx| {
        let pos = e.cursor_offset();
        let at = e.bounds_for_offset(pos).unwrap().origin;
        assert_eq!(
            e.character_index_for_point(at, w, cx),
            Some(e.offset_to_utf16(pos))
        );
        e.replace_and_mark_text_in_range(None, "かな", None, w, cx);
        assert_eq!(e.committed_text(), source);
        assert_eq!(e.character_index_for_point(at, w, cx), None);
    })
    .unwrap();
    paint(view, cx);
    view.update(cx, |e, w, cx| {
        let at = e.bounds_for_offset(e.cursor_offset()).unwrap().origin;
        assert!(e.character_index_for_point(at, w, cx).is_some());
        assert!(e.cancel_composition());
        assert_eq!(e.character_index_for_point(at, w, cx), None);
        assert_eq!(e.text(), source);
    })
    .unwrap();
}

#[gpui::test]
fn fresh_bidi_geometry_drives_visual_arrows(cx: &mut gpui::TestAppContext) {
    let source = "אבגד הוזח טיכל מנסע פצקר שתאב";
    let view = cx.open_window(size(px(110.), px(220.)), |w, cx| {
        let mut e = EditorState::new(w, cx).with_text(source);
        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
        e.set_cursor("אב".len(), cx);
        e.focus(w, cx);
        e
    });
    paint(view, cx);
    let (start, before) = view
        .update(cx, |e, w, cx| {
            let start = e.cursor_offset();
            let before = e.bounds_for_offset(start).unwrap();
            assert!(e.row_span(0) > 1 && e.bidi_map(0).unwrap().base_rtl);
            e.right(&Right, w, cx);
            (start, before)
        })
        .unwrap();
    paint(view, cx);
    view.update(cx, |e, w, cx| {
        let neighbor = e.cursor_offset();
        let right = e.bounds_for_offset(neighbor).unwrap();
        assert!(right.left() > before.left() && right.top() == before.top());
        e.select_left(&SelectLeft, w, cx);
        assert_eq!(e.cursor_offset(), start);
        assert_eq!(e.selected_range, start.min(neighbor)..start.max(neighbor));
        assert_eq!(
            e.text()[e.selected_range.clone()].graphemes(true).count(),
            1
        );
    })
    .unwrap();
}
