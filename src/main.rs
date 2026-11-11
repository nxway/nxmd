#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod design;
mod diagnostics;
mod theme;
mod ui;
use gpui::*;
fn main() {
    diagnostics::init();
    if std::env::args().any(|a| a == "--test-panic-log") {
        panic!("nxmd diagnostic self-test");
    }
    gpui_platform::application()
        .with_assets(design::Assets)
        .run(|cx| {
            zorite_editor::bind_keys(cx);
            cx.bind_keys([
                KeyBinding::new("escape", ui::DismissMenu, None),
                KeyBinding::new("ctrl-s", ui::Save, None),
                KeyBinding::new("ctrl-w", ui::CloseTab, None),
                KeyBinding::new("ctrl-shift-l", ui::ToggleSidebar, None),
                KeyBinding::new("ctrl-o", ui::OpenVault, None),
                KeyBinding::new("ctrl-n", ui::NewNote, None),
            ]);
            let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("nxmd".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |w, cx| cx.new(|cx| ui::Workspace::new(w, cx)),
            )
            .expect("Cannot open native window");
            cx.activate(true);
        });
}
