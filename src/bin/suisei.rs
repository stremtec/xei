#[path = "../buffer.rs"]
mod buffer;
#[path = "../highlight.rs"]
mod highlight;
#[path = "../syntax.rs"]
mod syntax;
#[path = "../theme.rs"]
mod theme;
#[path = "../gui/mod.rs"]
mod gui;

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let file = args.get(1).cloned();
    gui::run_desktop(file);
}
