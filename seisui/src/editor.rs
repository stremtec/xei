use gpui::*;
use crate::buffer::Buffer;
use crate::cursor::Cursor;
use crate::theme::Theme;
use crate::syntax::{SyntaxEngine, TokenKind};

#[derive(PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    Command,
}

pub struct EditorView {
    buffer: Buffer,
    cursor: Cursor,
    theme: Theme,
    syntax: SyntaxEngine,
    mode: Mode,
    pending_key: Option<String>,
    scroll_line: usize,
    viewport_size: Size<Pixels>,
    focus_handle: FocusHandle,
}

impl EditorView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut editor = Self {
            buffer: Buffer::new(include_str!("../sample.rs")),
            cursor: Cursor::new(),
            theme: Theme::default(),
            syntax: SyntaxEngine::new(),
            mode: Mode::Normal,
            pending_key: None,
            scroll_line: 0,
            viewport_size: size(px(0.), px(0.)),
            focus_handle: cx.focus_handle(),
        };
        editor.buffer.language = crate::buffer::Language::Rust;
        editor.reparse();
        editor
    }

    fn reparse(&mut self) {
        let source = self.buffer.to_string();
        self.syntax.parse(&source, &self.buffer.language);
    }

    fn handle_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            Mode::Normal => self.handle_normal(event, cx),
            Mode::Insert => self.handle_insert(event, cx),
            _ => {}
        }
    }

    fn handle_normal(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;

        if let Some(prefix) = self.pending_key.take() {
            match (prefix.as_str(), key.as_str()) {
                ("g", "g") => {
                    self.cursor.position.line = 0;
                    self.cursor.position.column = 0;
                    cx.notify();
                    return;
                }
                ("d", "d") => {
                    let line_start = self.buffer.line_col_to_char(self.cursor.position.line, 0);
                    let line_end = self.buffer.line_col_to_char(self.cursor.position.line + 1, 0);
                    let end = line_end.min(self.buffer.len_chars());
                    if line_start < end {
                        self.buffer.remove(line_start..end);
                        self.reparse();
                    }
                    if self.cursor.position.line >= self.buffer.line_count() {
                        self.cursor.position.line = self.buffer.line_count().saturating_sub(1);
                    }
                    self.cursor.position.column = 0;
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

        match key.as_str() {
            "i" => self.mode = Mode::Insert,
            "a" => { self.move_cursor_right(); self.mode = Mode::Insert; }
            "A" => {
                let len = self.buffer.line_len(self.cursor.position.line);
                self.cursor.position.column = len;
                self.mode = Mode::Insert;
                cx.notify();
            }
            "I" => {
                let text = self.buffer.line_without_newline(self.cursor.position.line);
                let first_non_ws = text.chars().position(|c| c != ' ' && c != '\t').unwrap_or(0);
                self.cursor.position.column = first_non_ws;
                self.mode = Mode::Insert;
                cx.notify();
            }
            "o" => {
                let line_end = self.buffer.line_col_to_char(self.cursor.position.line + 1, 0).saturating_sub(1);
                if line_end < self.buffer.len_chars() {
                    self.buffer.insert(line_end, "\n");
                } else {
                    self.buffer.insert(self.buffer.len_chars(), "\n");
                }
                self.cursor.position.line += 1;
                self.cursor.position.column = 0;
                self.reparse();
                self.mode = Mode::Insert;
                cx.notify();
            }
            "O" => {
                let offset = self.buffer.line_col_to_char(self.cursor.position.line, 0);
                self.buffer.insert(offset, "\n");
                self.cursor.position.column = 0;
                self.reparse();
                self.mode = Mode::Insert;
                cx.notify();
            }
            "h" | "left" => { self.move_cursor_left(); cx.notify(); }
            "j" | "down" => { self.move_cursor_down(); cx.notify(); }
            "k" | "up" => { self.move_cursor_up(); cx.notify(); }
            "l" | "right" => { self.move_cursor_right(); cx.notify(); }
            "w" => {
                let line = self.buffer.line_without_newline(self.cursor.position.line);
                let chars: Vec<char> = line.chars().collect();
                let mut i = self.cursor.position.column.min(chars.len());
                while i < chars.len() && chars[i].is_alphanumeric() { i += 1; }
                while i < chars.len() && !chars[i].is_alphanumeric() { i += 1; }
                self.cursor.position.column = i.min(chars.len());
                cx.notify();
            }
            "b" => {
                let line = self.buffer.line_without_newline(self.cursor.position.line);
                let chars: Vec<char> = line.chars().collect();
                let mut i = (self.cursor.position.column as isize - 1).max(0) as usize;
                while i > 0 && !chars[i].is_alphanumeric() { i -= 1; }
                while i > 0 && chars[i - 1].is_alphanumeric() { i -= 1; }
                self.cursor.position.column = i;
                cx.notify();
            }
            "0" | "home" => { self.cursor.position.column = 0; cx.notify(); }
            "$" | "end" => {
                let len = self.buffer.line_len(self.cursor.position.line);
                self.cursor.position.column = len;
                cx.notify();
            }
            "G" => {
                self.cursor.position.line = self.buffer.line_count().saturating_sub(1);
                self.cursor.position.column = 0;
                cx.notify();
            }
            "g" | "d" => {
                self.pending_key = Some(key.clone());
            }
            "x" => {
                let offset = self.offset_at();
                if offset < self.buffer.len_chars() {
                    self.buffer.remove(offset..offset + 1);
                    self.reparse();
                    cx.notify();
                }
            }
            ":" => {
                self.mode = Mode::Command;
                cx.notify();
            }
            _ => {}
        }
    }

    fn handle_insert(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        match key.as_str() {
            "escape" => {
                self.mode = Mode::Normal;
                if self.cursor.position.column > 0 {
                    self.cursor.position.column -= 1;
                }
                cx.notify();
            }
            "backspace" => {
                let offset = self.offset_at();
                if offset > 0 {
                    self.buffer.remove(offset - 1..offset);
                    if self.cursor.position.column > 0 {
                        self.cursor.position.column -= 1;
                    } else if self.cursor.position.line > 0 {
                        let prev_len = self.buffer.line_len(self.cursor.position.line - 1);
                        self.cursor.position.line -= 1;
                        self.cursor.position.column = prev_len;
                    }
                    self.reparse();
                    cx.notify();
                }
            }
            "return" | "enter" => {
                let offset = self.offset_at();
                self.buffer.insert(offset, "\n");
                self.cursor.position.line += 1;
                self.cursor.position.column = 0;
                self.reparse();
                cx.notify();
            }
            "tab" => {
                let offset = self.offset_at();
                self.buffer.insert(offset, "    ");
                self.cursor.position.column += 4;
                self.reparse();
                cx.notify();
            }
            _ => {
                if let Some(ch) = &event.keystroke.key_char {
                    if !ch.is_empty() && ch != "\u{7f}" {
                        let offset = self.offset_at();
                        self.buffer.insert(offset, ch);
                        self.cursor.position.column += 1;
                        self.reparse();
                        cx.notify();
                    }
                }
            }
        }
    }

    fn offset_at(&self) -> usize {
        self.buffer.line_col_to_char(self.cursor.position.line, self.cursor.position.column)
    }

    fn move_cursor_left(&mut self) {
        if self.cursor.position.column > 0 {
            self.cursor.position.column -= 1;
        } else if self.cursor.position.line > 0 {
            self.cursor.position.line -= 1;
            self.cursor.position.column = self.buffer.line_len(self.cursor.position.line);
        }
    }

    fn move_cursor_right(&mut self) {
        let len = self.buffer.line_len(self.cursor.position.line);
        if self.cursor.position.column < len {
            self.cursor.position.column += 1;
        } else if self.cursor.position.line + 1 < self.buffer.line_count() {
            self.cursor.position.line += 1;
            self.cursor.position.column = 0;
        }
    }

    fn move_cursor_up(&mut self) {
        if self.cursor.position.line > 0 {
            self.cursor.position.line -= 1;
            let len = self.buffer.line_len(self.cursor.position.line);
            if self.cursor.position.column > len {
                self.cursor.position.column = len;
            }
        }
    }

    fn move_cursor_down(&mut self) {
        if self.cursor.position.line + 1 < self.buffer.line_count() {
            self.cursor.position.line += 1;
            let len = self.buffer.line_len(self.cursor.position.line);
            if self.cursor.position.column > len {
                self.cursor.position.column = len;
            }
        }
    }

    fn line_segments(&self, line_idx: usize) -> Vec<(String, Hsla)> {
        let line = self.buffer.line_without_newline(line_idx);
        if line.is_empty() {
            return vec![(String::new(), self.theme.fg)];
        }
        let line_start_char = self.buffer.line_col_to_char(line_idx, 0);
        let line_end_char = line_start_char + line.len();
        let source = self.buffer.to_string();

        let tokens = self.syntax.tokens_for_range(&source, line_start_char, line_end_char);

        let mut relative_tokens: Vec<(usize, usize, TokenKind)> = tokens
            .iter()
            .map(|(s, e, k)| {
                let start = s.saturating_sub(line_start_char);
                let end = e.saturating_sub(line_start_char).min(line.len());
                (start, end, *k)
            })
            .filter(|(_, e, _)| *e > 0)
            .collect();
        relative_tokens.sort_by_key(|(s, _, _)| *s);

        let mut segments = Vec::new();
        let mut pos = 0;
        for (s, e, kind) in &relative_tokens {
            if *s > pos {
                let text: String = line.chars().skip(pos).take(s - pos).collect();
                segments.push((text, self.theme.fg));
            }
            let text: String = line.chars().skip(*s).take(e - s).collect();
            segments.push((text, self.theme.color_for(kind)));
            pos = *e;
        }
        if pos < line.len() {
            let text: String = line.chars().skip(pos).collect();
            segments.push((text, self.theme.fg));
        }

        segments
    }

    fn render_gutter(&self) -> AnyElement {
        let line_count = self.buffer.line_count().max(1);
        let lines: Vec<AnyElement> = (1..=line_count)
            .map(|n| {
                let is_cursor = n - 1 == self.cursor.position.line;
                let bg = if is_cursor { self.theme.line_highlight } else { self.theme.gutter_bg };
                div()
                    .bg(bg)
                    .w(px(60.))
                    .font_family("Buffer")
                    .child(format!("{:>4} ", n))
                    .into_any_element()
            })
            .collect();

        div()
            .bg(self.theme.gutter_bg)
            .text_color(self.theme.gutter_fg)
            .font_family("Buffer")
            .flex()
            .flex_col()
            .children(lines)
            .into_any_element()
    }

    fn render_text_line(&self, line_idx: usize) -> AnyElement {
        let segments = self.line_segments(line_idx);
        let is_cursor_line = line_idx == self.cursor.position.line;
        let col = self.cursor.position.column;
        let line = self.buffer.line_without_newline(line_idx);
        let line_bg = if is_cursor_line { self.theme.line_highlight } else { self.theme.bg };

        if !is_cursor_line {
            let children: Vec<AnyElement> = segments
                .into_iter()
                .map(|(text, color)| {
                    div().text_color(color).font_family("Buffer").child(text).into_any_element()
                })
                .collect();
            return div()
                .bg(line_bg).flex().flex_row().w_full()
                .children(children)
                .into_any_element();
        }

        let mut children: Vec<AnyElement> = Vec::new();
        let mut char_pos: usize = 0;
        let mut cursor_inserted = false;

        for (_, (text, color)) in segments.iter().enumerate() {
            let seg_len = text.chars().count();
            let seg_end = char_pos + seg_len;
            let cursor_in_seg = !cursor_inserted && col >= char_pos && col <= seg_end;

            if cursor_in_seg {
                let cursor_offset = col - char_pos;
                let before: String = text.chars().take(cursor_offset).collect();
                let at_cursor: String = text.chars().skip(cursor_offset).take(1).collect();
                let after: String = text.chars().skip(cursor_offset + 1).collect();

                if !before.is_empty() {
                    children.push(div().text_color(*color).font_family("Buffer").child(before).into_any_element());
                }

                if self.mode == Mode::Normal {
                    let cursor_text = if at_cursor.is_empty() { " ".to_string() } else { at_cursor };
                    children.push(
                        div()
                            .bg(self.theme.cursor)
                            .text_color(self.theme.bg)
                            .font_family("Buffer")
                            .child(cursor_text)
                            .into_any_element()
                    );
                } else {
                    children.push(
                        div()
                            .bg(self.theme.cursor)
                            .w(px(1.5))
                            .h(px(20.))
                            .into_any_element()
                    );
                    if !at_cursor.is_empty() {
                        children.push(div().text_color(*color).font_family("Buffer").child(at_cursor).into_any_element());
                    }
                }
                if !after.is_empty() {
                    children.push(div().text_color(*color).font_family("Buffer").child(after).into_any_element());
                }
                cursor_inserted = true;
            } else {
                children.push(div().text_color(*color).font_family("Buffer").child(text.clone()).into_any_element());
            }
            char_pos = seg_end;
        }

        if !cursor_inserted && col >= line.len() {
            if self.mode == Mode::Normal {
                children.push(
                    div()
                        .bg(self.theme.cursor)
                        .font_family("Buffer")
                        .child(" ")
                        .into_any_element()
                );
            } else {
                children.push(
                    div()
                        .bg(self.theme.cursor)
                        .w(px(1.5))
                        .h(px(20.))
                        .into_any_element()
                );
            }
        }

        div()
            .bg(line_bg)
            .flex().flex_row().w_full()
            .children(children)
            .into_any_element()
    }

    fn render_text_area(&self) -> AnyElement {
        let line_count = self.buffer.line_count();
        let lines: Vec<AnyElement> = (0..line_count)
            .map(|i| self.render_text_line(i))
            .collect();

        div()
            .size_full()
            .bg(self.theme.bg)
            .flex()
            .flex_col()
            .children(lines)
            .into_any_element()
    }

    fn render_status_bar(&self) -> AnyElement {
        let mode_text = match self.mode {
            Mode::Normal => {
                if let Some(ref pk) = self.pending_key {
                    pk.clone()
                } else {
                    "NORMAL".to_string()
                }
            }
            Mode::Insert => "INSERT".to_string(),
            Mode::Visual => "VISUAL".to_string(),
            Mode::Command => "COMMAND".to_string(),
        };

        let file_info = self.buffer.path.as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("[sample]")
            .to_string();

        let lang = self.buffer.language.name().to_string();
        let cur = format!("{}:{}", self.cursor.position.line + 1, self.cursor.position.column + 1);

        div()
            .w_full().h(px(24.))
            .bg(self.theme.status_bg)
            .text_color(self.theme.status_fg)
            .flex().flex_row().items_center().justify_between()
            .px(px(12.))
            .child(
                div().flex().flex_row().gap(px(16.))
                    .children(vec![div().child(mode_text), div().child(file_info)])
            )
            .child(
                div().flex().flex_row().gap(px(16.))
                    .children(vec![div().child(lang), div().child(cur)])
            )
            .into_any_element()
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex().flex_col().size_full()
            .bg(self.theme.bg)
            .key_context("Editor")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key(event, window, cx);
            }))
            .child(
                div().flex().flex_row().flex_1()
                    .children(vec![self.render_gutter(), self.render_text_area()])
            )
            .child(self.render_status_bar())
    }
}
