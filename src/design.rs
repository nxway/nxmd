//! Small native primitives; icons are original embedded SVG paths (no framework).
use gpui::*;
pub const BG: u32 = 0x191d1b;
pub const SIDEBAR: u32 = 0x141815;
pub const SURFACE: u32 = 0x222923;
pub const HOVER: u32 = 0x28312a;
pub const BORDER: u32 = 0x2c352e;
pub const TEXT: u32 = 0xdce4dd;
pub const MUTED: u32 = 0x8d9c91;
pub const ACCENT: u32 = 0xb3cfa2;
pub const DANGER: u32 = 0xe6a396;
pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        Ok(match path {
            "note" => Some(std::borrow::Cow::Borrowed(
                include_bytes!("../assets/icons/note.svg") as &[u8],
            )),
            "folder" => Some(std::borrow::Cow::Borrowed(
                include_bytes!("../assets/icons/folder.svg") as &[u8],
            )),
            "chevron" => Some(std::borrow::Cow::Borrowed(include_bytes!(
                "../assets/icons/chevron.svg"
            ) as &[u8])),
            "down" => Some(std::borrow::Cow::Borrowed(
                include_bytes!("../assets/icons/down.svg") as &[u8],
            )),
            "plus" => Some(std::borrow::Cow::Borrowed(
                include_bytes!("../assets/icons/plus.svg") as &[u8],
            )),
            "folder-plus" => Some(std::borrow::Cow::Borrowed(include_bytes!(
                "../assets/icons/folder-plus.svg"
            ) as &[u8])),
            "close" => Some(std::borrow::Cow::Borrowed(
                include_bytes!("../assets/icons/close.svg") as &[u8],
            )),
            "panel" => Some(std::borrow::Cow::Borrowed(
                include_bytes!("../assets/icons/panel.svg") as &[u8],
            )),
            "book" => Some(std::borrow::Cow::Borrowed(
                include_bytes!("../assets/icons/book.svg") as &[u8],
            )),
            "dots" => Some(std::borrow::Cow::Borrowed(
                include_bytes!("../assets/icons/dots.svg") as &[u8],
            )),
            _ => None,
        })
    }
    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec![])
    }
}
pub fn icon(name: &'static str) -> Svg {
    svg()
        .path(name)
        .size(px(16.))
        .text_color(rgb(MUTED))
        .flex_shrink_0()
}
pub fn button(id: impl Into<ElementId>, glyph: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(HOVER)))
        .active(|s| s.bg(rgb(BORDER)))
        .tooltip(move |_, cx| cx.new(|_| Tip(label.into())).into())
        .child(icon(glyph))
}
struct Tip(SharedString);
impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded(px(5.))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(BORDER))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(self.0.clone())
    }
}
pub fn tip(text: SharedString, cx: &mut App) -> AnyView {
    cx.new(|_| Tip(text)).into()
}
