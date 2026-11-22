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

#[gpui::test]
fn multiline_paste_preserves_source_endings(cx: &mut gpui::TestAppContext) {
    with_editor(cx, |e, window, cx| {
        for (source, expected) in [
            ("one\r\ntwo\n三", "one\r\ntwo\n三"),
            ("one\ntwo\r\n三", "one\ntwo\r\n三"),
            ("one\rtwo\r\n三", "one\ntwo\r\n三"),
        ] {
            e.set_text("old", cx);
            e.select_all(&SelectAll, window, cx);
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(source.to_owned()));
            e.paste(&Paste, window, cx);
            assert_eq!(e.text().as_bytes(), expected.as_bytes());
            e.undo(&Undo, window, cx);
            assert_eq!(e.text(), "old");
            e.redo(&Redo, window, cx);
            assert_eq!(e.text().as_bytes(), expected.as_bytes());
        }
    });
}

#[gpui::test]
fn exercise_table_layout(cx: &mut gpui::TestAppContext) {
    let source = include_str!("../../../tests/fixtures/responsive-exercise-tables.md");
    with_editor(cx, |e, w, cx| {
        e.set_text(source, cx);
        let scan = e.scan_data();
        assert_eq!(scan.tables.len(), 2);
        let style = markdown_syntax::tests::test_style();
        for region in &scan.tables {
            let metrics = tables::measure_table_columns(
                e,
                region,
                w,
                &w.text_style().font(),
                px(18.),
                Hsla::default(),
                Some(&style),
            );
            let cols = metrics.allocate(Some(px(400.)), |_| None).widths;
            let total: Pixels = cols.iter().copied().sum();
            assert!(
                total <= px(400.),
                "table at row {} occupies {total:?} of 400px",
                region.lines.start
            );
        }
    });
}

#[gpui::test]
fn table_reflow_keeps_hits_selection_and_source(cx: &mut gpui::TestAppContext) {
    let source = include_str!("../../../tests/fixtures/responsive-exercise-tables.md");
    let view = cx.open_window(size(px(444.), px(700.)), |w, cx| {
        let mut e = EditorState::new(w, cx).with_text(source);
        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
        e.focus(w, cx);
        e
    });
    let draw = |cx: &mut gpui::TestAppContext| {
        cx.update_window(view.into(), |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
    };
    for width in [230., 444., 760., 444.] {
        cx.simulate_window_resize(view.into(), size(px(width), px(700.)));
        draw(cx);
        view.update(cx, |e, w, _| {
            let bounds = e.last_bounds.unwrap();
            let a = e.table_rows[2].as_ref().unwrap();
            let body = e.table_rows[4].as_ref().unwrap();
            let b = e.table_rows[13].as_ref().unwrap();
            assert!(std::rc::Rc::ptr_eq(&a.col_widths, &body.col_widths));
            assert!(!std::rc::Rc::ptr_eq(&a.col_widths, &b.col_widths));
            assert_eq!(!e.table_thumbs.is_empty(), width == 230.);
            if width >= 444. {
                assert!(a.col_widths[4] > a.col_widths[1] && a.col_widths[0] > a.col_widths[2]);
                // Keep the longest rep range (30–45s) on a single visual line.
                let plank = e.table_rows[9].as_ref().unwrap();
                let inner = plank.col_widths[2] - plank.pad * 2.;
                let reps =
                    tables::shape_cell(w, &plank.projected[2], e.font_size, Some(inner)).unwrap();
                assert!(reps.wrap_boundaries().is_empty());
                // The end of Notes is on a continuation line. Hit that actual
                // caret and round-trip the mapped source offset after resize.
                let pos = source.find("movement").unwrap() + 3;
                let col = pos - e.line_starts()[4];
                let (x, y, cell, _) = table_caret_pos(
                    body,
                    col,
                    e.table_left(body, 4, &bounds),
                    &e.paint_font.clone().unwrap(),
                    e.font_size,
                    w,
                )
                .unwrap();
                assert_eq!(cell, 4);
                assert!(y > px(0.) && e.line_h(4) >= y + e.font_size * LINE_HEIGHT_RATIO);
                assert!(x < bounds.right() - px(TABLE_GUTTER));
                let hit = point(x, bounds.top() + e.line_tops[4] + px(7.) + y);
                assert_eq!(e.table_offset_at(hit, w), Some(pos));
            }
            assert_eq!(e.text().as_bytes(), source.as_bytes());
        })
        .unwrap();
    }
    let start = source.find("Hold dumbbell").unwrap();
    let end = source.find("movement").unwrap() + "movement".len();
    let copied = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    view.update(cx, |e, w, cx| {
        let result = copied.clone();
        e.set_clipboard_writer(std::rc::Rc::new(move |s, _| {
            *result.borrow_mut() = s.to_owned()
        }));
        e.selected_range = start..end;
        e.copy(&Copy, w, cx);
        assert_eq!(copied.borrow().as_str(), &source[start..end]);
        e.replace_text_in_range(None, "Жим 中文 👩‍💻", w, cx);
    })
    .unwrap();
    draw(cx);
    let edited_height = view
        .update(cx, |e, w, cx| {
            assert_eq!(
                e.text(),
                source.replacen(&source[start..end], "Жим 中文 👩‍💻", 1)
            );
            let height = e.line_h(4);
            e.undo(&Undo, w, cx);
            assert_eq!(e.text(), source);
            height
        })
        .unwrap();
    draw(cx);
    view.update(cx, |e, w, cx| {
        assert!(e.line_h(4) > edited_height);
        e.redo(&Redo, w, cx);
        assert_eq!(e.cursor_offset(), start + "Жим 中文 👩‍💻".len());
    })
    .unwrap();
}

#[gpui::test]
fn manual_table_widths_survive_save_close_and_reopen(cx: &mut gpui::TestAppContext) {
    let source = "| Name | Value |\r\n| --- | --- |\r\n| one | two |\r\n\r\n| Name | Value |\r\n| --- | --- |\r\n| abc | 日本語 |";
    let open = |cx: &mut gpui::TestAppContext, text: String| {
        cx.open_window(size(px(500.), px(400.)), |w, cx| {
            let mut e = EditorState::new(w, cx).with_text(text);
            e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
            e
        })
    };
    let view = open(cx, source.to_owned());
    cx.update_window(view.into(), |_, w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    })
    .unwrap();
    let saved = view
        .update(cx, |e, w, cx| {
            let width = f32::from(e.table_rows[0].as_ref().unwrap().col_widths[1]);
            let start = source.find("日本語").unwrap();
            e.selected_range = start..start + "日本".len();
            e.selection_reversed = true;
            let mut drag = TableColResize {
                header_row: 0,
                col: 1,
                start_x: px(0.),
                orig: width,
                width,
            };
            e.table_col_resize = Some(drag);
            e.on_mouse_up(&Default::default(), w, cx);
            assert_eq!(e.text(), source);
            assert!(e.undo_stack.is_empty());
            drag.width = 240.;
            e.table_col_resize = Some(drag);
            e.on_mouse_up(&Default::default(), w, cx);
            let persisted = e.text().to_owned();
            let (marker, rest) = persisted.split_once("\r\n").unwrap();
            assert!(
                marker.starts_with("<!-- table:grid cols=") && marker.ends_with(",240 -->"),
                "{marker}"
            );
            assert_eq!(rest.as_bytes(), source.as_bytes());
            assert_eq!(&persisted[e.selected_range.clone()], "日本");
            assert!(e.selection_reversed);
            e.undo(&Undo, w, cx);
            assert_eq!(e.text(), source);
            e.redo(&Redo, w, cx);
            assert_eq!(e.text(), persisted);
            w.remove_window();
            persisted
        })
        .unwrap();
    // A new window and state have no geometry or history from the drag.
    let reopened = open(cx, saved.clone());
    for width in [360., 720.] {
        cx.simulate_window_resize(reopened.into(), size(px(width), px(400.)));
        cx.update_window(reopened.into(), |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
        reopened
            .update(cx, |e, _, _| {
                assert_eq!(e.table_rows[1].as_ref().unwrap().col_widths[1], px(240.));
                assert!(e.table_rows[5].as_ref().unwrap().col_widths[1] < px(100.));
                assert!(e.undo_stack.is_empty());
                assert_eq!(e.text().as_bytes(), saved.as_bytes());
            })
            .unwrap();
    }
    reopened
        .update(cx, |e, w, cx| {
            // Replace a previously authored marker in place. Later edits move its
            // table and undo restores it without transferring widths to table two.
            let drag = TableColResize {
                header_row: 1,
                col: 1,
                start_x: px(0.),
                orig: 240.,
                width: 125.,
            };
            e.table_col_resize = Some(drag);
            e.on_mouse_up(&Default::default(), w, cx);
            assert_eq!(e.text(), saved.replacen(",240 -->", ",125 -->", 1));
            e.replace_range(0..0, "Intro\r\n", cx);
            assert_eq!(e.scan_data().tables[0].lines.start, 2);
            e.undo(&Undo, w, cx);
            e.undo(&Undo, w, cx);
            assert_eq!(e.text(), saved);
        })
        .unwrap();
}

#[gpui::test]
fn oversized_table_scroll_clamps_after_resize(cx: &mut gpui::TestAppContext) {
    let token =
        "https://example.com/abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz";
    let row = format!("| Жим | 中文 | 👩‍💻 | 60s | {token} | end |");
    let source = format!(
        "| Exercise | Sets | Reps | Rest | URL | Cue |\n| --- | --- | --- | --- | --- | --- |\n{}",
        vec![row.as_str(); 40].join("\n")
    );
    let view = cx.open_window(size(px(350.), px(300.)), |w, cx| {
        let mut e = EditorState::new(w, cx).with_text(&source);
        e.set_markdown_style(markdown_syntax::tests::test_style(), cx);
        e
    });
    for width in [350., 1800., 350.] {
        cx.simulate_window_resize(view.into(), size(px(width), px(300.)));
        cx.update_window(view.into(), |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
        view.update(cx, |e, _, _| {
            let t = e.table_rows[0].as_ref().unwrap();
            let total: Pixels = t.col_widths.iter().copied().sum();
            let bounds = e.last_bounds.unwrap();
            if width == 350. {
                assert_eq!(e.table_thumbs.len(), 1);
                assert!(t.col_widths[4] > bounds.size.width);
                assert!(e.line_tops[40] > px(300.));
                assert_eq!(
                    e.table_left(t, 0, &bounds),
                    bounds.left() + px(TABLE_GUTTER)
                );
                e.table_scroll_x.insert(0, f32::from(total));
                assert!(
                    (e.table_left(t, 0, &bounds) + total - (bounds.right() - px(TABLE_GUTTER)))
                        .abs()
                        < px(0.01)
                );
            } else {
                assert!(e.table_thumbs.is_empty() && e.table_scroll_x.is_empty());
            }
            assert_eq!(e.text(), source);
        })
        .unwrap();
    }
}
