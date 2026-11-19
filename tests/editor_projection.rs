//! Run the exact vendored projection code without GPUI's test-platform dependency.
#![cfg(feature = "desktop")]
#![allow(dead_code)]
pub use zorite_editor::Diagnostic;
#[path = "../vendor/zorite-editor/src/image_embed.rs"]
mod image_embed;
#[path = "../vendor/zorite-editor/src/markdown_syntax.rs"]
mod markdown_syntax;

#[path = "../vendor/zorite-editor/src/table_layout.rs"]
mod table_layout;

#[test]
fn outer_list_resumes_after_nested_continuation() {
    for marker in ["   1. inner", "   4. inner"] {
        for (continuation, expected) in [
            ("paragraph", 1),
            ("   outer continuation", 8),
            ("      inner continuation", 8),
        ] {
            let lines = ["7. outer", "", marker, "", continuation, "1. next"];
            assert_eq!(markdown_syntax::ordered_numbers(&lines)[5].0, expected);
        }
    }
}

#[path = "../vendor/zorite-editor/src/line_edit.rs"]
mod line_edit;
