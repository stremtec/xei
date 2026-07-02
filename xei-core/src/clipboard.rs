use std::io::Write;
use std::process::{Command, Stdio};

pub fn copy(text: &str) {
    if let Ok(mut child) = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(ref mut stdin) = child.stdin {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

pub fn paste() -> Option<String> {
    Command::new("pbpaste")
        .stdout(Stdio::piped())
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}
