use gpui::*;
use crate::app::{App, Mode};
use crate::highlight::TokenKind;

struct St {
    bg: Hsla, fg: Hsla, gbg: Hsla, gfg: Hsla, hl: Hsla, cur: Hsla,
    sbar: Hsla, sf: Hsla, kw: Hsla, st: Hsla, cm: Hsla, nm: Hsla, tn: Hsla,
    ebg: Hsla, efg: Hsla, edr: Hsla, ta: Hsla, ti: Hsla,
    xbg: Hsla, xfg: Hsla, sel: Hsla, err: Hsla, warn: Hsla,
    comp_bg: Hsla, comp_sel: Hsla, comp_border: Hsla,
    term_bg: Hsla, term_fg: Hsla,
}

impl Default for St {
    fn default() -> Self { St {
        bg: hsla(0.65, 0.15, 0.12, 1.0), fg: hsla(0.60, 0.20, 0.85, 1.0),
        gbg: hsla(0.65, 0.12, 0.10, 1.0), gfg: hsla(0.60, 0.10, 0.50, 1.0),
        hl: hsla(0.65, 0.15, 0.18, 1.0), cur: hsla(0.60, 0.80, 0.70, 1.0),
        sbar: hsla(0.65, 0.25, 0.20, 1.0), sf: hsla(0.60, 0.15, 0.70, 1.0),
        kw: hsla(0.72, 0.77, 0.58, 1.0), st: hsla(0.11, 0.57, 0.60, 1.0),
        cm: hsla(0.55, 0.30, 0.45, 1.0), nm: hsla(0.17, 0.86, 0.60, 1.0),
        tn: hsla(0.28, 0.65, 0.60, 1.0),
        ebg: hsla(0.65, 0.10, 0.08, 1.0), efg: hsla(0.60, 0.15, 0.70, 1.0),
        edr: hsla(0.58, 0.60, 0.60, 1.0),
        ta: hsla(0.65, 0.18, 0.18, 1.0), ti: hsla(0.65, 0.10, 0.08, 1.0),
        xbg: hsla(0.65, 0.10, 0.08, 1.0), xfg: hsla(0.60, 0.15, 0.70, 1.0),
        sel: hsla(0.67, 0.35, 0.30, 1.0), err: hsla(0.0, 0.70, 0.55, 1.0),
        warn: hsla(0.14, 0.70, 0.55, 1.0),
        comp_bg: hsla(0.65, 0.18, 0.16, 1.0), comp_sel: hsla(0.72, 0.30, 0.30, 1.0),
        comp_border: hsla(0.67, 0.35, 0.40, 1.0),
        term_bg: hsla(0.65, 0.08, 0.06, 1.0), term_fg: hsla(0.60, 0.20, 0.85, 1.0),
    }}
}

impl St {
    fn c(&self, k: TokenKind) -> Hsla { match k { TokenKind::Keyword=>self.kw,TokenKind::String=>self.st,TokenKind::Comment=>self.cm,TokenKind::Number=>self.nm,TokenKind::TypeName=>self.tn }}
    fn dc(&self, s: &str) -> Hsla { if s == "error" { self.err } else { self.warn }}
}

pub struct Suisei {
    app: App,
    t: St,
    ext: Option<String>,
    fh: FocusHandle,
}

impl Suisei {
    pub fn new(cx: &mut Context<Self>, fp: Option<String>) -> Self {
        let (mut app, ext) = if let Some(ref p) = fp {
            let a = App::open_file(p);
            let e = std::path::Path::new(p).extension().and_then(|e| e.to_str()).map(|s| s.to_string());
            (a, e)
        } else { (App::new(), None) };
        app.syntax.parse(&app.buffer.text(), ext.as_deref());
        app.explorer.refresh();
        let mut s = Self { app, t: St::default(), ext, fh: cx.focus_handle() };
        s.start_lsp(cx);
        s
    }

    fn start_lsp(&mut self, _cx: &mut Context<Self>) {
        if let Some(ref p) = self.app.filename {
            let path = p.display().to_string();
            self.app.lsp.auto_start(&path);
        }
    }

    fn rp(&mut self) { self.app.syntax.parse(&self.app.buffer.text(), self.ext.as_deref()); self.app.syntax.tokens.retain(|(_,_,_,r)| *r < self.app.buffer.line_count()); }

    fn notify(&mut self, cx: &mut Context<Self>) { cx.notify(); }

    fn is_selected(&self, row: usize, col: usize) -> bool {
        if self.app.mode != Mode::Visual && self.app.mode != Mode::VisualLine { return false; }
        let anchor = match self.app.visual_anchor { Some(a) => a, None => return false };
        let cur = self.app.buffer.cursor;
        let (sr, sc) = if anchor.row < cur.row || (anchor.row == cur.row && anchor.col <= cur.col) { (anchor.row, anchor.col) } else { (cur.row, cur.col) };
        let (er, ec) = if anchor.row > cur.row || (anchor.row == cur.row && anchor.col > cur.col) { (anchor.row, anchor.col) } else { (cur.row, cur.col) };
        if self.app.mode == Mode::VisualLine { row >= sr && row <= er }
        else if row > sr && row < er { true }
        else if row == sr && row == er { col >= sc && col <= ec }
        else if row == sr { col >= sc }
        else if row == er { col <= ec }
        else { false }
    }

    fn handle_key(&mut self, e: &KeyDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if self.app.mode == Mode::XlcInput { self.handle_xlc(e, cx); return; }

        if let Some(px) = self.app.pending_key.take() {
            match (px, e.keystroke.key.as_str()) {
                ('g', "g") => { self.app.buffer.cursor.row=0; self.app.buffer.cursor.col=0; self.app.scroll=0; self.notify(cx); return; }
                ('g', "t") => { self.app.next_tab(); self.rp(); self.app.lsp_restart_for_current(); self.notify(cx); return; }
                ('g', "T") => { self.app.prev_tab(); self.rp(); self.app.lsp_restart_for_current(); self.notify(cx); return; }
                _ => {}
            }
        }

        match self.app.mode {
            Mode::Normal => self.hn(e, cx),
            Mode::Insert => self.hi(e, cx),
            Mode::Visual | Mode::VisualLine => self.hv(e, cx),
            _ => {}
        }
        // Poll LSP and terminal after every key event
        self.app.lsp.poll();
        if self.app.terminal.open { self.app.terminal.poll(); }
        cx.notify();
    }

    fn hn(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = e.keystroke.key.as_str();
        match k {
            "i"=>{self.app.mode=Mode::Insert;}
            "a"=>{self.app.buffer.move_right();self.app.mode=Mode::Insert;}
            "A"=>{self.app.buffer.move_to_line_end();self.app.mode=Mode::Insert;self.notify(cx);}
            "I"=>{self.app.buffer.cursor.col=0;self.app.mode=Mode::Insert;self.notify(cx);}
            "o"=>{self.app.buffer.move_to_line_end();self.app.buffer.insert_newline_with_indent(false);self.rp();self.app.mode=Mode::Insert;self.notify(cx);}
            "O"=>{if self.app.buffer.cursor.row>0{self.app.buffer.cursor.row-=1;self.app.buffer.move_to_line_end();}self.app.buffer.cursor.col=0;self.app.buffer.insert_newline_with_indent(false);self.rp();self.app.mode=Mode::Insert;self.notify(cx);}
            "v"=>{self.app.enter_visual();self.notify(cx);}
            "V"=>{self.app.enter_visual_line();self.notify(cx);}
            "h"|"left"=>{self.app.buffer.move_left();self.app.update_scroll();self.notify(cx);}
            "j"|"down"=>{self.app.buffer.move_down();self.app.update_scroll();self.notify(cx);}
            "k"|"up"=>{self.app.buffer.move_up();self.app.update_scroll();self.notify(cx);}
            "l"|"right"=>{self.app.buffer.move_right();self.app.update_scroll();self.notify(cx);}
            "0"|"home"=>{self.app.buffer.cursor.col=0;self.notify(cx);}
            "$"|"end"=>{self.app.buffer.move_to_line_end();self.notify(cx);}
            "w"=>{self.app.buffer.move_word_forward();self.app.update_scroll();self.notify(cx);}
            "b"=>{self.app.buffer.move_word_back();self.app.update_scroll();self.notify(cx);}
            "G"=>{self.app.buffer.cursor.row=self.app.buffer.line_count().saturating_sub(1);self.app.buffer.cursor.col=0;self.app.update_scroll();self.notify(cx);}
            "g"|"d"|"y"=>{self.app.pending_key=Some(k.chars().next().unwrap_or(' '));}
            "x"=>{
                if self.app.buffer.cursor.col < self.app.buffer.line(self.app.buffer.cursor.row).len() {
                    self.app.buffer.delete_char_at_cursor();
                }
                self.rp();self.notify(cx);
            }
            "u"=>{self.app.undo();self.rp();self.notify(cx);}
            "p"=>{if let Some(ref yb)=self.app.yank_buffer.clone(){self.app.buffer.paste_line_after(yb);self.rp();self.notify(cx);}}
            ":"=>{self.app.mode=Mode::XlcInput;self.app.xlc.open_panel(None);self.notify(cx);}
            "/"=>{self.app.mode=Mode::XlcInput;self.app.xlc.open_panel(Some("/"));self.notify(cx);}
            "ctrl-f"|"f5"=>{self.app.explorer.toggle(self.app.filename.as_ref());self.notify(cx);}
            "ctrl-t"|"f12"=>{
                if self.app.terminal.open { self.app.terminal.shutdown(); self.app.terminal.open=false; }
                else { self.app.terminal.open=true; self.app.terminal.start(self.app.filename.as_ref()); }
                self.notify(cx);
            }
            _ => {}
        }
    }

    fn hi(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = e.keystroke.key.as_str();
        match k {
            "escape"=>{self.app.mode=Mode::Normal;if self.app.buffer.cursor.col>0{self.app.buffer.cursor.col-=1;}self.notify(cx);}
            "backspace"=>{self.app.buffer.backspace();self.rp();self.app.update_scroll();self.notify(cx);}
            "return"|"enter"=>{self.app.buffer.insert_newline_with_indent(false);self.rp();self.app.update_scroll();self.notify(cx);}
            "tab"=>{for _ in 0..4{self.app.buffer.insert_char(' ');}self.rp();self.app.update_scroll();self.notify(cx);}
            _ => { if let Some(ch)=&e.keystroke.key_char { if !ch.is_empty()&&ch!="\u{7f}" { for c in ch.chars(){self.app.buffer.insert_char(c);} self.rp();self.app.update_scroll();self.notify(cx); } } }
        }
    }

    fn hv(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = e.keystroke.key.as_str();
        match k {
            "escape"=>{self.app.mode=Mode::Normal;self.app.visual_anchor=None;self.notify(cx);}
            "h"|"left"=>{self.app.buffer.move_left();self.app.update_scroll();self.notify(cx);}
            "j"|"down"=>{self.app.buffer.move_down();self.app.update_scroll();self.notify(cx);}
            "k"|"up"=>{self.app.buffer.move_up();self.app.update_scroll();self.notify(cx);}
            "l"|"right"=>{self.app.buffer.move_right();self.app.update_scroll();self.notify(cx);}
            "w"=>{self.app.buffer.move_word_forward();self.app.update_scroll();self.notify(cx);}
            "b"=>{self.app.buffer.move_word_back();self.app.update_scroll();self.notify(cx);}
            "0"|"home"=>{self.app.buffer.cursor.col=0;self.notify(cx);}
            "$"|"end"=>{self.app.buffer.move_to_line_end();self.notify(cx);}
            "G"=>{self.app.buffer.cursor.row=self.app.buffer.line_count().saturating_sub(1);self.app.buffer.cursor.col=0;self.app.update_scroll();self.notify(cx);}
            "y"=>{self.app.yank_selection();self.app.mode=Mode::Normal;self.notify(cx);}
            "d"=>{self.app.delete_selection();self.rp();self.app.mode=Mode::Normal;self.notify(cx);}
            _ => {}
        }
    }

    fn handle_xlc(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = e.keystroke.key.as_str();
        match k {
            "escape"=>{self.app.xlc.close();self.app.mode=Mode::Normal;self.notify(cx);}
            "return"|"enter"=>{self.app.execute_xlc();self.rp();self.notify(cx);}
            "backspace"=>{self.app.xlc.pop_char();self.notify(cx);}
            "up"=>{self.app.xlc.history_up();self.notify(cx);}
            "down"=>{self.app.xlc.history_down();self.notify(cx);}
            _ => { if let Some(ch)=&e.keystroke.key_char { if !ch.is_empty()&&ch!="\u{7f}"&&ch.len()==1 { self.app.xlc.push_char(ch.chars().next().unwrap()); self.notify(cx); } } }
        }
    }

    // ── RENDER ──────────────────────────────────────────

    fn lt(&self, row: usize) -> Vec<(usize,usize,TokenKind)> {
        self.app.syntax.tokens.iter().filter(|(_,_,_,r)|*r==row).map(|(k,s,e,_)|(*s,if *e==usize::MAX{self.app.buffer.line(row).len()}else{*e.min(&self.app.buffer.line(row).len())},*k)).collect()
    }

    fn segs(&self, line: &str, tokens: &[(usize,usize,TokenKind)]) -> Vec<(String,Hsla)> {
        let mut v=Vec::new();let mut p:usize=0;
        for (s,e,k) in tokens {
            if *s>p&&p<line.len(){v.push((line.chars().skip(p).take(s-p).collect(),self.t.fg));}
            v.push((line.chars().skip(*s).take(e-s).collect(),self.t.c(*k)));p=*e;
        }
        if p<line.len(){v.push((line.chars().skip(p).collect(),self.t.fg));}
        v
    }

    fn render_line(&self, row: usize) -> AnyElement {
        let line: String = self.app.buffer.line(row).to_string();
        let is_cursor = row == self.app.buffer.cursor.row;
        let mut bg = self.t.bg;
        if is_cursor { bg = self.t.hl; }
        if self.is_selected(row, 0) { bg = self.t.sel; }

        // LSP diagnostic marker
        let diags: Vec<_> = self.app.lsp.diagnostics.iter().filter(|d|d.row==row).collect();
        let gutter_mark = if !diags.is_empty() {
            let sev = if diags.iter().any(|d|d.severity==crate::lsp::DiagnosticSeverity::Error) {"error"}else{"warning"};
            let color = self.t.dc(sev);
            Some((sev.to_string(), color))
        } else { None };

        let mut tokens = self.lt(row); tokens.sort_by_key(|(s,_,_)|*s);

        let segments = if tokens.is_empty() { vec![(line.clone(), self.t.fg)] } else { self.segs(&line, &tokens) };

        // Colored segments for the whole line
        let mut seg_elements: Vec<AnyElement> = Vec::new();
        let mut char_pos: usize = 0;
        let mut cursor_inserted = false;
        let col = self.app.buffer.cursor.col;

        for (text, color) in &segments {
            let seg_len = text.chars().count();
            let seg_end = char_pos + seg_len;
            let cursor_in_seg = is_cursor && !cursor_inserted && col >= char_pos && col <= seg_end;

            if cursor_in_seg {
                let off = col - char_pos;
                let bf: String = text.chars().take(off).collect();
                let at: String = text.chars().skip(off).take(1).collect();
                let af: String = text.chars().skip(off + 1).collect();
                if !bf.is_empty() { seg_elements.push(div().text_color(*color).child(bf).into_any_element()); }
                if self.app.mode == Mode::Insert {
                    seg_elements.push(div().bg(self.t.cur).w(px(1.5)).h(px(20.)).into_any_element());
                    if !at.is_empty() { seg_elements.push(div().text_color(*color).child(at).into_any_element()); }
                } else {
                    seg_elements.push(div().bg(self.t.cur).text_color(self.t.bg).child(if at.is_empty(){" ".to_string()}else{at}).into_any_element());
                }
                if !af.is_empty() { seg_elements.push(div().text_color(*color).child(af).into_any_element()); }
                cursor_inserted = true;
            } else {
                seg_elements.push(div().text_color(*color).child(text.clone()).into_any_element());
            }
            char_pos = seg_end;
        }

        if is_cursor && !cursor_inserted {
            if self.app.mode == Mode::Insert { seg_elements.push(div().bg(self.t.cur).w(px(1.5)).h(px(20.)).into_any_element()); }
            else { seg_elements.push(div().bg(self.t.cur).child(" ").into_any_element()); }
        }

        let line_div = div().bg(bg).flex().flex_row().w_full().children(seg_elements);

        // Diagnostic underline decoration
        if let Some((_, _color)) = gutter_mark {
            let underline = div().bg(self.t.err).w_full().h(px(1.));
            return div().flex().flex_col().w_full().child(line_div).child(underline).into_any_element();
        }

        line_div.into_any_element()
    }

    fn render_completion(&self) -> AnyElement {
        if !self.app.completions.active || self.app.completions.suggestions.is_empty() { return div().into_any_element(); }
        let max_h = 10.min(self.app.completions.suggestions.len());
        let items: Vec<AnyElement> = self.app.completions.suggestions.iter().enumerate().take(max_h).map(|(i,s)|{
            let bg = if i==self.app.completions.selected {self.t.comp_sel}else{self.t.comp_bg};
            div().bg(bg).px(px(8.)).py(px(2.)).child(format!("{} — {}", s.label, s.detail)).into_any_element()
        }).collect();
        div().absolute().bottom(px(40.)).left(px(100.)).bg(self.t.comp_bg).border_1().border_color(self.t.comp_border).rounded_md().overflow_hidden().w(px(300.)).children(items).into_any_element()
    }

    fn render_terminal(&self) -> AnyElement {
        if !self.app.terminal.open { return div().into_any_element(); }
        let raw = self.app.terminal.visible_rows();
        let rows: Vec<AnyElement> = raw.iter().map(|cells| {
            let cells_el: Vec<AnyElement> = cells.iter().map(|(ch, fg, _)| {
                let color = match fg { Some(c) => { let l = rat_to_hsla(c); Hsla { h:0., s:0., l, a:1. } }, None => self.t.term_fg };
                div().text_color(color).child(ch.clone()).into_any_element()
            }).collect();
            div().flex().flex_row().font_family("Buffer").children(cells_el).into_any_element()
        }).collect();
        div().bg(self.t.term_bg).h(px(200.)).flex().flex_col().children(rows).into_any_element()
    }

    fn render_tab_bar(&self) -> AnyElement {
        let tabs: Vec<AnyElement> = self.app.buffers.iter().enumerate().map(|(i,tb)|{
            let bg = if i==self.app.current_buffer{self.t.ta}else{self.t.ti};
            let name = tb.filename.as_ref().and_then(|p|p.file_name()).and_then(|n|n.to_str()).unwrap_or("[no name]");
            let label = if tb.modified { format!(" {}+ ",name) } else { format!(" {} ",name) };
            div().bg(bg).text_color(self.t.fg).px(px(6.)).py(px(2.)).child(label).into_any_element()
        }).collect();
        div().bg(self.t.ti).flex().flex_row().w_full().h(px(24.)).children(tabs).into_any_element()
    }

    fn render_explorer(&self) -> AnyElement {
        if !self.app.explorer.open { return div().into_any_element(); }
        let entries: Vec<AnyElement> = self.app.explorer.entries.iter().enumerate().map(|(i,e)|{
            let bg = if i==self.app.explorer.selected{self.t.hl}else{self.t.ebg};
            let color = if e.is_dir{self.t.edr}else{self.t.efg};
            let icon = if e.is_dir { "\u{1f4c1} " } else { "\u{1f4c4} " };
            div().bg(bg).text_color(color).child(format!("{}{}", icon, e.name)).into_any_element()
        }).collect();
        div().bg(self.t.ebg).flex().flex_col().w(px(200.)).h_full().children(entries).into_any_element()
    }

    fn render_xlc(&self) -> AnyElement {
        if !self.app.xlc.open { return div().into_any_element(); }
        let prompt = if self.app.search_pattern.is_none() { ":" } else { "/" };
        let input = format!("{}{}", prompt, self.app.xlc.input);
        div().w_full().bg(self.t.xbg).text_color(self.t.xfg).px(px(12.)).py(px(2.)).border_b_1().border_color(self.t.sbar).child(input).into_any_element()
    }

    fn render_status_bar(&self) -> AnyElement {
        let mode = match self.app.mode {
            Mode::Normal=>if let Some(pk)=self.app.pending_key{pk.to_string()}else{"NORMAL".to_string()},
            Mode::Insert=>"INSERT".to_string(), Mode::Visual=>"VISUAL".to_string(), Mode::VisualLine=>"V-LINE".to_string(),
            Mode::XlcInput=>"COMMAND".to_string(), _=>"---".to_string(),
        };
        let file = self.app.filename.as_ref().and_then(|p|p.file_name()).and_then(|n|n.to_str()).unwrap_or("[no name]").to_string();
        let cur = format!("{}:{}", self.app.buffer.cursor.row+1, self.app.buffer.cursor.col+1);
        let lang = self.ext.clone().unwrap_or_else(||"txt".to_string());
        let diags = self.app.lsp.diagnostics.len();
        let diag_text = if diags>0{format!(" {} issues",diags)}else{String::new()};

        div().w_full().h(px(24.)).bg(self.t.sbar).text_color(self.t.sf)
            .flex().flex_row().items_center().justify_between().px(px(12.))
            .child(div().flex().flex_row().gap(px(16.))
                .children(vec![div().child(mode), div().child(file)]))
            .child(div().flex().flex_row().gap(px(16.))
                .children(vec![div().child(lang), div().child(diag_text), div().child(cur)]))
            .into_any_element()
    }
}

impl Render for Suisei {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lc = self.app.buffer.line_count();
        let vs = self.app.scroll;
        let ve = (vs + 40).min(lc);
        let gutter: Vec<AnyElement> = (vs..ve).map(|n|{
            let has_diag = self.app.lsp.diagnostics.iter().any(|d|d.row==n);
            let color = if has_diag { self.t.err } else { self.t.gfg };
            let bg = if n==self.app.buffer.cursor.row{self.t.hl}else{self.t.gbg};
            div().bg(bg).w(px(52.)).px(px(8.)).text_color(color).child(format!("{:>3} ", n+1)).into_any_element()
        }).collect();
        let lines: Vec<AnyElement> = (vs..ve).map(|i| self.render_line(i)).collect();

        div().flex().flex_col().size_full().bg(self.t.bg)
            .key_context("Suisei").track_focus(&self.fh)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, w, cx| this.handle_key(e, w, cx)))
            .child(self.render_tab_bar())
            .child(div().flex().flex_row().flex_1()
                .child(self.render_explorer())
                .child(div().flex().flex_row().flex_1()
                    .child(div().flex().flex_col().bg(self.t.gbg).children(gutter))
                    .child(div().flex().flex_col().flex_1().children(lines))
                )
            )
            .child(self.render_xlc())
            .child(self.render_terminal())
            .child(self.render_completion())
            .child(self.render_status_bar())
    }
}

fn rat_to_hsla(c: &ratatui::style::Color) -> f32 {
    match c { ratatui::style::Color::White|ratatui::style::Color::Gray|ratatui::style::Color::LightGreen|ratatui::style::Color::LightYellow|ratatui::style::Color::LightCyan=>0.85, ratatui::style::Color::Black|ratatui::style::Color::DarkGray=>0.1, ratatui::style::Color::Red|ratatui::style::Color::LightRed=>0.55, ratatui::style::Color::Green=>0.55, ratatui::style::Color::Yellow=>0.85, ratatui::style::Color::Blue|ratatui::style::Color::LightBlue=>0.55, ratatui::style::Color::Magenta|ratatui::style::Color::LightMagenta=>0.55, ratatui::style::Color::Cyan=>0.55, ratatui::style::Color::Rgb(_,_,_)|ratatui::style::Color::Indexed(_)=>0.85, _=>0.85 }
}
