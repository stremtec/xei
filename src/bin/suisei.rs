#[cfg(target_os = "macos")]
#[path = "../app.rs"]
mod app;

#[cfg(target_os = "macos")]
#[path = "../buffer.rs"]
mod buffer;

#[cfg(target_os = "macos")]
#[path = "../config.rs"]
mod config;

#[cfg(target_os = "macos")]
#[path = "../completion.rs"]
mod completion;

#[cfg(target_os = "macos")]
#[path = "../explorer.rs"]
mod explorer;

#[cfg(target_os = "macos")]
#[path = "../highlight.rs"]
mod highlight;

#[cfg(target_os = "macos")]
#[path = "../lsp.rs"]
mod lsp;

#[cfg(target_os = "macos")]
#[path = "../syntax.rs"]
mod syntax;

#[cfg(target_os = "macos")]
#[path = "../term.rs"]
mod term;

#[cfg(target_os = "macos")]
#[path = "../theme.rs"]
mod theme;

#[cfg(target_os = "macos")]
#[path = "../xlc.rs"]
mod xlc;

#[cfg(target_os = "macos")]
#[path = "../gui/mod.rs"]
mod gui;

fn main() {
    #[cfg(target_os = "macos")]
    {
        let args: Vec<String> = std::env::args().collect();
        let file = args.get(1).cloned();
        gui::run_desktop(file);
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("suisei (彗星) is only available on macOS");
        std::process::exit(1);
    }
}
