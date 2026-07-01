use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

pub struct LspClient {
    stdin: Option<ChildStdin>,
    rx: Option<Receiver<LspMessage>>,
    _child: Option<Child>,
    next_id: u64,
    pub diagnostics: Vec<Diagnostic>,
    pub server_running: bool,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub message: String,
    pub severity: DiagnosticSeverity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticSeverity { Error, Warning, Info, Hint }

#[derive(Debug)]
pub enum LspMessage {
    Diagnostics(Vec<Diagnostic>),
    Definition { path: PathBuf, row: usize, col: usize },
    Completions(Vec<CompletionItem>),
    Log(String),
}

#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
}

impl Default for LspClient {
    fn default() -> Self {
        Self { stdin: None, rx: None, _child: None, next_id: 1, diagnostics: Vec::new(), server_running: false }
    }
}

impl LspClient {
    pub fn new() -> Self { Self::default() }

    pub fn start(&mut self, cmd: &str, root: &str, file_path: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() { return; }

        let mut child = match Command::new(parts[0])
            .args(&parts[1..]).current_dir(root)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .spawn()
        { Ok(c) => c, Err(_) => return };

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut header = String::new();
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => return,
                        Ok(_) => {
                            if line == "\r\n" { break; }
                            header.push_str(&line);
                        }
                    }
                }
                let content_len: usize = header.lines()
                    .find_map(|l| l.strip_prefix("Content-Length: "))
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);

                let mut body = vec![0u8; content_len];
                if reader.read_exact(&mut body).is_err() { return; }

                let text = String::from_utf8_lossy(&body);
                if let Some(msg) = parse_message(&text) {
                    if tx.send(msg).is_err() { return; }
                }
            }
        });

        self.stdin = Some(stdin);
        self.rx = Some(rx);
        self._child = Some(child);

        let init = format!(r#"{{"jsonrpc":"2.0","id":{},"method":"initialize","params":{{"processId":null,"rootUri":"file://{}","capabilities":{{}}}}}}"#, self.next_id, root);
        self.next_id += 1;
        self.send_raw(&init);
        self.send_raw(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#);

        let escaped = std::fs::read_to_string(file_path).unwrap_or_default()
            .replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
        let lang = lang_id(file_path);
        let open = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file://{}","languageId":"{}","version":1,"text":"{}"}}}}}}"#, file_path, lang, escaped);
        self.send_raw(&open);

        self.server_running = true;
    }

    pub fn notify_change(&mut self, path: &str, text: &str) {
        if !self.server_running { return; }
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
        let msg = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"file://{}","version":1}},"contentChanges":[{{"text":"{}"}}]}}}}"#, path, escaped);
        self.send_raw(&msg);
    }

    pub fn request_definition(&mut self, path: &str, row: usize, col: usize) {
        if !self.server_running { return; }
        let msg = format!(r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/definition","params":{{"textDocument":{{"uri":"file://{}"}},"position":{{"line":{},"character":{}}}}}}}"#, self.next_id, path, row, col);
        self.next_id += 1;
        self.send_raw(&msg);
    }

    fn send_raw(&mut self, msg: &str) {
        if let Some(ref mut stdin) = self.stdin {
            let header = format!("Content-Length: {}\r\n\r\n", msg.len());
            let _ = stdin.write_all(header.as_bytes());
            let _ = stdin.write_all(msg.as_bytes());
            let _ = stdin.flush();
        }
    }

    pub fn poll(&mut self) {
        if let Some(ref rx) = self.rx {
            loop {
                match rx.try_recv() {
                    Ok(LspMessage::Diagnostics(diags)) => self.diagnostics = diags,
                    Ok(_msg) => {},
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => { self.server_running = false; break; }
                }
            }
        }
    }

    pub fn shutdown(&mut self) {
        self.send_raw(r#"{"jsonrpc":"2.0","method":"shutdown","params":{}}"#);
        self.send_raw(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#);
        self.stdin = None;
        self.rx = None;
        self.server_running = false;
    }
}

fn parse_message(text: &str) -> Option<LspMessage> {
    let method = extract_str(text, "\"method\":\"").map(|s| s.to_string());

    if method.as_deref() == Some("textDocument/publishDiagnostics") {
        let mut diags = Vec::new();
        let items_start = text.find("\"diagnostics\":[");
        if let Some(start) = items_start {
            let rest = &text[start..];
            let items: Vec<&str> = rest.split("\"range\":").skip(1).collect();
            for item in items {
                let row = extract_int(item, "\"line\":").unwrap_or(0) as usize;
                let col_start = extract_int(item, "\"character\":").unwrap_or(0) as usize;
                let col_end = item.split("\"character\":").nth(2)
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.trim().parse().ok()).unwrap_or(col_start + 1);
                let msg = extract_str(item, "\"message\":\"").unwrap_or_default().to_string();
                let severity = match extract_int(item, "\"severity\":") {
                    Some(1) => DiagnosticSeverity::Error,
                    Some(2) => DiagnosticSeverity::Warning,
                    Some(3) => DiagnosticSeverity::Info,
                    _ => DiagnosticSeverity::Hint,
                };
                diags.push(Diagnostic { row, col_start, col_end, message: msg, severity });
            }
        }
        return Some(LspMessage::Diagnostics(diags));
    }

    if method.as_deref() == Some("textDocument/definition") {
        let row = extract_int(text, "\"line\":").unwrap_or(0) as usize;
        let col = extract_int(text, "\"character\":").unwrap_or(0) as usize;
        if let Some(uri) = extract_str(text, "\"uri\":\"") {
            let path = if uri.starts_with("file://") { &uri[7..] } else { &uri };
            return Some(LspMessage::Definition { path: PathBuf::from(path), row, col });
        }
    }

    None
}

fn extract_str<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.find(prefix).and_then(|i| {
        let start = i + prefix.len();
        text[start..].split('"').next()
    })
}

fn extract_int(text: &str, prefix: &str) -> Option<i64> {
    text.find(prefix).and_then(|i| {
        let s = &text[i + prefix.len()..];
        s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()
    })
}

fn lang_id(path: &str) -> &str {
    if path.ends_with(".rs") { "rust" }
    else if path.ends_with(".py") { "python" }
    else if path.ends_with(".ts") || path.ends_with(".tsx") { "typescript" }
    else if path.ends_with(".js") || path.ends_with(".jsx") { "javascript" }
    else if path.ends_with(".go") { "go" }
    else if path.ends_with(".c") || path.ends_with(".h") { "c" }
    else if path.ends_with(".cpp") || path.ends_with(".hpp") { "cpp" }
    else { "plaintext" }
}
