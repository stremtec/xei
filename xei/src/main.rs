use std::env;
use std::io::{self, Write};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};

use xei_core::App;

mod event;
mod ui;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-V" => {
                println!("xei {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("xei (晴) — a modern Vim-like terminal editor\n");
                println!("Usage: xei [FILE]       Open a file for editing");
                println!("       xei --version     Print version");
                println!("       xei --help        Show this help");
                println!("       xei --debug FILE  Open with debug logging\n");
                println!("Homepage: https://github.com/stremtec/xei");
                return Ok(());
            }
            _ => {}
        }
    }

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        prev_hook(info);
    }));

    let mut app = if args.len() > 1 && !args[1].starts_with('-') {
        App::open_file(&args[1])
    } else if args.len() > 2 && args[1] == "--debug" {
        let mut a = App::open_file(&args[2]);
        a.debug = true;
        a
    } else {
        App::new()
    };

    if let Some(saved_theme) = xei_core::config::load_theme() {
        if let Some(t) = xei_core::theme::find(&saved_theme) {
            app.theme = t;
        }
    }

    xei_core::set_cursor_esc(app.theme.cursor);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    let _ = std::panic::take_hook();
    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    while app.running {
        // DEC 2026 synchronized updates — eliminate screen tearing
        let _ = write!(io::stdout(), "\x1b[?2026h");
        terminal.draw(|f| ui::draw(f, app))?;
        let _ = write!(io::stdout(), "\x1b[?2026l");
        if !event::handle_events(app)? {
            break;
        }
        app.lsp.poll();
        let lsp_comps: Vec<_> = std::mem::take(&mut app.lsp.pending_completions);
        if !lsp_comps.is_empty() && app.completions.active {
            for item in lsp_comps {
                let exists = app.completions.suggestions.iter().any(|s| s.label == item.label);
                if !exists {
                    app.completions.suggestions.push(xei_core::completion::Suggestion {
                        label: item.label.clone(),
                        detail: item.detail.unwrap_or_else(|| "LSP".to_string()),
                        insert_text: item.label,
                    });
                }
            }
        }
        if let Some(loc) = app.lsp.pending_definition.take() {
            let path_str = loc.path.clone();
            app.open_new_tab(&path_str);
            app.buffer.cursor.row = loc.row;
            app.buffer.cursor.col = loc.col;
            app.message = format!("Jumped to definition: {}:{}", path_str, loc.row + 1);
        }
        app.check_external_change();
    }
    Ok(())
}
