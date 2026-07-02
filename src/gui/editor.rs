use gpui::*;
use crate::buffer::Buffer;
use crate::syntax::SyntaxEngine;
use crate::highlight::TokenKind;

#[derive(PartialEq)]
enum Mode {
    Normal,
    Insert,
}

struct GuiTheme {
    bg: Hsla,
    fg: Hsla,
    gutter_bg: Hsla,
    gutter_fg: Hsla,
    line_highlight: Hsla,
    cursor: Hsla,
    status_bg: Hsla,
    status_fg: Hsla,
    keyword: Hsla,
    string: Hsla,
    comment: Hsla,
    number: Hsla,
    type_name: Hsla,
}

impl Default for GuiTheme {
    fn default() -> Self {
        Self {
            bg: hsla(0.65, 0.15, 0.12, 1.0),
            fg: hsla(0.60, 0.20, 0.85, 1.0),
            gutter_bg: hsla(0.65, 0.12, 0.10, 1.0),
            gutter_fg: hsla(0.60, 0.10, 0.50, 1.0),
            line_highlight: hsla(0.65, 0.15, 0.18, 1.0),
            cursor: hsla(0.60, 0.80, 0.70, 1.0),
            status_bg: hsla(0.65, 0.25, 0.20, 1.0),
            status_fg: hsla(0.60, 0.15, 0.70, 1.0),
            keyword: hsla(0.72, 0.77, 0.58, 1.0),
            string: hsla(0.11, 0.57, 0.60, 1.0),
            comment: hsla(0.55, 0.30, 0.45, 1.0),
            number: hsla(0.17, 0.86, 0.60, 1.0),
            type_name: hsla(0.28, 0.65, 0.60, 1.0),
        }
    }
}

impl GuiTheme {
    fn color_for(&self, kind: TokenKind) -> Hsla {
        match kind {
            TokenKind::Keyword => self.keyword,
            TokenKind::String => self.string,
            TokenKind::Comment => self.comment,
            TokenKind::Number => self.number,
            TokenKind::TypeName => self.type_name,
        }
    }
}

pub struct GuiEditor {
    buffer: Buffer,
    syntax: SyntaxEngine,
    theme: GuiTheme,
    mode: Mode,
    filename: Option<String>,
    pending_key: Option<char>,
    ext: Option<String>,
    focus_handle: FocusHandle,
}

impl GuiEditor {
    pub fn new(cx: &mut Context<Self>, file_path: Option<String>) -> Self {
        let (buffer, filename, ext) = if let Some(ref path) = file_path {
            let content = std::fs::read_to_string(path).unwrap_or_default();
            let buf = Buffer::from_string(&content);
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string());
            (buf, Some(path.clone()), ext)
        } else {
            (Buffer::from_string(
                "Welcome to xei (晴) GUI — Rust-native editor.\n\n\
                 Press i to enter Insert mode\n\
                 Use h/j/k/l to navigate\n\
                 Press Esc to return to Normal mode\n",
            ), None, None)
        };

        let mut syntax = SyntaxEngine::new();
        syntax.parse(&buffer.text(), ext.as_deref());

        Self {
            buffer,
            syntax,
            theme: GuiTheme::default(),
            mode: Mode::Normal,
            filename,
            pending_key: None,
            ext,
            focus_handle: cx.focus_handle(),
        }
    }

    fn reparse(&mut self) {
        self.syntax.parse(&self.buffer.text(), self.ext.as_deref());
        self.syntax.tokens.retain(|(_, _, _, row)| *row < self.buffer.line_count());
    }

    fn handle_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            Mode::Normal => { self.handle_normal(event, cx); }
            Mode::Insert => { self.handle_insert(event, cx); }
        }
    }

    fn handle_normal(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        if let Some(prefix) = self.pending_key.take() {
            match (prefix, key.as_str()) {
                ('g', "g") => {
                    self.buffer.cursor.row = 0;
                    self.buffer.cursor.col = 0;
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        match key.as_str() {
            "i" => self.mode = Mode::Insert,
            "a" => { self.buffer.move_right(); self.mode = Mode::Insert; }
            "A" => { self.buffer.move_to_line_end(); self.mode = Mode::Insert; cx.notify(); }
            "I" => { self.buffer.cursor.col = 0; self.mode = Mode::Insert; cx.notify(); }
            "o" => {
                self.buffer.move_to_line_end();
                self.buffer.insert_newline_with_indent(false);
                self.reparse();
                self.mode = Mode::Insert;
                cx.notify();
            }
            "O" => {
                if self.buffer.cursor.row > 0 {
                    self.buffer.cursor.row -= 1;
                    self.buffer.move_to_line_end();
                }
                self.buffer.cursor.col = 0;
                self.buffer.insert_newline_with_indent(false);
                self.reparse();
                self.mode = Mode::Insert;
                cx.notify();
            }
            "h" | "left" => { self.buffer.move_left(); cx.notify(); }
            "j" | "down" => { self.buffer.move_down(); cx.notify(); }
            "k" | "up" => { self.buffer.move_up(); cx.notify(); }
            "l" | "right" => { self.buffer.move_right(); cx.notify(); }
            "0" | "home" => { self.buffer.cursor.col = 0; cx.notify(); }
            "$" | "end" => { self.buffer.move_to_line_end(); cx.notify(); }
            "G" => {
                self.buffer.cursor.row = self.buffer.line_count().saturating_sub(1);
                self.buffer.cursor.col = 0;
                cx.notify();
            }
            "g" | "d" => { self.pending_key = Some(key.chars().next().unwrap_or(' ')); }
            "x" => {
                self.buffer.delete_char_at_cursor();
                self.reparse();
                cx.notify();
            }
            _ => {}
        }
    }

    fn handle_insert(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        match key.as_str() {
            "escape" => { self.mode = Mode::Normal; cx.notify(); }
            "backspace" => {
                self.buffer.backspace();
                self.reparse();
                cx.notify();
            }
            "return" | "enter" => {
                self.buffer.insert_newline_with_indent(false);
                self.reparse();
                cx.notify();
            }
            "tab" => {
                for _ in 0..4 { self.buffer.insert_char(' '); }
                self.reparse();
                cx.notify();
            }
            _ => {
                if let Some(ch) = &event.keystroke.key_char {
                    if !ch.is_empty() && ch != "\u{7f}" {
                        for c in ch.chars() {
                            self.buffer.insert_char(c);
                        }
                        self.reparse();
                        cx.notify();
                    }
                }
            }
        }
    }

    fn line_tokens(&self, row: usize) -> Vec<(usize, usize, TokenKind)> {
        self.syntax.tokens.iter()
            .filter(|(_, _, _, r)| *r == row)
            .map(|(k, s, e, _)| (*s, if *e == usize::MAX { self.buffer.line(row).len() } else { *e }, *k))
            .collect()
    }

    fn render_line(&self, row: usize) -> AnyElement {
        let line = self.buffer.line(row);
        let is_cursor = row == self.buffer.cursor.row;
        let bg = if is_cursor { self.theme.line_highlight } else { self.theme.bg };
        let mut tokens = self.line_tokens(row);
        tokens.sort_by_key(|(s, _, _)| *s);

        if tokens.is_empty() {
            return self.render_plain_line(row, is_cursor, bg);
        }

        let mut segments: Vec<(String, Hsla)> = Vec::new();
        let mut pos: usize = 0;
        for (start, end, kind) in &tokens {
            if *start > pos && pos < line.len() {
                let text: String = line.chars().skip(pos).take(start - pos).collect();
                segments.push((text, self.theme.fg));
            }
            let text: String = line.chars().skip(*start).take(end - start).collect();
            segments.push((text, self.theme.color_for(*kind)));
            pos = *end;
        }
        if pos < line.len() {
            let text: String = line.chars().skip(pos).collect();
            segments.push((text, self.theme.fg));
        }

        if !is_cursor {
            let children: Vec<AnyElement> = segments.into_iter().map(|(text, color)| {
                div().text_color(color).child(text).into_any_element()
            }).collect();
            return div().bg(bg).flex().flex_row().w_full().children(children).into_any_element();
        }

        self.render_cursor_line(segments, bg)
    }

    fn render_plain_line(&self, row: usize, is_cursor: bool, bg: Hsla) -> AnyElement {
        let line: String = self.buffer.line(row).to_string();
        if !is_cursor {
            return div().bg(bg).text_color(self.theme.fg).child(line).into_any_element();
        }
        let col = self.buffer.cursor.col.min(line.len());
        let before: String = line.chars().take(col).collect();
        let at: String = line.chars().skip(col).take(1).collect();
        let after: String = line.chars().skip(col + 1).collect();

        let mut children: Vec<AnyElement> = vec![
            div().text_color(self.theme.fg).child(before).into_any_element(),
        ];
        if self.mode == Mode::Normal {
            children.push(
                div().bg(self.theme.cursor).text_color(self.theme.bg)
                    .child(if at.is_empty() { " ".to_string() } else { at.clone() })
                    .into_any_element()
            );
        } else {
            children.push(
                div().bg(self.theme.cursor).w(px(1.5)).h(px(20.)).into_any_element()
            );
            if !at.is_empty() {
                children.push(div().text_color(self.theme.fg).child(at).into_any_element());
            }
        }
        children.push(div().text_color(self.theme.fg).child(after).into_any_element());
        div().bg(bg).flex().flex_row().w_full().children(children).into_any_element()
    }

    fn render_cursor_line(&self, segments: Vec<(String, Hsla)>, bg: Hsla) -> AnyElement {
        let col = self.buffer.cursor.col;
        let mut children: Vec<AnyElement> = Vec::new();
        let mut char_pos: usize = 0;
        let mut cursor_inserted = false;

        for (text, color) in &segments {
            let seg_len = text.chars().count();
            let seg_end = char_pos + seg_len;
            let cursor_in_seg = !cursor_inserted && col >= char_pos && col <= seg_end;

            if cursor_in_seg {
                let offset = col - char_pos;
                let before: String = text.chars().take(offset).collect();
                let at: String = text.chars().skip(offset).take(1).collect();
                let after: String = text.chars().skip(offset + 1).collect();

                if !before.is_empty() {
                    children.push(div().text_color(*color).child(before).into_any_element());
                }
                if self.mode == Mode::Normal {
                    children.push(
                        div().bg(self.theme.cursor).text_color(self.theme.bg)
                            .child(if at.is_empty() { " ".to_string() } else { at.clone() })
                            .into_any_element()
                    );
                } else {
                    children.push(
                        div().bg(self.theme.cursor).w(px(1.5)).h(px(20.)).into_any_element()
                    );
                    if !at.is_empty() {
                        children.push(div().text_color(*color).child(at).into_any_element());
                    }
                }
                if !after.is_empty() {
                    children.push(div().text_color(*color).child(after).into_any_element());
                }
                cursor_inserted = true;
            } else {
                children.push(div().text_color(*color).child(text.clone()).into_any_element());
            }
            char_pos = seg_end;
        }

        // cursor at end of line
        if !cursor_inserted {
            if self.mode == Mode::Normal {
                children.push(
                    div().bg(self.theme.cursor).child(" ").into_any_element()
                );
            } else {
                children.push(
                    div().bg(self.theme.cursor).w(px(1.5)).h(px(20.)).into_any_element()
                );
            }
        }

        div().bg(bg).flex().flex_row().w_full().children(children).into_any_element()
    }

    fn render_status_bar(&self) -> AnyElement {
        let mode_text = match self.mode {
            Mode::Normal => {
                if let Some(pk) = self.pending_key {
                    pk.to_string()
                } else {
                    "NORMAL".to_string()
                }
            }
            Mode::Insert => "INSERT".to_string(),
        };
        let file = self.filename.clone().unwrap_or_else(|| "[no name]".to_string());
        let cur = format!("{}:{}", self.buffer.cursor.row + 1, self.buffer.cursor.col + 1);
        let lang = self.ext.clone().unwrap_or_else(|| "txt".to_string());

        div().w_full().h(px(24.))
            .bg(self.theme.status_bg).text_color(self.theme.status_fg)
            .flex().flex_row().items_center().justify_between()
            .px(px(12.))
            .child(div().flex().flex_row().gap(px(16.))
                .children(vec![div().child(mode_text), div().child(file)]))
            .child(div().flex().flex_row().gap(px(16.))
                .children(vec![div().child(lang), div().child(cur)]))
            .into_any_element()
    }
}

impl Render for GuiEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let line_count = self.buffer.line_count();
        let gutter: Vec<AnyElement> = (1..=line_count).map(|n| {
            let bg = if n - 1 == self.buffer.cursor.row { self.theme.line_highlight } else { self.theme.gutter_bg };
            div().bg(bg).w(px(52.)).px(px(8.)).child(format!("{:>3} ", n)).into_any_element()
        }).collect();

        let lines: Vec<AnyElement> = (0..line_count).map(|i| self.render_line(i)).collect();

        div().flex().flex_col().size_full().bg(self.theme.bg)
            .key_context("Editor").track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key(event, window, cx);
            }))
            .child(
                div().flex().flex_row().flex_1()
                    .child(
                        div().flex().flex_col().bg(self.theme.gutter_bg)
                            .text_color(self.theme.gutter_fg).children(gutter)
                    )
                    .child(
                        div().flex().flex_col().flex_1().children(lines)
                    )
            )
            .child(self.render_status_bar())
    }
}
