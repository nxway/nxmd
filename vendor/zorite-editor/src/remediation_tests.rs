//! Drive the production handlers, including the dispatch used by the table popup.
use super::*;
use gpui::AppContext as _;

fn with_editor(
    cx: &mut gpui::TestAppContext,
    f: impl FnOnce(&mut EditorState, &mut Window, &mut Context<EditorState>),
) {
    let editor = cx.update(|cx| {
        cx.open_window(Default::default(), |w, cx| {
            cx.new(|cx| EditorState::new(w, cx).with_source_navigation())
        })
        .unwrap()
    });
    editor.update(cx, f).unwrap();
}

#[gpui::test]
fn editable_line_ends(cx: &mut gpui::TestAppContext) {
    with_editor(cx, |e, w, cx| {
        for (first, second) in [
            ("\r\n", "\r\n"),
            ("\n", "\n"),
            ("\r\n", "\n"),
            ("\n", "\r\n"),
        ] {
            let source = format!("one{first}two{second}三");
            e.set_text(&source, cx);
            e.doc_start(&DocStart, w, cx);
            e.end(&End, w, cx);
            e.replace_text_in_range(None, "X", w, cx);
            assert_eq!(
                e.text().as_bytes(),
                format!("oneX{first}two{second}三").as_bytes()
            );
            e.undo(&Undo, w, cx);
            assert_eq!(e.text(), source);
            e.redo(&Redo, w, cx);
            e.set_cursor(0, cx);
            e.select_end(&SelectEnd, w, cx);
            assert_eq!(&e.text()[e.selected_range.clone()], "oneX");
            e.replace_text_in_range(None, "新しい", w, cx);
            e.delete(&Delete, w, cx);
            assert_eq!(e.text(), format!("新しいtwo{second}三"));
            e.undo(&Undo, w, cx);
            e.set_cursor("新しい".len() + first.len(), cx);
            e.backspace(&Backspace, w, cx);
            assert_eq!(e.text(), format!("新しいtwo{second}三"));
            e.end(&End, w, cx);
            assert_eq!(e.cursor_offset(), "新しいtwo".len());
        }
    });
}

#[gpui::test]
fn unicode_table_actions(cx: &mut gpui::TestAppContext) {
    with_editor(cx, |e, w, cx| {
        for eol in ["\n", "\r\n"] {
            for target in ["Ж文", "中文", "👩‍💻я", "e\u{301}😀"] {
                let source = [
                    "| h | j |",
                    "| --- | --- |",
                    &format!("| abc | {target} |"),
                    &format!("| {target} | end |"),
                ]
                .join(eol);
                for action in [
                    TableMenuAction::DeleteRow,
                    TableMenuAction::DeleteColumn,
                    TableMenuAction::InsertColLeft,
                    TableMenuAction::InsertRowBelow,
                ] {
                    e.set_text(&source, cx);
                    e.set_cursor(source.find("abc").unwrap() + 1, cx);
                    action.apply(e, cx);
                    let pos = e.cursor_offset();
                    let expected = match action {
                        TableMenuAction::DeleteRow | TableMenuAction::DeleteColumn => {
                            e.text().find(target).unwrap()
                                + target.graphemes(true).next().unwrap().len()
                        }
                        TableMenuAction::InsertColLeft => e.text().find("abc").unwrap() + 1,
                        _ => e.text().find("|  |  |").unwrap() + 2,
                    };
                    assert!(
                        e.text().is_char_boundary(pos),
                        "invalid caret {pos} in {:?}",
                        e.text()
                    );
                    assert_eq!(pos, expected);
                    let expected_rows = match action {
                        TableMenuAction::DeleteRow => vec![
                            "| h | j |".into(),
                            "| --- | --- |".into(),
                            format!("| {target} | end |"),
                        ],
                        TableMenuAction::DeleteColumn => vec![
                            "| j |".into(),
                            "| --- |".into(),
                            format!("| {target} |"),
                            "| end |".into(),
                        ],
                        TableMenuAction::InsertColLeft => vec![
                            "|  | h | j |".into(),
                            "| --- | --- | --- |".into(),
                            format!("|  | abc | {target} |"),
                            format!("|  | {target} | end |"),
                        ],
                        TableMenuAction::InsertRowBelow => vec![
                            "| h | j |".into(),
                            "| --- | --- |".into(),
                            format!("| abc | {target} |"),
                            "|  |  |".into(),
                            format!("| {target} | end |"),
                        ],
                        _ => unreachable!(),
                    };
                    assert_eq!(e.text().as_bytes(), expected_rows.join(eol).as_bytes());
                    let mutated = e.text().to_owned();
                    assert!(eol == "\n" || !mutated.replace("\r\n", "").contains(['\r', '\n']));
                    e.replace_text_in_range(None, "X", w, cx);
                    assert_eq!(e.text(), format!("{}X{}", &mutated[..pos], &mutated[pos..]));
                    e.select_left(&SelectLeft, w, cx);
                    e.delete(&Delete, w, cx);
                    assert_eq!(e.text(), mutated);
                    e.undo(&Undo, w, cx);
                    e.undo(&Undo, w, cx);
                    e.undo(&Undo, w, cx);
                    assert_eq!(e.text().as_bytes(), source.as_bytes());
                    assert_eq!(e.cursor_offset(), source.find("abc").unwrap() + 1);
                }
            }
        }
    });
}

#[gpui::test]
fn arrows_without_structural_editors(cx: &mut gpui::TestAppContext) {
    with_editor(cx, |e, w, cx| {
        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
        for source in [
            "a$x$b",
            "a\n$$\nx\n$$\nb",
            "a\nkey:: value\nother:: text\nb",
            "a **bold** b",
        ] {
            e.set_text(source, cx);
            for (pos, _) in source
                .char_indices()
                .skip(1)
                .chain(std::iter::once((source.len(), ' ')))
            {
                e.right(&Right, w, cx);
                assert_eq!(e.cursor_offset(), pos, "{source:?} right");
            }
            for (pos, _) in source.char_indices().rev() {
                e.left(&Left, w, cx);
                assert_eq!(e.cursor_offset(), pos, "{source:?} left");
            }
            let pos = source.find(['x', 'v', 'o']).unwrap();
            e.set_cursor(pos - 1, cx);
            e.right(&Right, w, cx);
            e.replace_text_in_range(None, "Z", w, cx);
            assert_eq!(e.text(), format!("{}Z{}", &source[..pos], &source[pos..]));
        }
    });
}

#[test]
fn expanded_lowercase_maps_to_complete_source_scalars() {
    assert_eq!(find_in_source("İstanbul", "i"), vec![0..2]);
    assert_eq!(find_in_source("aİİz", "\u{307}"), vec![1..3, 3..5]);
    assert_eq!(find_in_source("Яİ字", "i\u{307}字"), vec![2..7]);
}

#[gpui::test]
fn find_window_tracks_source_edits_and_undo(cx: &mut gpui::TestAppContext) {
    with_editor(cx, |e, w, cx| {
        for line in ["needle", "## [needle](path) **bold**"] {
            let text = vec![line; 4000].join("\r\n");
            e.set_text(&text, cx);
            e.set_search(find_in_source(&text, "needle"), Some(3351), cx);
            let stride = line.len() + 2;
            for row in [0, 1999, 3350, 3998] {
                let source = e.line_starts()[row]..e.after_line(row + 1);
                let hits: Vec<_> = e.search.as_ref().unwrap().visible(source).collect();
                assert_eq!(hits.len(), 2);
                assert_eq!(hits[0].0, row);
                assert_eq!(hits[1].0, row + 1);
                assert_eq!(&text[hits[1].1.clone()], "needle");
            }
            let pos = 3350 * stride;
            e.set_cursor(pos, cx);
            e.replace_text_in_range(None, "新\n", w, cx);
            assert!(e.search.is_none());
            assert_eq!(e.line_end(3350), pos + "新".len());
            assert_eq!(e.row_col(pos + 4), (3351, 0));
            e.undo(&Undo, w, cx);
            assert_eq!(e.line_end(3350), pos + line.len());
            assert_eq!(e.text().as_bytes(), text.as_bytes());
            e.redo(&Redo, w, cx);
            assert_eq!(e.line_starts()[3351], pos + 4);
        }
        let custom = SearchMatches::new(vec![100..110, 0..150, 25..35], Some(2));
        assert_eq!(
            custom.visible(30..32).map(|(i, _)| i).collect::<Vec<_>>(),
            vec![1, 2]
        );
    });
}

#[gpui::test]
fn scrolled_highlights_align_with_source_after_edit(cx: &mut gpui::TestAppContext) {
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
    let line = "## **row** needle";
    let source = vec![line; 1024].join("\r\n");
    let view = cx.open_window(size(px(500.), px(300.)), |w, cx| {
        let editor = cx.new(|cx| {
            let mut e = EditorState::new(w, cx).with_text(source);
            e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
            e.set_search(find_in_source(e.text(), "needle"), Some(700), cx);
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
            w.draw(cx).clear(cx);
        })
        .unwrap();
    };
    draw(cx);
    view.update(cx, |v, _, cx| {
        let top = v.editor.read(cx).line_tops[699];
        v.scroll.set_offset(point(px(0.), -top));
    })
    .unwrap();
    draw(cx);
    for edited in [false, true] {
        view.update(cx, |v, w, cx| {
            let e = v.editor.read(cx);
            let pos = e.line_end(700) - "needle".len();
            let at = e.bounds_for_offset(pos).unwrap();
            let end = e.bounds_for_offset(pos + 6).unwrap();
            assert!(at.top() >= px(0.) && at.top() < w.viewport_size().height);
            assert_eq!(
                e.index_for_mouse_position(at.origin + point(px(0.), px(1.))),
                pos
            );
            let quads = w.painted_quads();
            let active = quads
                .iter()
                .filter(|q| q.background == gpui::Background::from(rgba(0xFF9500DD)))
                .collect::<Vec<_>>();
            assert_eq!(active.len(), 1);
            let expected =
                Bounds::from_corners(at.origin, end.bottom_right()).scale(w.scale_factor());
            let painted = active[0].bounds;
            // GPUI snaps the painted edges to device pixels.
            for (a, b) in [
                (painted.left(), expected.left()),
                (painted.top(), expected.top()),
                (painted.right(), expected.right()),
                (painted.bottom(), expected.bottom()),
            ] {
                assert!(
                    (a.as_f32() - b.as_f32()).abs() <= 1.,
                    "{painted:?} vs {expected:?}"
                );
            }
            let ordinary = quads
                .iter()
                .filter(|q| q.background == gpui::Background::from(rgba(0xFFD60055)))
                .count();
            assert!((1..30).contains(&ordinary));
            if !edited {
                v.editor.update(cx, |e, cx| {
                    e.replace_range(pos..pos, "字", cx);
                    e.set_search(find_in_source(e.text(), "needle"), Some(700), cx);
                });
            }
        })
        .unwrap();
        draw(cx);
    }
}

#[gpui::test]
fn navigate_before_table_repaint(cx: &mut gpui::TestAppContext) {
    let source = "| h | j |\r\n| --- | --- |\r\n| abc | q |\r\n| 👩‍💻文 | Ж |";
    let view = cx.open_window(size(px(500.), px(300.)), |w, cx| {
        let mut e = EditorState::new(w, cx).with_text(source);
        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
        e.set_cursor(source.find("abc").unwrap() + 1, cx);
        e
    });
    cx.update_window(view.into(), |_, w, cx| {
        w.draw(cx).clear(cx);
    })
    .unwrap();
    view.update(cx, |e, w, cx| {
        assert!(e.caret_in_table());
        TableMenuAction::DeleteRow.apply(e, cx);
        assert_eq!(e.cursor_offset(), e.text().find('文').unwrap());
        e.select_down(&SelectDown, w, cx);
        assert_eq!(e.selected_range.end, e.text().len());
        e.replace_text_in_range(None, "X", w, cx);
        assert!(e.text().ends_with("| 👩‍💻X"));
    })
    .unwrap();
}

#[gpui::test]
fn forming_a_crlf_does_not_leave_caret_between_its_bytes(cx: &mut gpui::TestAppContext) {
    with_editor(cx, |e, w, cx| {
        e.set_text("one\ntwo", cx);
        e.end(&End, w, cx);
        e.replace_text_in_range(None, "\r", w, cx);
        assert_eq!(e.cursor_offset(), 3);
        e.replace_text_in_range(None, "X", w, cx);
        assert_eq!(e.text().as_bytes(), b"oneX\r\ntwo");
    });
}

#[gpui::test]
fn block_conversion_preserves_internal_terminators(cx: &mut gpui::TestAppContext) {
    with_editor(cx, |e, w, cx| {
        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
        for (source, kind, expected) in [
            (
                "> first\r\n> second\n> third\r\nend",
                TurnKind::Bullet,
                "- first\r\n- second\n- third\r\nend",
            ),
            ("word\r\nnext", TurnKind::Code, "```\r\nword\r\n```\r\nnext"),
            (
                "```rs\r\nfirst\nsecond\r\n```\n",
                TurnKind::Quote,
                "> first\n> second\n",
            ),
        ] {
            e.set_text(source, cx);
            e.turn_into(kind, w, cx);
            assert_eq!(e.text().as_bytes(), expected.as_bytes());
            e.undo(&Undo, w, cx);
            assert_eq!(e.text().as_bytes(), source.as_bytes());
        }
    });
}

#[gpui::test]
fn changing_code_language_leaves_caret_on_body(cx: &mut gpui::TestAppContext) {
    with_editor(cx, |e, w, cx| {
        for eol in ["\r\n", "\n"] {
            let source = ["```js", "界", "```"].join(eol);
            e.set_text(&source, cx);
            e.set_code_lang(0, "rust", cx);
            assert_eq!(e.cursor_offset(), e.text().find('界').unwrap());
            e.replace_text_in_range(None, "X", w, cx);
            assert_eq!(
                e.text().as_bytes(),
                ["```rust", "X界", "```"].join(eol).as_bytes()
            );
            e.undo(&Undo, w, cx);
            e.undo(&Undo, w, cx);
            assert_eq!(e.text().as_bytes(), source.as_bytes());
        }
    });
}

#[gpui::test]
fn composition_is_one_committed_edit(cx: &mut gpui::TestAppContext) {
    let e = cx.add_window(|w, cx| EditorState::new(w, cx).with_text("before OLD after"));
    let changes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = changes.clone();
    let _sub = e
        .update(cx, |_, _, cx| {
            cx.subscribe(&cx.entity(), move |e, _, event: &EditorEvent, _| {
                if matches!(event, EditorEvent::Changed) {
                    observed.borrow_mut().push(e.text().to_owned());
                }
            })
        })
        .unwrap();
    e.update(cx, |e, w, cx| {
        e.selected_range = 7..10;
        e.selection_reversed = true;
        e.replace_and_mark_text_in_range(None, "か", Some(1..1), w, cx);
        e.replace_and_mark_text_in_range(None, "かな", Some(2..2), w, cx);
        assert_eq!(e.text(), "before かな after");
    })
    .unwrap();
    assert!(
        changes.borrow().is_empty(),
        "provisional changes: {:?}",
        changes.borrow()
    );
    e.update(cx, |e, w, cx| {
        e.replace_text_in_range(None, "", w, cx);
        assert_eq!(e.text(), "before OLD after");
        assert_eq!(e.selected_range, 7..10);
        assert!(e.selection_reversed);
        assert!(e.undo_stack.is_empty());
        e.replace_and_mark_text_in_range(None, "かな", None, w, cx);
        e.replace_text_in_range(None, "仮名", w, cx);
        assert_eq!(e.undo_stack.len(), 1);
    })
    .unwrap();
    assert_eq!(*changes.borrow(), ["before 仮名 after"]);
    e.update(cx, |e, w, cx| {
        e.undo(&Undo, w, cx);
        assert_eq!(e.text(), "before OLD after");
        assert_eq!(e.selected_range, 7..10);
        assert!(e.selection_reversed);
        e.redo(&Redo, w, cx);
        assert_eq!(e.text(), "before 仮名 after");
        e.undo(&Undo, w, cx);
        assert_eq!(e.selected_range, 7..10);
    })
    .unwrap();
}

#[gpui::test]
fn durable_text_excludes_preedit_across_host_edits(cx: &mut gpui::TestAppContext) {
    with_editor(cx, |e, w, cx| {
        e.set_text("前OLD後", cx);
        e.selected_range = "前".len().."前OLD".len();
        e.replace_and_mark_text_in_range(None, "かな", Some(1..1), w, cx);
        assert_eq!(e.committed_text(), "前OLD後");
        e.replace_and_mark_text_in_range(None, "候補", None, w, cx);
        assert_eq!(e.committed_text(), "前OLD後");
        e.unmark_text(w, cx);
        assert_eq!(e.text(), "前OLD後");
        assert_eq!(&e.text()[e.selected_range.clone()], "OLD");
        e.replace_and_mark_text_in_range(None, "😀", None, w, cx);
        let end = e.text().len();
        e.replace_range(end..end, "!", cx);
        assert_eq!(e.committed_text(), "前OLD後!");
        assert_eq!(e.text(), e.committed_text());
        e.undo(&Undo, w, cx);
        assert_eq!(e.text(), "前OLD後");
        e.set_cursor(0, cx);
        e.replace_and_mark_text_in_range(Some(1..4), "漢字", Some(1..1), w, cx);
        e.replace_text_in_range(Some(1..3), "語", w, cx);
        assert_eq!(e.committed_text(), "前語後");
        e.undo(&Undo, w, cx);
        assert_eq!(e.text(), "前OLD後");
        assert_eq!(e.selected_range, 0..0);
    });
}
