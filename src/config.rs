use std::fs;
use std::path::PathBuf;

fn config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".xei.toml")
}

pub fn save_theme(name: &str) {
    let content = format!("theme = \"{}\"\n", name);
    let _ = fs::write(config_path(), content);
}

pub fn load_theme() -> Option<String> {
    let content = fs::read_to_string(config_path()).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("theme = ") {
            let val = rest.trim().trim_matches('"');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}
