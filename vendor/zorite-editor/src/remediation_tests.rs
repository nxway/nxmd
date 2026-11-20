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
