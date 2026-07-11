use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

impl Position {
    #[allow(dead_code)]
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }

    pub fn zero() -> Self {
        Self { row: 0, col: 0 }
    }
}

#[derive(Clone)]
pub struct Buffer {
    lines: Vec<String>,
    pub cursor: Position,
    /// Bumped on every text mutation — frames re-parse/re-sync only on change.
    version: u64,
}

fn next_version() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl Default for Buffer {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            version: next_version(),
            cursor: Position::zero(),
        }
    }
}

impl Buffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_string(text: &str) -> Self {
        let lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        Self {
            lines,
            cursor: Position::zero(),
            version: next_version(),
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, row: usize) -> &str {
        self.lines.get(row).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> Position {
        self.cursor
    }

    pub fn current_line_len(&self) -> usize {
        self.line(self.cursor.row).chars().count()
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
    }

    pub fn move_right(&mut self) {
        let max = self.current_line_len();
        if self.cursor.col < max {
            self.cursor.col += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor.row > 0 {
            self.cursor.row -= 1;
            self.clamp_col();
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor.row < self.lines.len() - 1 {
            self.cursor.row += 1;
            self.clamp_col();
        }
    }

    pub fn move_to_line_start(&mut self) {
        self.cursor.col = 0;
    }

    pub fn move_to_line_end(&mut self) {
        self.cursor.col = self.current_line_len();
    }

    pub fn clamp_col(&mut self) {
        let max = self.current_line_len();
        if self.cursor.col > max {
            self.cursor.col = max;
        }
    }

    pub fn set_line(&mut self, row: usize, text: String) {
        self.touch();
        if row < self.lines.len() {
            self.lines[row] = text;
        }
    }

    pub fn buffer_col_to_screen_col(&self, row: usize, buf_col: usize) -> usize {
        let line = self.line(row);
        let mut visual = 0;
        for (i, ch) in line.chars().enumerate() {
            if i >= buf_col {
                return visual;
            }
            visual += if ch == '\t' { 4 - (visual % 4) } else { ch.width().unwrap_or(1) };
        }
        visual
    }

    pub fn screen_col_to_buffer_col(&self, row: usize, screen_col: usize) -> usize {
        let line = self.line(row);
        let mut visual = 0;
        let mut buf_col = 0;
        for ch in line.chars() {
            let w = if ch == '\t' {
                4 - (visual % 4)
            } else {
                ch.width().unwrap_or(1)
            };
            if visual + w > screen_col {
                return buf_col;
            }
            visual += w;
            buf_col += 1;
        }
        buf_col
    }

    #[allow(dead_code)]
    pub fn append_to_line(&mut self, row: usize, text: &str) {
        self.touch();
        if row < self.lines.len() {
            self.lines[row].push_str(text);
        }
    }

    pub fn insert_line_at(&mut self, row: usize, line: String) {
        self.touch();
        self.lines.insert(row, line);
        self.cursor.row = row;
    }

    pub fn insert_char(&mut self, ch: char) {
        self.touch();
        let line = &mut self.lines[self.cursor.row];
        let byte_idx = char_to_byte(self.cursor.col, line);
        line.insert(byte_idx, ch);
        self.cursor.col += 1;
    }

    /// Insert multi-line text at the cursor (snippets / paste-like).
    pub fn insert_str(&mut self, s: &str) {
        self.touch();
        for ch in s.chars() {
            if ch == '\n' {
                self.insert_newline();
            } else {
                self.insert_char(ch);
            }
        }
    }

    pub fn insert_char_pair(&mut self, open: char, close: char) {
        self.touch();
        let line = &mut self.lines[self.cursor.row];
        let byte_idx = char_to_byte(self.cursor.col, line);
        line.insert(byte_idx, open);
        let byte_idx2 = char_to_byte(self.cursor.col + 1, line);
        line.insert(byte_idx2, close);
        self.cursor.col += 1;
    }

    pub fn char_after_cursor(&self) -> Option<char> {
        self.line(self.cursor.row).chars().nth(self.cursor.col)
    }

    pub fn char_before_cursor(&self) -> Option<char> {
        if self.cursor.col > 0 {
            self.line(self.cursor.row).chars().nth(self.cursor.col - 1)
        } else {
            None
        }
    }

    pub fn skip_char_if_match(&mut self, ch: char) -> bool {
        if self.char_after_cursor() == Some(ch) {
            self.cursor.col += 1;
            true
        } else {
            false
        }
    }

    pub fn delete_pair(&mut self, open: char, close: char) -> bool {
        self.touch();
        if self.char_before_cursor() == Some(open) && self.char_after_cursor() == Some(close) {
            let line_str = self.lines[self.cursor.row].clone();
            let open_byte = char_to_byte(self.cursor.col - 1, &line_str);
            let open_end = char_to_byte(self.cursor.col, &line_str);
            let close_byte = char_to_byte(self.cursor.col, &line_str);
            let close_end = char_to_byte(self.cursor.col + 1, &line_str);

            let line = &mut self.lines[self.cursor.row];
            line.drain(close_byte..close_end);
            line.drain(open_byte..open_end);
            self.cursor.col -= 1;
            true
        } else {
            false
        }
    }

    pub fn insert_newline(&mut self) {
        self.touch();
        let line = &mut self.lines[self.cursor.row];
        let byte_idx = char_to_byte(self.cursor.col, line);
        let after: String = line.drain(byte_idx..).collect();
        self.lines.insert(self.cursor.row + 1, after);
        self.cursor.row += 1;
        self.cursor.col = 0;
    }

    pub fn insert_newline_with_indent(&mut self, extra_indent: bool) {
        self.touch();
        let current_row = self.cursor.row;
        let indent = self.leading_indent(current_row);

        let line = &mut self.lines[current_row];
        let byte_idx = char_to_byte(self.cursor.col, line);
        let after: String = line.drain(byte_idx..).collect();

        let mut new_line = indent.clone();
        if extra_indent {
            new_line.push_str("    ");
        }
        new_line.push_str(&after);

        self.lines.insert(current_row + 1, new_line);
        self.cursor.row += 1;
        // cursor.col is a char index
        self.cursor.col = indent.chars().count() + if extra_indent { 4 } else { 0 };
    }

    pub fn leading_indent(&self, row: usize) -> String {
        let line = self.line(row);
        let indent_len = line.chars().take_while(|c| c.is_whitespace()).count();
        line.chars().take(indent_len).collect()
    }

    pub fn backspace(&mut self) {
        self.touch();
        if self.cursor.col > 0 {
            let line = &mut self.lines[self.cursor.row];
            let byte_idx = char_to_byte(self.cursor.col - 1, line);
            let next_byte_idx = char_to_byte(self.cursor.col, line);
            line.drain(byte_idx..next_byte_idx);
            self.cursor.col -= 1;
        } else if self.cursor.row > 0 {
            let moved_line = self.lines.remove(self.cursor.row);
            self.cursor.row -= 1;
            let prev_line_len = self.line(self.cursor.row).chars().count();
            let prev_line = &mut self.lines[self.cursor.row];
            prev_line.push_str(&moved_line);
            self.cursor.col = prev_line_len;
        }
    }

    pub fn delete_char_at_cursor(&mut self) {
        self.touch();
        let line_len = self.current_line_len();
        if self.cursor.col < line_len {
            let line = &mut self.lines[self.cursor.row];
            let byte_idx = char_to_byte(self.cursor.col, line);
            let next_byte_idx = char_to_byte(self.cursor.col + 1, line);
            line.drain(byte_idx..next_byte_idx);
        } else if self.cursor.row < self.lines.len() - 1 {
            let next_line = self.lines.remove(self.cursor.row + 1);
            self.lines[self.cursor.row].push_str(&next_line);
        }
    }

    pub fn delete_line(&mut self) -> String {
        self.touch();
        if self.lines.len() == 1 {
            let line = std::mem::take(&mut self.lines[0]);
            self.cursor.col = 0;
            return line;
        }
        let line = self.lines.remove(self.cursor.row);
        if self.cursor.row >= self.lines.len() {
            self.cursor.row = self.lines.len() - 1;
        }
        self.clamp_col();
        line
    }

    pub fn delete_word(&mut self) -> String {
        self.touch();
        let chars: Vec<char> = self.line(self.cursor.row).chars().collect();
        if self.cursor.col >= chars.len() {
            return String::new();
        }

        let start = self.cursor.col;
        let mut end = start;

        let class = char_class(chars[start]);
        while end < chars.len() && char_class(chars[end]) == class {
            end += 1;
        }

        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }

        let deleted: String = chars[start..end].iter().collect();
        let line = &mut self.lines[self.cursor.row];
        let start_byte = char_to_byte(start, line);
        let end_byte = char_to_byte(end, line);
        line.drain(start_byte..end_byte);
        self.clamp_col();
        deleted
    }

    pub fn paste_line_after(&mut self, text: &str) {
        self.touch();
        self.lines.insert(self.cursor.row + 1, text.to_string());
        self.cursor.row += 1;
        self.cursor.col = 0;
    }

    pub fn move_word_forward(&mut self) {
        let chars: Vec<char> = self.line(self.cursor.row).chars().collect();
        let mut pos = self.cursor.col;

        if pos >= chars.len() {
            if self.cursor.row < self.lines.len() - 1 {
                self.cursor.row += 1;
                self.cursor.col = 0;
                let new_chars: Vec<char> = self.line(self.cursor.row).chars().collect();
                let mut p = 0;
                while p < new_chars.len() && char_class(new_chars[p]) == CharClass::Whitespace {
                    p += 1;
                }
                self.cursor.col = p;
            }
            return;
        }

        let current_class = char_class(chars[pos]);
        while pos < chars.len() && char_class(chars[pos]) == current_class {
            pos += 1;
        }

        while pos < chars.len() && char_class(chars[pos]) == CharClass::Whitespace {
            pos += 1;
        }

        if pos >= chars.len() && self.cursor.row < self.lines.len() - 1 {
            self.cursor.row += 1;
            self.cursor.col = 0;
            let new_chars: Vec<char> = self.line(self.cursor.row).chars().collect();
            let mut p = 0;
            while p < new_chars.len() && char_class(new_chars[p]) == CharClass::Whitespace {
                p += 1;
            }
            self.cursor.col = p;
        } else {
            self.cursor.col = pos;
        }
    }

    pub fn move_word_back(&mut self) {
        if self.cursor.col == 0 {
            if self.cursor.row > 0 {
                self.cursor.row -= 1;
                self.cursor.col = self.current_line_len();
            }
            return;
        }

        let chars: Vec<char> = self.line(self.cursor.row).chars().collect();
        let mut pos = self.cursor.col;

        pos = pos.saturating_sub(1);

        while pos > 0 && char_class(chars[pos]) == CharClass::Whitespace {
            pos -= 1;
        }

        if pos == 0 && char_class(chars[0]) == CharClass::Whitespace {
            self.cursor.col = 0;
            return;
        }

        let target_class = char_class(chars[pos]);
        while pos > 0 && char_class(chars[pos - 1]) == target_class {
            pos -= 1;
        }

        self.cursor.col = pos;
    }

    /// Monotonic text version (constructor + every mutation get fresh values).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Mark the text as changed.
    pub fn touch(&mut self) {
        self.version = next_version();
    }

    pub fn snapshot(&self) -> BufferSnapshot {
        BufferSnapshot {
            lines: self.lines.clone(),
            cursor: self.cursor,
        }
    }

    pub fn restore(&mut self, snapshot: &BufferSnapshot) {
        self.touch();
        self.lines = snapshot.lines.clone();
        self.cursor = snapshot.cursor;
    }

    // ── In-line character search (char indices, UTF-8 safe) ──

    pub fn find_char_forward(&mut self, ch: char) {
        let chars: Vec<char> = self.line(self.cursor.row).chars().collect();
        if self.cursor.col + 1 >= chars.len() {
            return;
        }
        if let Some(rel) = chars[self.cursor.col + 1..].iter().position(|c| *c == ch) {
            self.cursor.col = self.cursor.col + 1 + rel;
        }
    }

    pub fn find_char_backward(&mut self, ch: char) {
        if self.cursor.col == 0 {
            return;
        }
        let chars: Vec<char> = self.line(self.cursor.row).chars().collect();
        let end = self.cursor.col.min(chars.len());
        if let Some(pos) = chars[..end].iter().rposition(|c| *c == ch) {
            self.cursor.col = pos;
        }
    }

    pub fn till_char_forward(&mut self, ch: char) {
        let chars: Vec<char> = self.line(self.cursor.row).chars().collect();
        if self.cursor.col + 1 >= chars.len() {
            return;
        }
        if let Some(rel) = chars[self.cursor.col + 1..].iter().position(|c| *c == ch) {
            if rel > 0 {
                self.cursor.col = self.cursor.col + rel;
            }
        }
    }

    pub fn till_char_backward(&mut self, ch: char) {
        if self.cursor.col <= 1 {
            return;
        }
        let chars: Vec<char> = self.line(self.cursor.row).chars().collect();
        let end = self.cursor.col.saturating_sub(1).min(chars.len());
        if let Some(pos) = chars[..end].iter().rposition(|c| *c == ch) {
            self.cursor.col = pos + 1;
        }
    }

    // ── Replace ────────────────────────────────────────

    pub fn replace_char(&mut self, ch: char) {
        self.touch();
        let line = &self.lines[self.cursor.row];
        let len = line.chars().count();
        if self.cursor.col >= len {
            return;
        }
        let start = char_to_byte(self.cursor.col, line);
        let end = char_to_byte(self.cursor.col + 1, line);
        let mut new_line = String::new();
        new_line.push_str(&line[..start]);
        new_line.push(ch);
        new_line.push_str(&line[end..]);
        self.lines[self.cursor.row] = new_line;
    }

    // ── Indent / Dedent ────────────────────────────────

    pub fn indent_line(&mut self) {
        self.touch();
        self.lines[self.cursor.row].insert_str(0, "    ");
        self.cursor.col += 4;
    }

    pub fn dedent_line(&mut self) {
        self.touch();
        let line = &self.lines[self.cursor.row];
        if line.starts_with("    ") {
            self.lines[self.cursor.row] = line[4..].to_string();
            self.cursor.col = self.cursor.col.saturating_sub(4);
        } else if line.starts_with(' ') {
            let spaces = line.chars().take_while(|c| *c == ' ').count().min(4);
            let byte = char_to_byte(spaces, line);
            self.lines[self.cursor.row] = line[byte..].to_string();
            self.cursor.col = self.cursor.col.saturating_sub(spaces);
        } else if line.starts_with('\t') {
            self.lines[self.cursor.row] = line[1..].to_string();
            self.cursor.col = self.cursor.col.saturating_sub(1);
        }
    }

    // ── Join lines ─────────────────────────────────────

    pub fn join_lines(&mut self) {
        self.touch();
        if self.cursor.row + 1 < self.lines.len() {
            let next = self.lines.remove(self.cursor.row + 1);
            let next_trim = next.trim_start();
            let current_len = self.lines[self.cursor.row].chars().count();
            let cur = &mut self.lines[self.cursor.row];
            if !cur.is_empty() && !cur.ends_with(' ') && !next_trim.is_empty() {
                cur.push(' ');
            }
            cur.push_str(next_trim);
            self.cursor.col = if next_trim.is_empty() {
                current_len
            } else if current_len > 0 {
                current_len // space is at current_len if we added one... simplified:
            } else {
                0
            };
            // Place cursor on the joining space (vim-like)
            if !next_trim.is_empty() && current_len > 0 {
                self.cursor.col = current_len; // on the space we may have inserted
            }
        }
    }

    // ── First non-blank ────────────────────────────────

    pub fn move_to_first_non_blank(&mut self) {
        let line = self.line(self.cursor.row);
        self.cursor.col = line.chars().position(|c| c != ' ' && c != '\t').unwrap_or(0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharClass {
    Whitespace,
    Word,
    Punctuation,
}

fn char_class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

#[derive(Clone)]
pub struct BufferSnapshot {
    lines: Vec<String>,
    cursor: Position,
}

impl BufferSnapshot {
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
    pub fn cursor(&self) -> Position {
        self.cursor
    }
    pub fn from_parts(lines: Vec<String>, cursor: Position) -> Self {
        Self { lines, cursor }
    }
}

fn char_to_byte(char_idx: usize, line: &str) -> usize {
    line.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(line.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer_has_one_empty_line() {
        let buf = Buffer::new();
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.line(0), "");
    }

    #[test]
    fn test_from_string_multiline() {
        let buf = Buffer::from_string("hello\nworld");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line(0), "hello");
        assert_eq!(buf.line(1), "world");
    }

    #[test]
    fn test_insert_char() {
        let mut buf = Buffer::new();
        buf.insert_char('h');
        buf.insert_char('i');
        assert_eq!(buf.line(0), "hi");
        assert_eq!(buf.cursor.col, 2);
    }

    #[test]
    fn test_insert_newline() {
        let mut buf = Buffer::from_string("hello");
        buf.cursor = Position::new(0, 2);
        buf.insert_newline();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line(0), "he");
        assert_eq!(buf.line(1), "llo");
        assert_eq!(buf.cursor, Position::new(1, 0));
    }

    #[test]
    fn test_backspace_merge_lines() {
        let mut buf = Buffer::from_string("he\nllo");
        buf.cursor = Position::new(1, 0);
        buf.backspace();
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.line(0), "hello");
        assert_eq!(buf.cursor, Position::new(0, 2));
    }

    #[test]
    fn test_move_left_right() {
        let mut buf = Buffer::from_string("abc");
        buf.cursor.col = 1;
        buf.move_left();
        assert_eq!(buf.cursor.col, 0);
        buf.move_left();
        assert_eq!(buf.cursor.col, 0);
        buf.move_right();
        assert_eq!(buf.cursor.col, 1);
    }

    #[test]
    fn test_move_up_down_clamps_col() {
        let mut buf = Buffer::from_string("long line\nx");
        buf.cursor = Position::new(0, 5);
        buf.move_down();
        assert_eq!(buf.cursor.row, 1);
        assert_eq!(buf.cursor.col, 1);
    }

    #[test]
    fn test_delete_char_at_cursor() {
        let mut buf = Buffer::from_string("abc");
        buf.cursor.col = 0;
        buf.delete_char_at_cursor();
        assert_eq!(buf.line(0), "bc");
    }

    #[test]
    fn test_unicode_insert_and_delete() {
        let mut buf = Buffer::new();
        buf.insert_char('ä');
        buf.insert_char('o');
        buf.insert_char('\u{3042}');
        assert_eq!(buf.line(0), "äoあ");
        assert_eq!(buf.cursor.col, 3);
        buf.backspace();
        assert_eq!(buf.line(0), "äo");
    }

    #[test]
    fn test_text_output() {
        let buf = Buffer::from_string("line1\nline2\nline3");
        assert_eq!(buf.text(), "line1\nline2\nline3");
    }

    #[test]
    fn test_delete_line() {
        let mut buf = Buffer::from_string("a\nb\nc");
        buf.cursor.row = 1;
        let deleted = buf.delete_line();
        assert_eq!(deleted, "b");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line(0), "a");
        assert_eq!(buf.line(1), "c");
    }

    #[test]
    fn test_delete_last_line() {
        let mut buf = Buffer::from_string("a\nb");
        buf.cursor.row = 1;
        let deleted = buf.delete_line();
        assert_eq!(deleted, "b");
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.line(0), "a");
        assert_eq!(buf.cursor.row, 0);
    }

    #[test]
    fn test_delete_only_line() {
        let mut buf = Buffer::from_string("hello");
        let deleted = buf.delete_line();
        assert_eq!(deleted, "hello");
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.line(0), "");
    }

    #[test]
    fn test_delete_word() {
        let mut buf = Buffer::from_string("hello world");
        buf.cursor.col = 0;
        let deleted = buf.delete_word();
        assert_eq!(deleted, "hello ");
        assert_eq!(buf.line(0), "world");
    }

    #[test]
    fn test_delete_word_punctuation() {
        let mut buf = Buffer::from_string("foo = bar");
        buf.cursor.col = 4;
        let deleted = buf.delete_word();
        assert_eq!(deleted, "= ");
        assert_eq!(buf.line(0), "foo bar");
    }

    #[test]
    fn test_move_word_forward() {
        let mut buf = Buffer::from_string("hello world foo");
        buf.cursor.col = 0;
        buf.move_word_forward();
        assert_eq!(buf.cursor.col, 6);
        buf.move_word_forward();
        assert_eq!(buf.cursor.col, 12);
    }

    #[test]
    fn test_move_word_back() {
        let mut buf = Buffer::from_string("hello world foo");
        buf.cursor.col = 12;
        buf.move_word_back();
        assert_eq!(buf.cursor.col, 6);
        buf.move_word_back();
        assert_eq!(buf.cursor.col, 0);
    }

    #[test]
    fn test_paste_line_after() {
        let mut buf = Buffer::from_string("a\nc");
        buf.paste_line_after("b");
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.line(0), "a");
        assert_eq!(buf.line(1), "b");
        assert_eq!(buf.line(2), "c");
        assert_eq!(buf.cursor.row, 1);
    }

    #[test]
    fn test_screen_col_to_buffer_col() {
        let buf = Buffer::from_string("\thello");
        assert_eq!(buf.screen_col_to_buffer_col(0, 0), 0);
        assert_eq!(buf.screen_col_to_buffer_col(0, 1), 0);
        assert_eq!(buf.screen_col_to_buffer_col(0, 3), 0);
        assert_eq!(buf.screen_col_to_buffer_col(0, 4), 1);
        assert_eq!(buf.screen_col_to_buffer_col(0, 5), 2);
    }

    #[test]
    fn test_buffer_col_to_screen_col() {
        let buf = Buffer::from_string("\thello");
        assert_eq!(buf.buffer_col_to_screen_col(0, 0), 0);
        assert_eq!(buf.buffer_col_to_screen_col(0, 1), 4);
        assert_eq!(buf.buffer_col_to_screen_col(0, 5), 8);
    }

    #[test]
    fn test_col_roundtrip_tabs() {
        let buf = Buffer::from_string("\t\tfn main()");
        for bc in 0..=buf.line(0).chars().count() {
            let sc = buf.buffer_col_to_screen_col(0, bc);
            let back = buf.screen_col_to_buffer_col(0, sc);
            assert_eq!(back, bc, "roundtrip failed at buf_col={}", bc);
        }
    }

    #[test]
    fn test_col_roundtrip_spaces() {
        let buf = Buffer::from_string("        let x = 1;");
        for bc in 0..=buf.line(0).chars().count() {
            let sc = buf.buffer_col_to_screen_col(0, bc);
            let back = buf.screen_col_to_buffer_col(0, sc);
            assert_eq!(back, bc, "roundtrip failed at buf_col={}", bc);
        }
    }

    #[test]
    fn test_col_roundtrip_cjk() {
        let buf = Buffer::from_string("야르~");
        for bc in 0..=buf.line(0).chars().count() {
            let sc = buf.buffer_col_to_screen_col(0, bc);
            let back = buf.screen_col_to_buffer_col(0, sc);
            assert_eq!(back, bc, "roundtrip failed at buf_col={} for '야르~'", bc);
        }
    }

    #[test]
    fn test_cjk_width() {
        let buf = Buffer::from_string("a한b");
        assert_eq!(buf.buffer_col_to_screen_col(0, 0), 0); // 'a' at col 0
        assert_eq!(buf.buffer_col_to_screen_col(0, 1), 1); // '한' at col 1 → screen col 1
        assert_eq!(buf.buffer_col_to_screen_col(0, 2), 3); // 'b' at col 2 → screen col 3 (한=width 2)
    }
    #[test]
    fn test_col_roundtrip_mixed() {
        let buf = Buffer::from_string("  \t  hello\tworld");
        for bc in 0..=buf.line(0).chars().count() {
            let sc = buf.buffer_col_to_screen_col(0, bc);
            let back = buf.screen_col_to_buffer_col(0, sc);
            assert_eq!(back, bc, "roundtrip failed at buf_col={}", bc);
        }
    }

    #[test]
    fn test_snapshot_restore() {
        let mut buf = Buffer::from_string("hello");
        buf.cursor.col = 5;
        buf.insert_char('!');
        let snap = buf.snapshot();
        buf.insert_char('x');
        assert_eq!(buf.line(0), "hello!x");
        buf.restore(&snap);
        assert_eq!(buf.line(0), "hello!");
    }

    #[test]
    fn test_find_char_utf8() {
        let mut buf = Buffer::from_string("한a글b");
        buf.cursor = Position::new(0, 0);
        buf.find_char_forward('글');
        assert_eq!(buf.cursor.col, 2);
        buf.find_char_forward('b');
        assert_eq!(buf.cursor.col, 3);
        buf.find_char_backward('a');
        assert_eq!(buf.cursor.col, 1);
    }

    #[test]
    fn test_replace_char_utf8() {
        let mut buf = Buffer::from_string("한x글");
        buf.cursor = Position::new(0, 1);
        buf.replace_char('야');
        assert_eq!(buf.line(0), "한야글");
    }

    #[test]
    fn test_join_lines() {
        let mut buf = Buffer::from_string("hello\nworld");
        buf.cursor = Position::new(0, 0);
        buf.join_lines();
        assert_eq!(buf.line(0), "hello world");
        assert_eq!(buf.line_count(), 1);
    }
}
