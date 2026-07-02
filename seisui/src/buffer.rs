use ropey::Rope;
use std::path::PathBuf;

#[derive(Clone, PartialEq)]
pub enum Language {
    Rust, Python, JavaScript, TypeScript, C, Cpp, Go,
    Html, Css, Json, Yaml, Toml, Markdown, PlainText,
}

impl Language {
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "rs" => Language::Rust,
            "py" => Language::Python,
            "js" | "mjs" | "cjs" => Language::JavaScript,
            "ts" => Language::TypeScript,
            "c" | "h" => Language::C,
            "cpp" | "hpp" | "cc" | "cxx" | "hxx" => Language::Cpp,
            "go" => Language::Go,
            "html" | "htm" => Language::Html,
            "css" | "scss" | "less" => Language::Css,
            "json" => Language::Json,
            "yaml" | "yml" => Language::Yaml,
            "toml" => Language::Toml,
            "md" | "mdx" => Language::Markdown,
            _ => Language::PlainText,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::Go => "Go",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Toml => "TOML",
            Language::Markdown => "Markdown",
            Language::PlainText => "Plain Text",
        }
    }
}

pub struct Buffer {
    pub rope: Rope,
    pub path: Option<PathBuf>,
    pub modified: bool,
    pub language: Language,
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from(text),
            path: None,
            modified: false,
            language: Language::PlainText,
        }
    }

    pub fn open(path: PathBuf, content: &str) -> Self {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let lang = Language::from_extension(ext);
        Self {
            rope: Rope::from(content),
            path: Some(path),
            modified: false,
            language: lang,
        }
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line_without_newline(&self, index: usize) -> String {
        let line = self.rope.line(index);
        let text = line.to_string();
        text.trim_end_matches(['\n', '\r']).to_string()
    }

    pub fn line_len(&self, index: usize) -> usize {
        if index >= self.line_count() {
            return 0;
        }
        let line = self.rope.line(index);
        let text = line.to_string();
        text.trim_end_matches(['\n', '\r']).chars().count()
    }

    pub fn char_at(&self, offset: usize) -> Option<char> {
        if offset >= self.rope.len_chars() {
            return None;
        }
        self.rope.get_char(offset)
    }

    pub fn char_before(&self, offset: usize) -> Option<char> {
        if offset == 0 {
            return None;
        }
        self.rope.get_char(offset - 1)
    }

    pub fn insert(&mut self, char_offset: usize, text: &str) {
        self.rope.insert(char_offset, text);
        self.modified = true;
    }

    pub fn remove(&mut self, range: std::ops::Range<usize>) {
        self.rope.remove(range);
        self.modified = true;
    }

    pub fn char_to_line_col(&self, char_offset: usize) -> (usize, usize) {
        let line = self.rope.char_to_line(char_offset);
        let col = char_offset - self.rope.line_to_char(line);
        (line, col)
    }

    pub fn line_col_to_char(&self, line: usize, col: usize) -> usize {
        self.rope.line_to_char(line) + col
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn to_string(&self) -> String {
        self.rope.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_insert_and_retrieve() {
        let mut buf = Buffer::new("hello\nworld");
        let offset = buf.line_col_to_char(1, 0);
        buf.insert(offset, "beautiful ");

        assert_eq!(buf.to_string(), "hello\nbeautiful world");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line_len(0), 5);
        assert_eq!(buf.line_len(1), 15);
    }

    #[test]
    fn test_line_col_to_char_roundtrip() {
        let buf = Buffer::new("hello\nbeautiful world\nfoo");

        // Convert (line, col) → char offset → back to (line, col)
        let offset = buf.line_col_to_char(1, 4);
        let (line, col) = buf.char_to_line_col(offset);
        assert_eq!(line, 1);
        assert_eq!(col, 4);

        let offset2 = buf.line_col_to_char(0, 0);
        assert_eq!(offset2, 0);

        let offset3 = buf.line_col_to_char(1, 0);
        let (line3, col3) = buf.char_to_line_col(offset3);
        assert_eq!(line3, 1);
        assert_eq!(col3, 0);
    }

    #[test]
    fn test_delete_and_replacement() {
        let mut buf = Buffer::new("hello world\nfoo bar");
        let offset = buf.line_col_to_char(0, 6);
        buf.remove(offset..offset + 5);  // remove "world"

        assert_eq!(buf.to_string(), "hello \nfoo bar");
    }

    #[test]
    fn test_multiple_line_insertion() {
        let buf = Buffer::new("line1\nline2\nline3");

        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.line_without_newline(0), "line1");
        assert_eq!(buf.line_without_newline(1), "line2");
        assert_eq!(buf.line_without_newline(2), "line3");
    }

    #[test]
    fn test_append_at_end() {
        let mut buf = Buffer::new("hello");
        let end = buf.line_col_to_char(0, 5);
        buf.insert(end, " world");

        assert_eq!(buf.to_string(), "hello world");
    }

    #[test]
    fn test_empty_buffer_operations() {
        let buf = Buffer::new("");
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.line_len(0), 0);
    }

    #[test]
    fn test_jump_to_start_and_end() {
        let buf = Buffer::new("line1\nline2\nline3\nline4");

        // gg equivalent: line 0, col 0
        let start_offset = buf.line_col_to_char(0, 0);
        let (line, col) = buf.char_to_line_col(start_offset);
        assert_eq!(line, 0);
        assert_eq!(col, 0);

        // G equivalent: last line
        let last_line = buf.line_count() - 1;
        let end_offset = buf.line_col_to_char(last_line, 0);
        let (line, col) = buf.char_to_line_col(end_offset);
        assert_eq!(line, last_line);
        assert_eq!(col, 0);
    }
}
