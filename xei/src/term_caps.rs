//! Terminal capability detection for GPU-accelerated emulators
//! (Ghostty, Kitty, WezTerm, iTerm2, …).

#![allow(dead_code)] // query / hyperlink helpers used progressively

use std::env;
use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal;

/// Feature flags discovered (or inferred) from the host terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCaps {
    /// DEC mode 2026 — tear-free full redraws.
    pub sync_output: bool,
    /// CSI 4:3 undercurl (or equivalent).
    pub undercurl: bool,
    /// Colored underlines (SGR 58).
    pub underline_color: bool,
    /// OSC 8 hyperlinks.
    pub hyperlinks: bool,
    /// Kitty graphics protocol (Phase B).
    pub kitty_graphics: bool,
    /// Likely a GPU / modern terminal.
    pub modern: bool,
    /// Human-readable identity, e.g. "ghostty", "kitty".
    pub name: &'static str,
}

impl Default for TerminalCaps {
    fn default() -> Self {
        Self {
            sync_output: false,
            undercurl: false,
            underline_color: false,
            hyperlinks: false,
            kitty_graphics: false,
            modern: false,
            name: "unknown",
        }
    }
}

impl TerminalCaps {
    /// Detect from environment (+ light heuristics). Safe without queries.
    pub fn detect() -> Self {
        let term = env::var("TERM").unwrap_or_default().to_ascii_lowercase();
        let program = env::var("TERM_PROGRAM")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let term_program_version = env::var("TERM_PROGRAM_VERSION").unwrap_or_default();
        let _ = term_program_version;

        let ghostty = program.contains("ghostty")
            || env::var("GHOSTTY_RESOURCES_DIR").is_ok()
            || env::var("GHOSTTY_SHELL_INTEGRATION_NO_SUDO").is_ok();
        let kitty = program.contains("kitty")
            || env::var("KITTY_WINDOW_ID").is_ok()
            || env::var("KITTY_PID").is_ok()
            || term.contains("kitty");
        let wez = program.contains("wezterm") || term.contains("wezterm");
        let iterm = program.contains("iterm") || env::var("ITERM_SESSION_ID").is_ok();
        let foot = term.contains("foot") || program.contains("foot");
        let alacritty = program.contains("alacritty") || term.contains("alacritty");
        let windows_terminal = program.contains("vscode") // often false
            || env::var("WT_SESSION").is_ok();

        let (name, modern) = if ghostty {
            ("ghostty", true)
        } else if kitty {
            ("kitty", true)
        } else if wez {
            ("wezterm", true)
        } else if iterm {
            ("iterm2", true)
        } else if foot {
            ("foot", true)
        } else if alacritty {
            ("alacritty", true)
        } else if windows_terminal {
            ("windows-terminal", true)
        } else if term.contains("xterm-256") || term.contains("xterm-ghostty") {
            ("xterm-256color", false)
        } else {
            ("generic", false)
        };

        // Capability matrix — conservative defaults for known modern emulators.
        let mut caps = Self {
            name,
            modern,
            sync_output: modern || ghostty || kitty || wez || foot,
            undercurl: ghostty || kitty || wez || iterm || foot,
            underline_color: modern || ghostty || kitty || wez || iterm || foot || alacritty,
            hyperlinks: ghostty || kitty || wez || iterm || foot,
            kitty_graphics: kitty || ghostty || wez, // WezTerm speaks Kitty graphics
            ..Self::default()
        };

        // TERM hints
        if term.contains("direct") || term.contains("truecolor") {
            caps.underline_color = true;
        }

        caps
    }

    /// Optional active query for DEC 2026 support. Best-effort; may no-op.
    /// Must be called after raw mode is enabled, with stdin available.
    pub fn query_sync_support(&mut self) {
        if self.sync_output {
            // Already assumed; still try to confirm on modern hosts.
        }
        // CSI ? 2026 $ p  →  CSI ? 2026 ; Ps $ y
        let mut stdout = io::stdout();
        let _ = write!(stdout, "\x1b[?2026$p");
        let _ = stdout.flush();

        // Brief poll for DECRPM reply
        let deadline = Duration::from_millis(40);
        let start = std::time::Instant::now();
        while start.elapsed() < deadline {
            if event::poll(Duration::from_millis(5)).unwrap_or(false) {
                if let Ok(Event::Key(k)) = event::read() {
                    // Ignore — DECRPM often arrives as unknown / ignored bytes.
                    // crossterm may not surface CSI replies as Key events.
                    let _ = k;
                }
            } else {
                break;
            }
        }
        // Keep heuristic result; queries are unreliable across multiplexers.
        let _ = terminal::is_raw_mode_enabled();
    }

    /// Short status for About / statusline.
    pub fn summary(self) -> String {
        if !self.modern && self.name == "generic" {
            return "term: basic".into();
        }
        let mut flags = Vec::new();
        if self.sync_output {
            flags.push("sync");
        }
        if self.undercurl {
            flags.push("curl");
        }
        if self.underline_color {
            flags.push("ul");
        }
        if self.hyperlinks {
            flags.push("link");
        }
        if self.kitty_graphics {
            flags.push("gfx");
        }
        if flags.is_empty() {
            format!("term: {}", self.name)
        } else {
            format!("term: {} [{}]", self.name, flags.join("+"))
        }
    }
}

/// OSC 8 hyperlink open. Pair with [`hyperlink_end`].
pub fn hyperlink_open(url: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\")
}

/// OSC 8 hyperlink close.
pub fn hyperlink_end() -> &'static str {
    "\x1b]8;;\x1b\\"
}

/// Build a file:// URL for a local path (best-effort absolute).
pub fn file_url(path: &str) -> String {
    let p = std::path::Path::new(path);
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        env::current_dir().unwrap_or_default().join(p)
    };
    // Spaces etc.
    let s = abs.display().to_string();
    let encoded = s.replace(' ', "%20");
    format!("file://{encoded}")
}

/// Whether we should use GPU progressive features given user toggle + caps.
pub fn gpu_features_active(gpu_acc: bool, caps: &TerminalCaps) -> bool {
    gpu_acc && (caps.modern || caps.sync_output || caps.underline_color)
}

/// Drain any leftover query responses so they don't leak into the first key.
pub fn drain_input_noise() {
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(30) {
        match event::poll(Duration::from_millis(2)) {
            Ok(true) => {
                let _ = event::read();
            }
            _ => break,
        }
    }
    // Silence unused import warning path for KeyCode in some builds
    let _ = KeyCode::Esc;
    let _ = KeyEventKind::Press;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_never_panics() {
        let c = TerminalCaps::detect();
        assert!(!c.name.is_empty());
    }

    #[test]
    fn file_url_abs() {
        let u = file_url("/tmp/foo bar.rs");
        assert!(u.starts_with("file://"));
        assert!(u.contains("%20") || u.contains("foo"));
    }

    #[test]
    fn gpu_gate() {
        let mut caps = TerminalCaps::default();
        assert!(!gpu_features_active(true, &caps));
        caps.modern = true;
        assert!(gpu_features_active(true, &caps));
        assert!(!gpu_features_active(false, &caps));
    }
}
