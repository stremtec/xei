mod app;
mod buffer;
mod cursor;
mod editor;
mod ext;
mod syntax;
mod theme;

use gpui::*;

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.open_window(
            WindowOptions::default(),
            |_window, cx| {
                cx.new(|cx| editor::EditorView::new(cx))
            },
        )
        .unwrap();
    });
}
