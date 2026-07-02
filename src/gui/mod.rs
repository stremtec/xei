pub mod editor;

use gpui::*;

pub fn run_desktop(file_path: Option<String>) {
    Application::new().run(move |cx: &mut App| {
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("suisei (彗星)".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None, size(px(1200.), px(800.)), cx,
                ))),
                ..Default::default()
            },
            |_window, cx| {
                cx.new(|cx| editor::Suisei::new(cx, file_path))
            },
        )
        .unwrap();
    });
}
