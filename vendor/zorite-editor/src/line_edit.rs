//! Plan one source replacement. The editor records it as one undoable edit.
use crate::markdown_syntax::{blockquote_prefix, list_prefix, task_prefix};
use std::ops::Range;

pub(crate) fn newline(
    source: &str,
    mut range: Range<usize>,
    hard: bool,
    markdown: bool,
) -> (Range<usize>, String) {
    // End on a CRLF row can be reported between its two bytes.
    for offset in [&mut range.start, &mut range.end] {
        if source.as_bytes().get(*offset) == Some(&b'\n')
            && *offset > 0
            && source.as_bytes()[*offset - 1] == b'\r'
        {
            *offset -= 1;
        }
    }
    let before = &source[..range.start];
    let start = before.rfind('\n').map_or(0, |n| n + 1);
    let rest = &source[start..];
    let end = rest.find('\n').unwrap_or(rest.len());
    let line = rest[..end].strip_suffix('\r').unwrap_or(&rest[..end]);
    let eol = if rest[..end].ends_with('\r')
        || (end == rest.len()
            && before
                .rsplit_once('\n')
                .is_some_and(|(prior, _)| prior.ends_with('\r')))
    {
        "\r\n"
    } else {
        "\n"
    };
    if !markdown || fenced(&source[..start], line) {
        return (range, eol.into());
    }
    let quote = blockquote_prefix(line).unwrap_or(0);
    let body = &line[quote..];
    let mut insertion = String::new();
    if hard {
        // A Markdown hard break terminates inline content. On a blank row
        // the same two spaces would become selectable indentation.
        let left = &before[(start + quote).min(range.start)..];
        let marker = task_prefix(left)
            .map(|(len, ..)| len)
            .or_else(|| list_prefix(left).map(|(len, ..)| len))
            .unwrap_or(0);
        if !left[marker..].trim_matches([' ', '\t']).is_empty() {
            let spaces = left
                .bytes()
                .rev()
                .take_while(|&b| b == b' ')
                .take(2)
                .count();
            insertion.push_str(&" ".repeat(2 - spaces));
        }
    }
    insertion.push_str(eol);
    insertion.push_str(&line[..quote]);
    // On a hanging continuation, Enter resumes the owning item's list.
    let owner = if list_prefix(body).is_some() {
        Some(body)
    } else {
        let ws = body.len() - body.trim_start_matches([' ', '\t']).len();
        let mut owner = None;
        let mut loose = false;
        if ws > 0 {
            for prior in source[..start].lines().rev() {
                if prior.trim().is_empty() {
                    loose = true;
                    continue;
                }
                let Some(prior) = prior.strip_prefix(&line[..quote]) else {
                    break;
                };
                if let Some((prefix, indent, ..)) = list_prefix(prior) {
                    if ws >= prefix || (!loose && ws >= indent) {
                        owner = Some(prior);
                        break;
                    }
                    if indent == 0 {
                        break;
                    }
                    // The line may resume an outer item after a nested list.
                } else if !prior.starts_with([' ', '\t']) {
                    break;
                }
            }
        }
        owner
    };
    if let Some((marker, (prefix, indent, ordered, number))) =
        owner.and_then(|m| Some((m, list_prefix(m)?)))
    {
        let content = task_prefix(marker).map_or(prefix, |(len, ..)| len);
        if range.is_empty()
            && !hard
            && list_prefix(body).is_some()
            && body[content..].trim().is_empty()
        {
            return (start + quote..start + line.len(), String::new());
        }
        if list_prefix(body).is_none() || range.start >= start + quote + prefix {
            insertion.push_str(&marker[..indent]);
            if hard {
                insertion.push_str(&" ".repeat(content - indent));
            } else {
                if ordered {
                    insertion.push_str(&number.saturating_add(1).to_string());
                    insertion.push(marker.as_bytes()[prefix - 2] as char);
                    insertion.push(' ');
                } else {
                    insertion.push_str(&marker[indent..prefix]);
                }
                if content != prefix {
                    insertion.push_str("[ ] ");
                }
            }
        }
    } else if hard {
        insertion.push_str(&body[..body.len() - body.trim_start_matches([' ', '\t']).len()]);
    }
    (range, insertion)
}

fn fenced(previous: &str, current: &str) -> bool {
    let mut open = None;
    for line in previous.lines() {
        let Some((ch, len, suffix)) = fence_marker(line) else {
            continue;
        };
        match open {
            Some((mark, width)) if mark == ch && len >= width && suffix.trim().is_empty() => {
                open = None
            }
            None => open = Some((ch, len)),
            _ => {}
        }
    }
    open.is_some() || fence_marker(current).is_some()
}

fn fence_marker(line: &str) -> Option<(u8, usize, &str)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let ch = *trimmed.as_bytes().first()?;
    if !matches!(ch, 96 | 126) {
        return None;
    }
    let len = trimmed.bytes().take_while(|&b| b == ch).count();
    (len >= 3).then_some((ch, len, &trimmed[len..]))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn end(s: &str, hard: bool) -> String {
        let (r, t) = newline(s, s.len()..s.len(), hard, true);
        let mut out = s.to_owned();
        out.replace_range(r, &t);
        out
    }
    #[test]
    fn loose_nested_item_resumes_its_owning_list() {
        let source = "7. Outer\n   1. Inner\n\n   continued";
        assert_eq!(end(source, false), format!("{source}\n8. "));
        assert_eq!(end(source, true), format!("{source}  \n   "));
        let tight = "7. Outer\n   1. Inner\n   continued";
        assert_eq!(end(tight, false), format!("{tight}\n   2. "));
        let independent = "1. Earlier\n\nUnrelated\n\n   prose";
        assert_eq!(end(independent, false), format!("{independent}\n"));
    }

    #[test]
    fn hard_break_on_blank_row_does_not_create_selectable_spaces() {
        for eol in ["\n", "\r\n"] {
            let source = format!("МММ{eol}");
            assert_eq!(end(&source, true), format!("{source}{eol}"));
        }
    }

    #[test]
    fn hard_break_only_marks_inline_content_before_the_selection() {
        for (source, caret, insertion) in [
            ("word", 0, "\n"),
            ("word", 2, "  \n"),
            (" ", 1, "\n "),
            ("> ", 2, "\n> "),
            ("- ", 2, "\n  "),
            ("- [ ] ", 6, "\n      "),
            ("- body", 6, "  \n  "),
        ] {
            assert_eq!(newline(source, caret..caret, true, true).1, insertion);
        }
        assert_eq!(newline("one", 0..3, true, true), (0..3, "\n".into()));
    }

    #[test]
    fn source_breaks() {
        assert_eq!(end("hello", false), "hello\n");
        assert_eq!(end("hello ", true), "hello  \n");
        assert_eq!(end("a\r\nb", true), "a\r\nb  \r\n");
        assert_eq!(end("   7) [x] task", false), "   7) [x] task\n   8) [ ] ");
        assert_eq!(end("   * task", true), "   * task  \n     ");
        assert_eq!(end("   * ", false), "");
        assert_eq!(
            end("7) [x] Task  \n       continued", false),
            "7) [x] Task  \n       continued\n8) [ ] "
        );
        assert_eq!(end("> quote", true), "> quote  \n> ");
        assert_eq!(end("~~~rs\n- literal", true), "~~~rs\n- literal\n");
        let (r, t) = newline("a\r\nb", 2..2, false, true);
        assert_eq!((r, t), (1..1, "\r\n".into()));
    }
}
