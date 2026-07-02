use gpui::*;
use crate::app::{App, Mode};
use crate::highlight::TokenKind;

struct SuiteTheme {
    bg: Hsla, fg: Hsla, gutter_bg: Hsla, gutter_fg: Hsla,
    line_highlight: Hsla, cursor: Hsla, status_bg: Hsla, status_fg: Hsla,
    keyword: Hsla, string: Hsla, comment: Hsla, number: Hsla, type_name: Hsla,
    explorer_bg: Hsla, explorer_fg: Hsla, explorer_dir: Hsla,
    tab_active: Hsla, tab_inactive: Hsla,
    xlc_bg: Hsla, xlc_fg: Hsla,
}

impl Default for SuiteTheme {
    fn default() -> Self {
        SuiteTheme {
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
            explorer_bg: hsla(0.65, 0.10, 0.08, 1.0),
            explorer_fg: hsla(0.60, 0.15, 0.70, 1.0),
            explorer_dir: hsla(0.58, 0.60, 0.60, 1.0),
            tab_active: hsla(0.65, 0.18, 0.18, 1.0),
            tab_inactive: hsla(0.65, 0.10, 0.08, 1.0),
            xlc_bg: hsla(0.65, 0.10, 0.08, 1.0),
            xlc_fg: hsla(0.60, 0.15, 0.70, 1.0),
        }
    }
}

impl SuiteTheme {
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

pub struct GuiSuite {
    app: App,
    theme: SuiteTheme,
    ext: Option<String>,
    focus_handle: FocusHandle,
}

impl GuiSuite {
    pub fn new(cx: &mut Context<Self>, file_path: Option<String>) -> Self {
        let (mut app, ext) = if let Some(ref path) = file_path {
            let a = App::open_file(path);
            let ext = std::path::Path::new(path)
                .extension().and_then(|e| e.to_str()).map(|s| s.to_string());
            (a, ext)
        } else {
            (App::new(), None)
        };

        app.syntax.parse(&app.buffer.text(), ext.as_deref());
        app.explorer.refresh();

        let suite = GuiSuite {
            app,
            theme: SuiteTheme::default(),
            ext,
            focus_handle: cx.focus_handle(),
        };

        suite
    }

    fn reparse(&mut self) {
        self.app.syntax.parse(&self.app.buffer.text(), self.ext.as_deref());
        self.app.syntax.tokens.retain(|(_, _, _, row)| *row < self.app.buffer.line_count());
    }

    fn notify(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    // ── Key handling ──────────────────────────────────────

    fn on_key(&mut self, event: &KeyDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if self.app.mode == Mode::XlcInput {
            self.handle_xlc_key(event, cx);
            return;
        }

        if let Some(prefix) = self.app.pending_key.take() {
            match (prefix, event.keystroke.key.as_str()) {
                ('g', "g") => {
                    self.app.buffer.cursor.row = 0;
                    self.app.buffer.cursor.col = 0;
                    self.app.scroll = 0;
                    self.notify(cx);
                    return;
                }
                ('g', "t") => {
                    self.app.next_tab();
                    self.reparse();
                    self.notify(cx);
                    return;
                }
                ('g', "T") => {
                    self.app.prev_tab();
                    self.reparse();
                    self.notify(cx);
                    return;
                }
                _ => {}
            }
        }

        match self.app.mode {
            Mode::Normal => self.handle_normal(event, cx),
            Mode::Insert => self.handle_insert(event, cx),
            _ => {}
        }
    }

    fn handle_normal(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = event.keystroke.key.as_str();
        match k {
            "i" => self.app.mode = Mode::Insert,
            "a" => { self.app.buffer.move_right(); self.app.mode = Mode::Insert; }
            "A" => { self.app.buffer.move_to_line_end(); self.app.mode = Mode::Insert; self.notify(cx); }
            "I" => { self.app.buffer.cursor.col = 0; self.app.mode = Mode::Insert; self.notify(cx); }
            "o" => { self.app.buffer.move_to_line_end(); self.app.buffer.insert_newline_with_indent(false); self.reparse(); self.app.mode = Mode::Insert; self.notify(cx); }
            "O" => {
                if self.app.buffer.cursor.row > 0 { self.app.buffer.cursor.row -= 1; self.app.buffer.move_to_line_end(); }
                self.app.buffer.cursor.col = 0;
                self.app.buffer.insert_newline_with_indent(false);
                self.reparse(); self.app.mode = Mode::Insert; self.notify(cx);
            }
            "h" | "left" => { self.app.buffer.move_left(); self.app.update_scroll(); self.notify(cx); }
            "j" | "down" => { self.app.buffer.move_down(); self.app.update_scroll(); self.notify(cx); }
            "k" | "up" => { self.app.buffer.move_up(); self.app.update_scroll(); self.notify(cx); }
            "l" | "right" => { self.app.buffer.move_right(); self.app.update_scroll(); self.notify(cx); }
            "0" | "home" => { self.app.buffer.cursor.col = 0; self.notify(cx); }
            "$" | "end" => { self.app.buffer.move_to_line_end(); self.notify(cx); }
            "w" => { self.app.buffer.move_word_forward(); self.app.update_scroll(); self.notify(cx); }
            "b" => { self.app.buffer.move_word_back(); self.app.update_scroll(); self.notify(cx); }
            "G" => {
                self.app.buffer.cursor.row = self.app.buffer.line_count().saturating_sub(1);
                self.app.buffer.cursor.col = 0;
                self.app.update_scroll();
                self.notify(cx);
            }
            "g" | "d" => { self.app.pending_key = Some(k.chars().next().unwrap_or(' ')); }
            "x" => { self.app.buffer.delete_char_at_cursor(); self.reparse(); self.notify(cx); }
            "u" => { self.app.undo(); self.reparse(); self.notify(cx); }
            "p" => {
                if let Some(yank) = self.app.yank_buffer.clone() {
                    self.app.buffer.paste_line_after(&yank);
                    self.reparse(); self.notify(cx);
                }
            }
            "y" => {
                self.app.yank_buffer = Some(self.app.buffer.line(self.app.buffer.cursor.row).to_string());
            }
            ":" => {
                self.app.mode = Mode::XlcInput;
                self.app.xlc.open_panel(None);
                self.notify(cx);
            }
            "/" => {
                self.app.mode = Mode::XlcInput;
                self.app.xlc.open_panel(Some("/"));
                self.notify(cx);
            }
            "ctrl-f" | "f5" => {
                self.app.explorer.toggle(self.app.filename.as_ref());
                self.notify(cx);
            }
            _ => {}
        }
    }

    fn handle_insert(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = event.keystroke.key.as_str();
        match k {
            "escape" => { self.app.mode = Mode::Normal; self.notify(cx); }
            "backspace" => { self.app.buffer.backspace(); self.reparse(); self.app.update_scroll(); self.notify(cx); }
            "return" | "enter" => { self.app.buffer.insert_newline_with_indent(false); self.reparse(); self.app.update_scroll(); self.notify(cx); }
            "tab" => {
                for _ in 0..4 { self.app.buffer.insert_char(' '); }
                self.reparse(); self.app.update_scroll(); self.notify(cx);
            }
            _ => {
                if let Some(ch) = &event.keystroke.key_char {
                    if !ch.is_empty() && ch != "\u{7f}" {
                        for c in ch.chars() { self.app.buffer.insert_char(c); }
                        self.reparse(); self.app.update_scroll(); self.notify(cx);
                    }
                }
            }
        }
    }

    fn handle_xlc_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = event.keystroke.key.as_str();
        match k {
            "escape" => {
                self.app.xlc.close();
                self.app.mode = Mode::Normal;
                self.notify(cx);
            }
            "return" | "enter" => {
                self.app.execute_xlc();
                self.reparse();
                self.notify(cx);
            }
            "backspace" => {
                self.app.xlc.pop_char();
                self.notify(cx);
            }
            "up" => {
                self.app.xlc.history_up();
                self.notify(cx);
            }
            "down" => {
                self.app.xlc.history_down();
                self.notify(cx);
            }
            _ => {
                if let Some(ch) = &event.keystroke.key_char {
                    if !ch.is_empty() && ch != "\u{7f}" && ch.len() == 1 {
                        self.app.xlc.push_char(ch.chars().next().unwrap());
                        self.notify(cx);
                    }
                }
            }
        }
    }

    // ── Rendering ─────────────────────────────────────────

    fn line_tokens(&self, row: usize) -> Vec<(usize, usize, TokenKind)> {
        self.app.syntax.tokens.iter()
            .filter(|(_, _, _, r)| *r == row)
            .map(|(k, s, e, _)| {
                let line_len = self.app.buffer.line(row).len();
                (*s, if *e == usize::MAX { line_len } else { *e.min(&line_len) }, *k)
            })
            .collect()
    }

    fn render_line(&self, row: usize, offset: usize) -> AnyElement {
        let actual_row = offset + row;
        let line: String = self.app.buffer.line(actual_row).to_string();
        let is_cursor = actual_row == self.app.buffer.cursor.row;
        let bg = if is_cursor { self.theme.line_highlight } else { self.theme.bg };
        let mut tokens = self.line_tokens(actual_row);
        tokens.sort_by_key(|(s, _, _)| *s);

        if tokens.is_empty() {
            return self.render_plain_line(&line, is_cursor, bg);
        }

        let segments = self.build_segments(&line, &tokens);
        if !is_cursor {
            return self.render_colored_line(&segments, bg);
        }
        self.render_cursor_line(&segments, &line, bg)
    }

    fn build_segments(&self, line: &str, tokens: &[(usize, usize, TokenKind)]) -> Vec<(String, Hsla)> {
        let mut segments: Vec<(String, Hsla)> = Vec::new();
        let mut pos: usize = 0;
        for (start, end, kind) in tokens {
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
        segments
    }

    fn render_colored_line(&self, segments: &[(String, Hsla)], bg: Hsla) -> AnyElement {
        let children: Vec<AnyElement> = segments.iter().map(|(text, color)| {
            div().text_color(*color).child(text.clone()).into_any_element()
        }).collect();
        div().bg(bg).flex().flex_row().w_full().children(children).into_any_element()
    }

    fn render_plain_line(&self, line: &str, is_cursor: bool, bg: Hsla) -> AnyElement {
        if !is_cursor {
            return div().bg(bg).text_color(self.theme.fg).child(line.to_string()).into_any_element();
        }
        let col = self.app.buffer.cursor.col.min(line.len());
        let chars: Vec<char> = line.chars().collect();
        let before: String = chars[..col.min(chars.len())].iter().collect();
        let at: String = chars.get(col).map(|c| c.to_string()).unwrap_or_default();
        let after: String = chars.get((col+1)..).map(|s| s.iter().collect()).unwrap_or_default();

        let mut children: Vec<AnyElement> = vec![
            div().text_color(self.theme.fg).child(before).into_any_element(),
        ];
        if self.app.mode == Mode::Insert {
            children.push(div().bg(self.theme.cursor).w(px(1.5)).h(px(20.)).into_any_element());
            if !at.is_empty() { children.push(div().text_color(self.theme.fg).child(at).into_any_element()); }
        } else {
            children.push(div().bg(self.theme.cursor).text_color(self.theme.bg).child(if at.is_empty() { " ".to_string() } else { at }).into_any_element());
        }
        if !after.is_empty() { children.push(div().text_color(self.theme.fg).child(after).into_any_element()); }
        div().bg(bg).flex().flex_row().w_full().children(children).into_any_element()
    }

    fn render_cursor_line(&self, segments: &[(String, Hsla)], _line: &str, bg: Hsla) -> AnyElement {
        let col = self.app.buffer.cursor.col;
        let mut children: Vec<AnyElement> = Vec::new();
        let mut char_pos: usize = 0;
        let mut cursor_inserted = false;

        for (text, color) in segments {
            let seg_len = text.chars().count();
            let seg_end = char_pos + seg_len;
            let cursor_in_seg = !cursor_inserted && col >= char_pos && col <= seg_end;

            if cursor_in_seg {
                let offset = col - char_pos;
                let before: String = text.chars().take(offset).collect();
                let at: String = text.chars().skip(offset).take(1).collect();
                let after: String = text.chars().skip(offset + 1).collect();

                if !before.is_empty() { children.push(div().text_color(*color).child(before).into_any_element()); }
                if self.app.mode == Mode::Insert {
                    children.push(div().bg(self.theme.cursor).w(px(1.5)).h(px(20.)).into_any_element());
                    if !at.is_empty() { children.push(div().text_color(*color).child(at).into_any_element()); }
                } else {
                    children.push(div().bg(self.theme.cursor).text_color(self.theme.bg).child(if at.is_empty() { " ".to_string() } else { at }).into_any_element());
                }
                if !after.is_empty() { children.push(div().text_color(*color).child(after).into_any_element()); }
                cursor_inserted = true;
            } else {
                children.push(div().text_color(*color).child(text.clone()).into_any_element());
            }
            char_pos = seg_end;
        }

        if !cursor_inserted {
            if self.app.mode == Mode::Insert {
                children.push(div().bg(self.theme.cursor).w(px(1.5)).h(px(20.)).into_any_element());
            } else {
                children.push(div().bg(self.theme.cursor).child(" ").into_any_element());
            }
        }

        div().bg(bg).flex().flex_row().w_full().children(children).into_any_element()
    }

    fn render_tab_bar(&self) -> AnyElement {
        let tabs: Vec<AnyElement> = self.app.buffers.iter().enumerate().map(|(i, tab)| {
            let bg = if i == self.app.current_buffer { self.theme.tab_active } else { self.theme.tab_inactive };
            let name = tab.filename.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("[no name]");
            let label = if tab.modified { format!(" {}+ ", name) } else { format!(" {} ", name) };
            div().bg(bg).text_color(self.theme.fg).px(px(6.)).py(px(2.)).child(label).into_any_element()
        }).collect();

        div().bg(self.theme.tab_inactive).flex().flex_row().w_full().h(px(24.)).children(tabs).into_any_element()
    }

    fn render_explorer(&self) -> AnyElement {
        if !self.app.explorer.open {
            return div().into_any_element();
        }
        let entries: Vec<AnyElement> = self.app.explorer.entries.iter().enumerate().map(|(i, entry)| {
            let bg = if i == self.app.explorer.selected { self.theme.line_highlight } else { self.theme.explorer_bg };
            let color = if entry.is_dir { self.theme.explorer_dir } else { self.theme.explorer_fg };
            let prefix = if entry.is_dir { "📁 " } else { "📄 " };
            div().bg(bg).text_color(color).child(format!("{}{}", prefix, entry.name)).into_any_element()
        }).collect();

        div().bg(self.theme.explorer_bg).flex().flex_col().w(px(200.)).h_full().children(entries).into_any_element()
    }

    fn render_xlc(&self) -> AnyElement {
        if !self.app.xlc.open {
            return div().into_any_element();
        }
        let prompt = if self.app.search_pattern.is_none() { ":" } else { "/" };
        let input = format!("{}{}", prompt, self.app.xlc.input);
        div().w_full().bg(self.theme.xlc_bg).text_color(self.theme.xlc_fg)
            .px(px(12.)).py(px(2.)).border_b_1().border_color(self.theme.status_bg)
            .child(input).into_any_element()
    }

    fn render_status_bar(&self) -> AnyElement {
        let mode_text = match self.app.mode {
            Mode::Normal => {
                if let Some(pk) = self.app.pending_key { pk.to_string() } else { "NORMAL".to_string() }
            }
            Mode::Insert => "INSERT".to_string(),
            Mode::XlcInput => "COMMAND".to_string(),
            _ => "---".to_string(),
        };
        let file = self.app.filename.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("[no name]").to_string();
        let cur = format!("{}:{}", self.app.buffer.cursor.row + 1, self.app.buffer.cursor.col + 1);
        let lang = self.ext.clone().unwrap_or_else(|| "txt".to_string());

        div().w_full().h(px(24.)).bg(self.theme.status_bg).text_color(self.theme.status_fg)
            .flex().flex_row().items_center().justify_between().px(px(12.))
            .child(div().flex().flex_row().gap(px(16.))
                .children(vec![div().child(mode_text), div().child(file)]))
            .child(div().flex().flex_row().gap(px(16.))
                .children(vec![div().child(lang), div().child(cur)]))
            .into_any_element()
    }
}

impl Render for GuiSuite {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let line_count = self.app.buffer.line_count();
        let visible_start = self.app.scroll;
        let visible_end = (visible_start + 40).min(line_count);

        let gutter: Vec<AnyElement> = (visible_start..visible_end).map(|n| {
            let bg = if n == self.app.buffer.cursor.row { self.theme.line_highlight } else { self.theme.gutter_bg };
            div().bg(bg).w(px(52.)).px(px(8.)).child(format!("{:>3} ", n + 1)).into_any_element()
        }).collect();

        let lines: Vec<AnyElement> = (visible_start..visible_end).map(|i| self.render_line(i - visible_start, visible_start)).collect();

        let tab_bar = self.render_tab_bar();
        let explorer = self.render_explorer();
        let xlc_bar = self.render_xlc();
        let status = self.render_status_bar();

        div().flex().flex_col().size_full().bg(self.theme.bg)
            .key_context("Editor").track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.on_key(event, window, cx);
            }))
            .child(tab_bar)
            .child(
                div().flex().flex_row().flex_1()
                    .child(explorer)
                    .child(
                        div().flex().flex_row().flex_1()
                            .child(div().flex().flex_col().bg(self.theme.gutter_bg).text_color(self.theme.gutter_fg).children(gutter))
                            .child(div().flex().flex_col().flex_1().children(lines))
                    )
            )
            .child(xlc_bar)
            .child(status)
    }
}
