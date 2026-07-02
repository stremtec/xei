#[cfg(target_os = "macos")]
#[path = "../buffer.rs"]
mod buffer;

#[cfg(target_os = "macos")]
#[path = "../highlight.rs"]
mod highlight;

#[cfg(target_os = "macos")]
#[path = "../syntax.rs"]
mod syntax;

#[cfg(target_os = "macos")]
#[path = "../theme.rs"]
mod theme;

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
