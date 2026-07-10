//! System clipboard access with multi-backend fallbacks.
//!
//! Order for **copy**:
//! 1. macOS `pbcopy`
//! 2. Wayland `wl-copy`
//! 3. X11 `xclip` / `xsel`
//! 4. OSC 52 (terminal clipboard — works in many modern terminals)
//!
//! Order for **paste**:
//! 1. `pbpaste` / `wl-paste` / `xclip -o` / `xsel -o`

use std::io::Write;
use std::process::{Command, Stdio};

/// Copy `text` to the OS clipboard. Returns whether at least one backend succeeded.
pub fn copy(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }

    let mut ok = false;

    // macOS
    if pipe_to(&["pbcopy"], text) {
        ok = true;
    }

    // Wayland
    if !ok && pipe_to(&["wl-copy"], text) {
        ok = true;
    }

    // X11
    if !ok && pipe_to(&["xclip", "-selection", "clipboard"], text) {
        ok = true;
    }
    if !ok && pipe_to(&["xsel", "--clipboard", "--input"], text) {
        ok = true;
    }

    // Always try OSC 52 as well (helps when running inside tmux/ssh/kitty/wezterm)
    // Some terminals need this even when pbcopy works from a different process context.
    osc52_copy(text);

    ok
}

/// Read from the OS clipboard.
pub fn paste() -> Option<String> {
    if let Some(s) = run_stdout(&["pbpaste"]) {
        return Some(s);
    }
    if let Some(s) = run_stdout(&["wl-paste", "-n"]) {
        return Some(s);
    }
    if let Some(s) = run_stdout(&["xclip", "-selection", "clipboard", "-o"]) {
        return Some(s);
    }
    if let Some(s) = run_stdout(&["xsel", "--clipboard", "--output"]) {
        return Some(s);
    }
    None
}

/// Whether a clipboard tool appears available (best-effort).
pub fn available() -> bool {
    which("pbcopy")
        || which("pbpaste")
        || which("wl-copy")
        || which("wl-paste")
        || which("xclip")
        || which("xsel")
}

fn which(bin: &str) -> bool {
    Command::new("which")
        .arg(bin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn pipe_to(cmd: &[&str], text: &str) -> bool {
    if cmd.is_empty() {
        return false;
    }
    let mut child = match Command::new(cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Drop stdin so the tool sees EOF (critical for pbcopy).
    {
        let Some(mut stdin) = child.stdin.take() else {
            let _ = child.kill();
            return false;
        };
        if stdin.write_all(text.as_bytes()).is_err() {
            let _ = child.kill();
            return false;
        }
        // explicit flush + drop
        let _ = stdin.flush();
    }

    match child.wait() {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

fn run_stdout(cmd: &[&str]) -> Option<String> {
    if cmd.is_empty() {
        return None;
    }
    let output = Command::new(cmd[0])
        .args(&cmd[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).into_owned();
    Some(s)
}

/// OSC 52 clipboard write. Base64-encode payload.
/// Format: ESC ] 52 ; c ; <base64> BEL  (or ST)
fn osc52_copy(text: &str) {
    let b64 = base64_encode(text.as_bytes());
    // Prefer BEL terminator; many terminals accept both.
    let seq = format!("\x1b]52;c;{}\x07", b64);
    let _ = std::io::stdout().write_all(seq.as_bytes());
    let _ = std::io::stdout().flush();
}

/// Minimal base64 (no external crate).
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    match data.len() - i {
        1 => {
            let n = (data[i] as u32) << 16;
            out.push(T[((n >> 18) & 63) as usize] as char);
            out.push(T[((n >> 12) & 63) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
            out.push(T[((n >> 18) & 63) as usize] as char);
            out.push(T[((n >> 12) & 63) as usize] as char);
            out.push(T[((n >> 6) & 63) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_hello() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn base64_empty() {
        assert_eq!(base64_encode(b""), "");
    }
}
