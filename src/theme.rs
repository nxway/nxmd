use gpui::{font, hsla};
use zorite_editor::SyntaxStyle;
pub fn markdown_style() -> SyntaxStyle {
    SyntaxStyle {
        block_label: None,
        block_label_gen: 0,
        block_ref_count: None,
        marker: hsla(0., 0., 0.5, 0.55), // dimmed gray syntax markers
        code: hsla(0.09, 0.6, 0.72, 1.), // warm inline code text
        code_bg: hsla(0., 0., 1., 0.06), // faint code chip background
        link: hsla(0.58, 0.75, 0.66, 1.), // blue links / wiki-links
        tag: hsla(0.33, 0.45, 0.62, 1.), // green tags
        quote: hsla(0., 0., 0.6, 1.),    // muted blockquote text/border
        alert_note: hsla(0.58, 0.9, 0.62, 1.), // GitHub alert blues/greens…
        alert_tip: hsla(0.36, 0.5, 0.48, 1.),
        alert_important: hsla(0.74, 0.85, 0.73, 1.),
        alert_warning: hsla(0.12, 0.7, 0.48, 1.),
        alert_caution: hsla(0.01, 0.9, 0.63, 1.),
        alert_icons: None,
        rule: hsla(0., 0., 1., 0.18),                // `---` divider
        mark_bg: hsla(0.13, 1., 0.5, 0.4),           // yellow <mark> highlight
        popover_bg: hsla(0., 0., 0.16, 1.),          // dark menu surface
        popover_border: hsla(0., 0., 0.28, 1.),      // menu border
        popover_fg: hsla(0., 0., 0.9, 1.),           // menu text
        popover_hover: hsla(0.58, 0.75, 0.66, 0.16), // soft accent tint
        popover_divider: hsla(0., 0., 1., 0.18),     // group divider
        popover_danger: gpui::rgb(0xE5484D).into(),  // destructive rows
        mono: font("Consolas"),
        property_icon: None,
    }
}
