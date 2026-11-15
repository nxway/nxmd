//! Run the exact vendored projection code without GPUI's test-platform dependency.
#![cfg(feature = "desktop")]
#![allow(dead_code)]
pub use zorite_editor::Diagnostic;
#[path = "../vendor/zorite-editor/src/image_embed.rs"]
mod image_embed;
#[path = "../vendor/zorite-editor/src/markdown_syntax.rs"]
mod markdown_syntax;
