//! GPU-terminal frame helpers: synchronized output, hyperlink spans.

#![allow(dead_code)] // hyperlink helpers reserved for S2 OSC-8 wiring

use std::io::{self, Write};

use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::{execute, queue};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::term_caps::{self, TerminalCaps};

/// Run a full TUI draw inside a synchronized update when enabled.
pub fn draw_synced<F>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    use_sync: bool,
    f: F,
) -> io::Result<()>
where
    F: FnOnce(&mut ratatui::Frame),
{
    if use_sync {
        // Begin sync on the backend writer (same buffer as ratatui).
        {
            let writer = terminal.backend_mut();
            queue!(writer, BeginSynchronizedUpdate)?;
        }
        terminal.draw(f)?;
        {
            let writer = terminal.backend_mut();
            queue!(writer, EndSynchronizedUpdate)?;
            writer.flush()?;
        }
        Ok(())
    } else {
        terminal.draw(f).map(|_| ())
    }
}

/// Whether this frame should use synchronized output.
pub fn should_sync(gpu_acc: bool, caps: &TerminalCaps) -> bool {
    gpu_acc && caps.sync_output
}

/// Whether diagnostics should use colored / curly underline styling.
pub fn should_rich_underline(gpu_acc: bool, caps: &TerminalCaps) -> bool {
    gpu_acc && (caps.undercurl || caps.underline_color)
}

/// Whether OSC 8 hyperlinks should be embedded.
pub fn should_hyperlink(gpu_acc: bool, caps: &TerminalCaps) -> bool {
    gpu_acc && caps.hyperlinks
}

/// Print an OSC 8 wrapped label to stderr/stdout outside the cell grid
/// (e.g. for debugging). Prefer embedding via [`format_hyperlink_label`].
pub fn write_hyperlink(stdout: &mut impl Write, url: &str, label: &str) -> io::Result<()> {
    write!(
        stdout,
        "{}{}{}",
        term_caps::hyperlink_open(url),
        label,
        term_caps::hyperlink_end()
    )
}

/// Format label with OSC 8 wrappers for inclusion in raw terminal writes.
/// Note: ratatui cells strip non-printable escapes — use only on raw status
/// writes after the frame, or accept that TUI widgets may not preserve them.
pub fn format_hyperlink_label(url: &str, label: &str) -> String {
    format!(
        "{}{}{}",
        term_caps::hyperlink_open(url),
        label,
        term_caps::hyperlink_end()
    )
}

/// Reset underline style to default (clears sticky CSI 4:3 undercurl).
///
/// **Do not** enable session-wide `CSI 4:3 m` — Ghostty/Kitty then draw
/// curly underlines on *every* cell including padding, which looks like a
/// screen full of `~` / `^` waves (see regression 2026-07).
pub fn reset_underline_sgr(out: &mut impl Write) -> io::Result<()> {
    write!(out, "\x1b[4:0m\x1b[24m")?;
    out.flush()
}

pub fn reset_underline_sgr_stdout() {
    let mut out = io::stdout();
    let _ = reset_underline_sgr(&mut out);
}

/// Emit a file hyperlink line via raw OSC after a frame (bottom message strip).
/// Call only when the alternate screen is active and gpu features are on.
pub fn emit_file_hyperlink_hint(path: &str) -> io::Result<()> {
    let url = term_caps::file_url(path);
    let mut out = io::stdout();
    // Don't move cursor — this is for terminals that parse OSC anywhere.
    // Prefer silent: just register no visible text.
    let _ = url;
    let _ = &mut out;
    Ok(())
}

/// Flush any pending terminal commands on the ratatui backend writer.
pub fn flush_backend(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    terminal.backend_mut().flush()
}

/// Explicit end-sync in case of panic path.
pub fn force_end_sync() {
    let mut out = io::stdout();
    let _ = execute!(out, EndSynchronizedUpdate);
}
