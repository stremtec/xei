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
    pub server_name: String,
    pub server_lang: String,
    pub initialized: bool,
    pending_didopen: Option<(String, String, String)>,
    pub pending_definition: Option<Location>,
    pub pending_completions: Vec<CompletionItem>,
    pub error: Option<String>,
    current_uri: String,
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
    Diagnostics(Vec<Diagnostic>, String),
    Definition { path: PathBuf, row: usize, col: usize },
    Completions(Vec<CompletionItem>),
    InitResponse,
    Log(String),
}

#[derive(Debug, Clone)]
pub struct Location {
    pub path: String,
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
}

impl Default for LspClient {
    fn default() -> Self {
        Self { stdin: None, rx: None, _child: None, next_id: 1, diagnostics: Vec::new(), server_running: false, server_name: String::new(), server_lang: String::new(), initialized: false, pending_didopen: None, pending_definition: None, pending_completions: Vec::new(), error: None, current_uri: String::new() }
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
        { Ok(c) => c, Err(e) => {
            self.error = Some(format!("LSP failed to start: {}", e));
            return;
        }};

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
        self.server_name = parts[0].to_string();
        self.current_uri = file_uri(file_path);

        let init = format!(r#"{{"jsonrpc":"2.0","id":{},"method":"initialize","params":{{"processId":null,"rootUri":"{}","capabilities":{{}}}}}}"#, self.next_id, file_uri(root));
        self.next_id += 1;
        self.send_raw(&init);

        let escaped = escape_json(&std::fs::read_to_string(file_path).unwrap_or_default());
        let lang = lang_id(file_path);
        self.pending_didopen = Some((file_path.to_string(), lang.to_string(), escaped));

        // server_running = true after init response, not here
    }

    pub fn notify_change(&mut self, path: &str, text: &str) {
        if !self.server_running { return; }
        let escaped = escape_json(text);
        let msg = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":1}},"contentChanges":[{{"text":"{}"}}]}}}}"#, file_uri(path), escaped);
        self.send_raw(&msg);
    }

    pub fn request_definition(&mut self, path: &str, row: usize, col: usize) {
        if !self.server_running { return; }
        let msg = format!(r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}}}}}"#,
            self.next_id, file_uri(path), row, col);
        self.next_id += 1;
        self.send_raw(&msg);
    }

    pub fn auto_start(&mut self, file_path: &str) {
        if self.server_running { return; }
        let ext = std::path::Path::new(file_path).extension().and_then(|e| e.to_str()).unwrap_or("");
        let cmd = match ext {
            "rs" => "rust-analyzer",
            "py" => "pyright-langserver --stdio",
            "ts" | "tsx" => "typescript-language-server --stdio",
            "js" | "jsx" | "mjs" | "cjs" => "typescript-language-server --stdio",
            "c" | "h" | "cpp" | "hpp" | "cc" | "cxx" => "clangd",
            "go" => "gopls",
            "html" | "htm" => "vscode-html-language-server --stdio",
            "css" | "scss" | "less" => "vscode-css-language-server --stdio",
            "json" => "vscode-json-language-server --stdio",
            "yaml" | "yml" => "yaml-language-server --stdio",
            "sh" | "bash" | "zsh" => "bash-language-server start",
            "rb" => "solargraph stdio",
            "md" | "mdx" => "marksman server",
            "toml" => "taplo lsp stdio",
            _ => return,
        };
        let root = std::path::Path::new(file_path).parent()
            .map(|p| p.display().to_string()).unwrap_or_default();
        self.start(cmd, &root, file_path);
        if !self.server_running {
            self.error = Some(format!("{} not found. Install it or set manually with :LspStart", cmd));
        }
    }

    pub fn request_completion(&mut self, path: &str, row: usize, col: usize) {
        if !self.server_running { return; }
        let msg = format!(r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/completion","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}}}}}"#, self.next_id, file_uri(path), row, col);
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
                    Ok(LspMessage::Diagnostics(diags, uri)) => {
                        if uri.is_empty() || uri == self.current_uri {
                            if !diags.is_empty() {
                                self.diagnostics = diags;
                            }
                        }
                    }
                    Ok(LspMessage::InitResponse) => {
                        if !self.initialized {
                            self.initialized = true;
                            self.server_running = true;
                        }
                    }
                    Ok(LspMessage::Definition { path, row, col }) => {
                        self.pending_definition = Some(Location { path: path.display().to_string(), row, col });
                    }
                    Ok(LspMessage::Completions(items)) => {
                        self.pending_completions = items;
                    }
                    Ok(_msg) => {},
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => { self.server_running = false; break; }
                }
            }
        }
        if self.initialized {
            if let Some((path, lang, text)) = self.pending_didopen.take() {
                self.send_raw(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#);
                let msg = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"{}","version":1,"text":"{}"}}}}}}"#, file_uri(&path), lang, text);
                self.send_raw(&msg);
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

    if text.contains("\"result\"") && text.contains("\"capabilities\"") {
        return Some(LspMessage::InitResponse);
    }

    if method.as_deref() == Some("textDocument/publishDiagnostics") {
        let uri = extract_str(text, "\"uri\":\"").unwrap_or("").to_string();
        let mut diags = Vec::new();
        let items_start = text.find("\"diagnostics\":[");
        if let Some(start) = items_start {
            let rest = &text[start..];
            let items: Vec<&str> = rest.split("\"range\":").skip(1).collect();
            for item in items {
                let row = extract_int(item, "\"line\":").unwrap_or(0) as usize;
                let col_start = extract_int(item, "\"character\":").unwrap_or(0) as usize;
                let col_end = item.split("\"character\":").nth(2)
                    .and_then(|s| s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok())
                    .unwrap_or(col_start + 1);
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
        return Some(LspMessage::Diagnostics(diags, uri));
    }

    if method.as_deref() == Some("textDocument/definition") {
        let row = extract_int(text, "\"line\":").unwrap_or(0) as usize;
        let col = extract_int(text, "\"character\":").unwrap_or(0) as usize;
        if let Some(uri) = extract_str(text, "\"uri\":\"") {
            let path = if uri.starts_with("file://") { &uri[7..] } else { uri };
            return Some(LspMessage::Definition { path: PathBuf::from(path), row, col });
        }
    }

    if text.contains("\"completion\"") || text.contains("\"label\""){
        let mut items = Vec::new();
        for chunk in text.split("\"label\":\"").skip(1) {
            let label = chunk.split('"').next().unwrap_or("").to_string();
            let detail = extract_str(chunk, "\"detail\":\"").map(|s| s.to_string());
            items.push(CompletionItem { label, detail });
        }
        return Some(LspMessage::Completions(items));
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

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn file_uri(path: &str) -> String {
    let encoded = path.replace('\\', "/").replace(' ', "%20");
    format!("file://{}", encoded)
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
