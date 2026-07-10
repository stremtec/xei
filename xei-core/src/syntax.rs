//! Tree-sitter syntax highlighting via **highlight queries** (`highlights.scm`).
//!
//! Tree-sitter columns are **byte** offsets; the editor uses **char** indices.
//! Query captures map to [`TokenKind`] through [`crate::highlight::from_capture`].

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};

use crate::highlight::{self, TokenKind};

/// One highlight span: (kind, start_col, end_col, row) — char columns, end exclusive.
pub type HlToken = (TokenKind, usize, usize, usize);

struct LangBundle {
    parser: Parser,
    query: Query,
}

pub struct SyntaxEngine {
    rust: Option<LangBundle>,
    python: Option<LangBundle>,
    javascript: Option<LangBundle>,
    typescript: Option<LangBundle>,
    tsx: Option<LangBundle>,
    c: Option<LangBundle>,
    go: Option<LangBundle>,
    bash: Option<LangBundle>,
    json: Option<LangBundle>,
    tree: Option<Tree>,
    last_ext: String,
    last_len: usize,
    last_fingerprint: u64,
    /// Query-based tokens (char columns)
    pub tokens: Vec<HlToken>,
    pub active: bool,
}

impl Default for SyntaxEngine {
    fn default() -> Self {
        Self {
            rust: make_lang(
                tree_sitter_rust::LANGUAGE.into(),
                tree_sitter_rust::HIGHLIGHTS_QUERY,
            ),
            python: make_lang(
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::HIGHLIGHTS_QUERY,
            ),
            javascript: make_lang(
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::HIGHLIGHT_QUERY,
            ),
            typescript: make_lang(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ),
            tsx: make_lang(
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                // TSX uses the same highlights as TS + JSX patterns when available
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ),
            c: make_lang(
                tree_sitter_c::LANGUAGE.into(),
                tree_sitter_c::HIGHLIGHT_QUERY,
            ),
            go: make_lang(tree_sitter_go::LANGUAGE.into(), tree_sitter_go::HIGHLIGHTS_QUERY),
            bash: make_lang(
                tree_sitter_bash::LANGUAGE.into(),
                tree_sitter_bash::HIGHLIGHT_QUERY,
            ),
            json: make_lang(
                tree_sitter_json::LANGUAGE.into(),
                tree_sitter_json::HIGHLIGHTS_QUERY,
            ),
            tree: None,
            last_ext: String::new(),
            last_len: 0,
            last_fingerprint: 0,
            tokens: Vec::new(),
            active: false,
        }
    }
}

fn make_lang(language: Language, source: &str) -> Option<LangBundle> {
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return None;
    }
    let query = Query::new(&language, source).ok()?;
    Some(LangBundle { parser, query })
}

impl SyntaxEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse(&mut self, text: &str, ext: Option<&str>) {
        let ext_str = ext.unwrap_or("");
        let fingerprint = fingerprint_text(text);

        // Skip full work when content unchanged
        if self.active
            && self.last_ext == ext_str
            && self.last_len == text.len()
            && self.last_fingerprint == fingerprint
            && !self.tokens.is_empty()
        {
            return;
        }

        let bundle = match ext {
            Some("rs") => self.rust.as_mut(),
            Some("py" | "pyi") => self.python.as_mut(),
            Some("js" | "mjs" | "cjs") => self.javascript.as_mut(),
            Some("jsx") => self.javascript.as_mut(),
            Some("ts" | "mts" | "cts") => self.typescript.as_mut(),
            Some("tsx") => self.tsx.as_mut().or(self.typescript.as_mut()),
            Some("c" | "h") => self.c.as_mut(),
            Some("cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx") => self.c.as_mut(),
            Some("go") => self.go.as_mut(),
            Some("sh" | "bash" | "zsh") => self.bash.as_mut(),
            Some("json" | "jsonc") => self.json.as_mut(),
            _ => {
                self.tokens.clear();
                self.tree = None;
                self.last_ext.clear();
                self.last_len = 0;
                self.last_fingerprint = 0;
                self.active = false;
                return;
            }
        };

        let Some(bundle) = bundle else {
            self.tokens.clear();
            self.active = false;
            return;
        };

        self.active = true;
        let len = text.len();

        // IMPORTANT: never pass a stale Tree without `tree.edit(...)`.
        // Incremental parse without edit panics inside tree-sitter.
        // Drop any previous tree and full-reparse; wrap ALL ts calls in
        // catch_unwind so a binding panic never kills the editor process.
        self.tree = None;
        self.tokens.clear();
        self.last_ext = ext_str.to_string();
        self.last_len = len;
        self.last_fingerprint = fingerprint;

        let source = text.as_bytes();
        let lines: Vec<&str> = text.split('\n').collect();
        let capture_names = bundle.query.capture_names().to_vec();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let Some(tree) = bundle.parser.parse(text, None) else {
                return None;
            };
            let root = tree.root_node();
            let mut cursor = QueryCursor::new();
            let mut tokens: Vec<HlToken> = Vec::new();
            let mut matches = cursor.matches(&bundle.query, root, source);
            while let Some(m) = matches.next() {
                for cap in m.captures {
                    let name = capture_names
                        .get(cap.index as usize)
                        .copied()
                        .unwrap_or("");
                    let Some(kind) = highlight::from_capture(name) else {
                        continue;
                    };
                    let node = cap.node;
                    let end_byte = node.end_byte();
                    if end_byte > source.len() || node.start_byte() > end_byte {
                        continue;
                    }
                    push_node_tokens(node, &lines, kind, &mut tokens);
                }
            }
            tokens.sort_by_key(|(_, st, ed, row)| (*row, ed.saturating_sub(*st), *st));
            Some((tree, tokens))
        }));

        match result {
            Ok(Some((tree, tokens))) => {
                self.tree = Some(tree);
                self.tokens = tokens;
            }
            Ok(None) => {
                self.tree = None;
                self.tokens.clear();
            }
            Err(_) => {
                // tree-sitter panicked — stay alive with no highlight
                self.tree = None;
                self.tokens.clear();
                self.active = false;
            }
        }
    }
}

fn push_node_tokens(
    node: tree_sitter::Node,
    lines: &[&str],
    kind: TokenKind,
    tokens: &mut Vec<HlToken>,
) {
    let start = node.start_position();
    let end = node.end_position();

    // Safety: skip huge multi-line non-comment/string spans
    if start.row != end.row && !matches!(kind, TokenKind::Comment | TokenKind::String) {
        return;
    }

    if start.row == end.row {
        if let Some(line) = lines.get(start.row) {
            let scol = byte_col_to_char_col(line, start.column);
            let ecol = byte_col_to_char_col(line, end.column);
            if scol < ecol {
                tokens.push((kind, scol, ecol, start.row));
            }
        }
        return;
    }

    // Multi-line comments / strings
    if let Some(line) = lines.get(start.row) {
        let scol = byte_col_to_char_col(line, start.column);
        let ecol = line.chars().count();
        if scol < ecol {
            tokens.push((kind, scol, ecol, start.row));
        }
    }
    for row in start.row + 1..end.row {
        if let Some(line) = lines.get(row) {
            let ecol = line.chars().count();
            if ecol > 0 {
                tokens.push((kind, 0, ecol, row));
            }
        }
    }
    if let Some(line) = lines.get(end.row) {
        let ecol = byte_col_to_char_col(line, end.column);
        if ecol > 0 {
            tokens.push((kind, 0, ecol, end.row));
        }
    }
}

fn byte_col_to_char_col(line: &str, byte_col: usize) -> usize {
    if byte_col == 0 {
        return 0;
    }
    if byte_col >= line.len() {
        return line.chars().count();
    }
    let mut idx = byte_col;
    while idx > 0 && !line.is_char_boundary(idx) {
        idx -= 1;
    }
    line.get(..idx).map(|s| s.chars().count()).unwrap_or(0)
}

/// Full-content FNV-1a fingerprint (skip re-query only when bytes are identical).
fn fingerprint_text(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in text.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    // Mix length so empty vs non-empty always differ
    for b in text.len().to_le_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_to_char_ascii() {
        assert_eq!(byte_col_to_char_col("hello", 0), 0);
        assert_eq!(byte_col_to_char_col("hello", 5), 5);
    }

    #[test]
    fn byte_to_char_cjk() {
        let line = "a한b";
        assert_eq!(byte_col_to_char_col(line, 0), 0);
        assert_eq!(byte_col_to_char_col(line, 1), 1);
        assert_eq!(byte_col_to_char_col(line, 4), 2);
    }

    #[test]
    fn parse_rust_produces_tokens() {
        let mut eng = SyntaxEngine::new();
        eng.parse("fn main() { let x = 1; }", Some("rs"));
        assert!(eng.active);
        assert!(!eng.tokens.is_empty(), "expected query tokens");
    }

    #[test]
    fn rust_does_not_paint_whole_function_as_one_token() {
        let mut eng = SyntaxEngine::new();
        let src = "fn main() {\n    let x = 42;\n    let s = \"hi\";\n}\n";
        eng.parse(src, Some("rs"));
        let first_line_len = src.lines().next().unwrap().chars().count();
        let paints_whole_line = eng.tokens.iter().any(|(k, st, ed, row)| {
            *row == 0 && *st == 0 && *ed >= first_line_len && matches!(k, TokenKind::Keyword)
        });
        assert!(
            !paints_whole_line,
            "keyword token painted entire first line: {:?}",
            eng.tokens
        );
        let has_number = eng
            .tokens
            .iter()
            .any(|(k, _, _, _)| matches!(k, TokenKind::Number));
        let has_string = eng
            .tokens
            .iter()
            .any(|(k, _, _, _)| matches!(k, TokenKind::String));
        assert!(
            has_number || has_string,
            "expected number/string tokens, got {:?}",
            eng.tokens
        );
    }

    #[test]
    fn rust_highlights_fn_and_function_name() {
        let mut eng = SyntaxEngine::new();
        eng.parse("fn main() {}", Some("rs"));
        assert!(eng.active);
        assert!(!eng.tokens.is_empty());
    }

    #[test]
    fn python_query_active() {
        let mut eng = SyntaxEngine::new();
        eng.parse("def foo(x):\n    return x + 1\n", Some("py"));
        assert!(eng.active);
        assert!(!eng.tokens.is_empty());
    }

    #[test]
    fn go_and_json_query_active() {
        let mut eng = SyntaxEngine::new();
        eng.parse("package main\nfunc Hello() {}\n", Some("go"));
        assert!(eng.active);
        assert!(!eng.tokens.is_empty());
        eng.parse(r#"{"a": 1, "b": "x"}"#, Some("json"));
        assert!(eng.active);
        assert!(!eng.tokens.is_empty());
    }

    #[test]
    fn skip_reparse_when_unchanged() {
        let mut eng = SyntaxEngine::new();
        eng.parse("fn a() {}", Some("rs"));
        let n = eng.tokens.len();
        eng.parse("fn a() {}", Some("rs"));
        assert_eq!(eng.tokens.len(), n);
    }

    #[test]
    fn rapid_edits_do_not_panic() {
        // Regression: incremental parse without tree.edit() panicked with
        // "range start index N out of range for slice of length M".
        let mut eng = SyntaxEngine::new();
        let mut src = String::from("fn main() {\n    let x = 1;\n}\n");
        eng.parse(&src, Some("rs"));
        for i in 0..80 {
            src.insert(src.len().saturating_sub(2), char::from(b'a' + (i % 26) as u8));
            eng.parse(&src, Some("rs"));
            // delete a char near the middle
            if src.len() > 10 {
                let mid = src.len() / 2;
                if src.is_char_boundary(mid) {
                    src.remove(mid);
                }
                eng.parse(&src, Some("rs"));
            }
        }
        assert!(eng.active || eng.tokens.is_empty());
    }

    #[test]
    fn switch_language_and_edit() {
        let mut eng = SyntaxEngine::new();
        eng.parse("fn foo() {}", Some("rs"));
        eng.parse("def foo():\n  pass\n", Some("py"));
        eng.parse("fn bar() { let y = 2; }", Some("rs"));
        assert!(eng.tokens.iter().any(|t| t.0 == TokenKind::Keyword || t.0 == TokenKind::Function));
    }
}
