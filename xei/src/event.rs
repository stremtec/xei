use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use std::io;

use xei_core::app::{App, Mode, ResizeTarget};
use xei_core::buffer::Position;

pub fn handle_events(app: &mut App) -> io::Result<bool> {
    if !event::poll(std::time::Duration::from_millis(10))? {
        return Ok(app.running);
    }
    loop {
        match event::read()? {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Press {
                    handle_key(app, key.code, key.modifiers);
                }
            }
            Event::Mouse(mouse) => {
                handle_mouse(app, mouse.kind, mouse.column, mouse.row, mouse.modifiers);
            }
            Event::Resize(_w, _h) => {}
            _ => {}
        }
        if !event::poll(std::time::Duration::from_millis(0))? {
            break;
        }
    }
    Ok(app.running)
}
fn handle_mouse(
    app: &mut App,
    kind: MouseEventKind,
    column: u16,
    row: u16,
    _modifiers: KeyModifiers,
) {
    if app.viewport.height == 0 || app.viewport.width == 0 {
        return;
    }

    match kind {
        MouseEventKind::ScrollUp => {
            app.scroll = app.scroll.saturating_sub(3);
        }
        MouseEventKind::ScrollDown => {
            let line_count = app.buffer.line_count();
            let visible = app.viewport.height as usize;
            if app.scroll + visible < line_count {
                app.scroll = (app.scroll + 3).min(line_count.saturating_sub(visible));
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if is_on_separator(app, column, row) {
                return;
            }
            let pos = screen_to_buffer_clamped(app, column, row);
            app.buffer.cursor = pos;
            app.mouse.dragging = true;
            app.mouse.drag_anchor = Some(pos);
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(target) = app.resize_target {
                match target {
                    ResizeTarget::Explorer => {
                        app.explorer_width = column.max(8).min(60);
                    }
                    ResizeTarget::Terminal => {
                        let new_width = app.screen_width.saturating_sub(column).max(10).min(60);
                        app.terminal_width = new_width;
                    }
                    ResizeTarget::Xlc => {
                        let new_height = app.screen_height.saturating_sub(row).saturating_sub(1).max(5).min(30);
                        app.xlc_height = new_height;
                    }
                }
                return;
            }

            if app.mouse.dragging {
                if matches!(app.mode, Mode::Normal | Mode::Visual | Mode::VisualLine)
                    && !matches!(app.mode, Mode::Visual)
                {
                    app.visual_anchor = app.mouse.drag_anchor;
                    app.mode = Mode::Visual;
                    app.message = String::from("-- VISUAL --");
                    app.completions.deactivate();
                }
                edge_scroll(app, row);
                let pos = screen_to_buffer_clamped(app, column, row);
                app.buffer.cursor = pos;
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.mouse.dragging = false;
            app.mouse.drag_anchor = None;
            app.resize_target = None;
        }
        _ => {}
    }
}

fn is_on_separator(app: &mut App, column: u16, row: u16) -> bool {
    const HIT_MARGIN: u16 = 3;

    if app.explorer.open {
        let sep = app.explorer_separator_x;
        if column >= sep.saturating_sub(HIT_MARGIN) && column <= sep.saturating_add(HIT_MARGIN) {
            app.resize_target = Some(ResizeTarget::Explorer);
            app.mouse.dragging = false;
            return true;
        }
    }

    if app.terminal.open {
        let sep = app.terminal_separator_x;
        if column >= sep.saturating_sub(HIT_MARGIN) && column <= sep.saturating_add(HIT_MARGIN) {
            app.resize_target = Some(ResizeTarget::Terminal);
            app.mouse.dragging = false;
            return true;
        }
    }

    if app.xlc.open {
        let sep = app.xlc_separator_y;
        if row >= sep.saturating_sub(HIT_MARGIN) && row <= sep.saturating_add(HIT_MARGIN) {
            app.resize_target = Some(ResizeTarget::Xlc);
            app.mouse.dragging = false;
            return true;
        }
    }

    false
}

fn screen_to_buffer_clamped(app: &App, column: u16, row: u16) -> Position {
    let vp = app.viewport;
    let text_x = vp.x + LINE_NO_WIDTH;
    let max_x = vp.x + vp.width.saturating_sub(1);
    let max_y = vp.y + vp.height.saturating_sub(1);

    let clamped_col = column.max(text_x).min(max_x);
    let clamped_row = row.max(vp.y).min(max_y);

    let col = (clamped_col.saturating_sub(text_x)) as usize;
    let visible_row = (clamped_row.saturating_sub(vp.y)) as usize;
    let buffer_row = (visible_row + app.scroll).min(app.buffer.line_count().saturating_sub(1));
    let col = app.buffer.screen_col_to_buffer_col(buffer_row, col);
    let max_col = app.buffer.line(buffer_row).chars().count();

    Position::new(buffer_row, col.min(max_col))
}

fn edge_scroll(app: &mut App, row: u16) {
    let vp = app.viewport;
    let line_count = app.buffer.line_count();
    let visible = vp.height as usize;

    if row <= vp.y && app.scroll > 0 {
        app.scroll = app.scroll.saturating_sub(1);
    } else if row >= vp.y + vp.height.saturating_sub(1) && app.scroll + visible < line_count {
        app.scroll += 1;
    }
}

const LINE_NO_WIDTH: u16 = 5;

fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    if modifiers.contains(KeyModifiers::SUPER) {
        match code {
            KeyCode::Char('c') => {
                if matches!(app.mode, Mode::Visual | Mode::VisualLine) {
                    app.yank_selection();
                    if let Some(ref text) = app.yank_buffer {
                        xei_core::clipboard::copy(text);
                    }
                    app.message = String::from("Copied to clipboard");
                }
                return;
            }
            KeyCode::Char('v') => {
                if let Some(text) = xei_core::clipboard::paste() {
                    app.push_undo();
                    for ch in text.chars() {
                        if ch == '\n' {
                            app.buffer.insert_newline();
                        } else if ch == '\r' {
                        } else {
                            app.buffer.insert_char(ch);
                        }
                    }
                    app.message = String::from("Pasted from clipboard");
                }
                return;
            }
            _ => {}
        }
    }

    if code == KeyCode::F(12) {
        if app.terminal.open {
            app.terminal.open = false;
            app.terminal.shutdown();
            app.mode = Mode::Normal;
        } else {
            app.terminal.open = true;
            app.terminal.start(app.filename.as_ref());
            app.mode = Mode::Terminal;
        }
        return;
    }

    if modifiers.contains(KeyModifiers::CONTROL) {
        let ctrl_char = match code {
            KeyCode::Char(c) => c,
            _ => { /* fall through */ '?' },
        };

        if ctrl_char == 'q' {
            if app.terminal.open {
                app.terminal.open = false;
                app.terminal.shutdown();
                app.mode = Mode::Normal;
            }
            return;
        }

        if ctrl_char == 't' {
            if app.terminal.open {
                app.terminal.open = false;
                app.terminal.shutdown();
                app.mode = Mode::Normal;
            } else {
                app.terminal.open = true;
                app.terminal.start(app.filename.as_ref());
                app.mode = Mode::Terminal;
            }
            return;
        }

        if app.mode != Mode::Terminal {
            match code {
            KeyCode::Char('e') => {
                if app.mode == Mode::XlcInput || app.mode == Mode::Search {
                    app.close_xlc();
                    app.enter_normal();
                } else {
                    app.enter_xlc(None);
                }
                return;
            }
            KeyCode::Char('a') => {
                if app.mode == Mode::Insert {
                    trigger_completion(app);
                }
                return;
            }
            KeyCode::Char('u') => {
                if app.mode == Mode::XlcInput {
                    app.xlc.scroll_up(3);
                    return;
                }
            }
            KeyCode::Char('d') => {
                if app.mode == Mode::XlcInput {
                    app.xlc.scroll_down(3);
                    return;
                }
            }
            KeyCode::Char('b') => {
                if app.mode == Mode::XlcInput {
                    app.xlc.scroll_up(8);
                    return;
                }
            }
            KeyCode::Char('f') => {
                if app.mode == Mode::XlcInput {
                    app.xlc.scroll_down(8);
                } else if matches!(app.mode, Mode::Normal | Mode::Insert | Mode::Explorer) {
                    if app.explorer.open {
                        app.explorer.close();
                        app.mode = Mode::Normal;
                    } else {
                        app.explorer.toggle_at(app.filename.as_ref());
                        app.mode = Mode::Explorer;
                    }
                }
                return;
            }
            _ => {}
        }
        } else if let KeyCode::Char(c) = code {
            let ctrl_byte = if c.is_ascii_lowercase() {
                c as u8 - b'a' + 1
            } else {
                c as u8
            };
            app.terminal.write_input(&[ctrl_byte]);
            return;
        }
    }

    match app.mode {
        Mode::Normal => handle_normal(app, code),
        Mode::Insert => handle_insert(app, code),
        Mode::Visual | Mode::VisualLine => handle_visual(app, code),
        Mode::XlcInput => handle_xlc(app, code),
        Mode::Search => handle_search_input(app, code),
        Mode::Explorer => handle_explorer(app, code),
        Mode::Terminal => handle_terminal(app, code),
    }
}

fn handle_normal(app: &mut App, code: KeyCode) {
    if let Some(pending) = app.pending_key.take() {
        handle_pending(app, pending, code);
        return;
    }

    // Handle pending f/F/t/T/r and digit count accumulation
    if let KeyCode::Char(c) = code {
        // f/F/t/T/r: pending char target
        if let Some(op) = app.pending_ft.take() {
            match op {
                'f' => app.buffer.find_char_forward(c),
                'F' => app.buffer.find_char_backward(c),
                't' => app.buffer.till_char_forward(c),
                'T' => app.buffer.till_char_backward(c),
                'r' => app.buffer.replace_char(c),
                _ => {}
            }
            app.update_scroll();
            return;
        }
        // Count accumulation
        if c.is_ascii_digit() && c != '0' {
            let d = c.to_digit(10).unwrap() as usize;
            app.count = Some(app.count.unwrap_or(0) * 10 + d);
            return;
        }
        // If count was set and we get a non-digit, apply count to the command
        let n = app.count.take().unwrap_or(1);
        match c {
            'f' | 'F' | 't' | 'T' | 'r' => {
                app.pending_ft = Some(c);
                return;
            }
            'c' => {
                app.push_undo();
                app.mode = Mode::Insert;
            }
            'C' => {
                app.push_undo();
                let pos = app.buffer.cursor;
                let col = pos.col;
                let line = app.buffer.line(pos.row).to_string();
                if col < line.len() {
                    app.buffer.set_line(pos.row, line[..col].to_string());
                }
                app.mode = Mode::Insert;
                return;
            }
            'h' | 'j' | 'k' | 'l' | 'w' | 'b' => {
                for _ in 0..n {
                    match c {
                        'h' => app.buffer.move_left(),
                        'j' => app.buffer.move_down(),
                        'k' => app.buffer.move_up(),
                        'l' => app.buffer.move_right(),
                        'w' => app.buffer.move_word_forward(),
                        'b' => app.buffer.move_word_back(),
                        _ => {}
                    }
                }
                app.update_scroll();
                return;
            }
            'x' => {
                app.push_undo();
                for _ in 0..n {
                    if app.buffer.cursor.col < app.buffer.line(app.buffer.cursor.row).len() {
                        app.buffer.delete_char_at_cursor();
                    }
                }
                return;
            }
            'd' => { app.pending_key = Some('d'); app.count = Some(n); return; }
            'y' => { app.pending_key = Some('y'); app.count = Some(n); return; }
            'g' => { app.pending_key = Some('g'); app.count = Some(n); return; }
            _ => {}
        }
    }

    match code {
        KeyCode::Char(':') => app.enter_xlc(None),
        KeyCode::Char('/') => app.enter_search(),
        KeyCode::Char('i') => app.enter_insert(),
        KeyCode::Char('a') => {
            app.buffer.move_right();
            app.enter_insert();
        }
        KeyCode::Char('A') => {
            app.buffer.move_to_line_end();
            app.enter_insert();
        }
        KeyCode::Char('v') => app.enter_visual(),
        KeyCode::Char('V') => app.enter_visual_line(),
        KeyCode::Char('h') | KeyCode::Left => app.move_left(),
        KeyCode::Char('l') | KeyCode::Right => app.move_right(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('0') => app.buffer.move_to_line_start(),
        KeyCode::Char('$') => app.buffer.move_to_line_end(),
        KeyCode::Char('^') => app.buffer.move_to_first_non_blank(),
        KeyCode::Char('J') => {
            app.push_undo();
            app.buffer.join_lines();
        }
        KeyCode::Char('>') => {
            app.push_undo();
            app.buffer.indent_line();
            app.buffer.move_to_first_non_blank();
        }
        KeyCode::Char('<') => {
            app.push_undo();
            app.buffer.dedent_line();
            app.buffer.move_to_first_non_blank();
        }
        KeyCode::Char('P') => app.paste(),
        KeyCode::Char('D') => {
            app.push_undo();
            let col = app.buffer.cursor.col;
            let row = app.buffer.cursor.row;
            let line_str = app.buffer.line(row);
            if col < line_str.len() {
                let line = app.buffer.line(row).to_string();
                let prefix = &line[..col];
                app.buffer.set_line(row, prefix.to_string());
            }
        }
        KeyCode::Char('e') => {
            let line = app.buffer.line(app.buffer.cursor.row).to_string();
            let chars: Vec<char> = line.chars().collect();
            let mut i = app.buffer.cursor.col;
            while i < chars.len().saturating_sub(1) && !chars[i+1].is_alphanumeric() { i += 1; }
            while i < chars.len().saturating_sub(1) && chars[i+1].is_alphanumeric() { i += 1; }
            app.buffer.cursor.col = (i + 1).min(chars.len());
            app.update_scroll();
        }
        KeyCode::Char('w') => app.buffer.move_word_forward(),
        KeyCode::Char('b') => app.buffer.move_word_back(),
        KeyCode::Char('o') => {
            app.push_undo();
            app.buffer.move_to_line_end();
            app.buffer.insert_newline();
            app.enter_insert();
        }
        KeyCode::Char('O') => {
            app.push_undo();
            let row = app.buffer.cursor.row;
            let indent = app.buffer.leading_indent(row);
            app.buffer.insert_line_at(row, indent);
            app.enter_insert();
        }
        KeyCode::Char('x') => {
            app.push_undo();
            app.buffer.delete_char_at_cursor();
        }
        KeyCode::Char('d') => {
            app.pending_key = Some('d');
            app.pending_hints = vec![("dd", "delete line"), ("dw", "delete word")];
            app.message = String::from("-- PENDING: d --");
        }
        KeyCode::Char('p') => app.paste(),
        KeyCode::Char('u') => app.undo(),
        KeyCode::Char('t') => {
            app.pending_key = Some('t');
            app.pending_hints = vec![("tt", "next tab"), ("tT", "previous tab")];
            app.message = String::from("-- PENDING: t --");
        }
        KeyCode::Char('n') => app.search_next(),
        KeyCode::Char('N') => app.search_prev(),
        KeyCode::Char('G') => {
            let last_row = app.buffer.line_count().saturating_sub(1);
            app.buffer.cursor.row = last_row;
            app.buffer.move_to_line_start();
            app.update_scroll();
        }
        KeyCode::Char('g') => {
            app.pending_key = Some('g');
            app.pending_hints = vec![("gg", "go to top"), ("gd", "go to def (LSP)"), ("gT", "prev tab")];
            app.message = String::from("-- PENDING: g --");
        }
        _ => {}
    }
}

fn handle_pending(app: &mut App, pending: char, code: KeyCode) {
    match (pending, code) {
        ('d', KeyCode::Char('d')) => app.delete_line(),
        ('d', KeyCode::Char('w')) => app.delete_word(),
        ('d', _) => {
            app.message = String::from("d: use dd (delete line) or dw (delete word)");
        }
        ('g', KeyCode::Char('g')) => {
            app.buffer.cursor.row = 0;
            app.buffer.cursor.col = 0;
            app.scroll = 0;
            app.message = String::new();
        }
        ('g', KeyCode::Char('d')) => {
            if let Some(ref path) = app.filename {
                let cursor = app.buffer.cursor();
                app.lsp.request_definition(&path.display().to_string(), cursor.row, cursor.col);
                app.message = String::from("Requested go-to-definition");
            }
        }
        ('g', _) => {
            app.message = String::from("g: use gg (go to top)");
        }
        ('t', KeyCode::Char('t')) => { app.next_tab(); }
        ('t', KeyCode::Char('T')) => { app.prev_tab(); }
        ('t', _) => {
            app.message = String::from("t: use gt (next tab) or gT (prev tab)");
        }
        _ => {}
    }
}

fn handle_insert(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.completions.deactivate();
            app.enter_normal();
        }
        KeyCode::Enter => {
            if app.completions.active {
                apply_completion(app);
            } else {
                let trimmed = app.buffer.line(app.buffer.cursor.row).trim_end().to_string();
                let ends_block = trimmed.ends_with('{')
                    || trimmed.ends_with('[')
                    || trimmed.ends_with('(')
                    || trimmed.ends_with(':')
                    || trimmed.ends_with("=>")
                    || trimmed.ends_with("->");
                let ends_close = trimmed.ends_with(')') || trimmed.ends_with(']');
                app.buffer.insert_newline_with_indent(ends_block && !ends_close);
                app.update_scroll();
            }
        }
        KeyCode::Tab => {
            if app.completions.active && !app.completions.suggestions.is_empty() {
                apply_completion(app);
            } else {
                for _ in 0..4 {
                    app.buffer.insert_char(' ');
                }
            }
        }
        KeyCode::BackTab => {
            if app.completions.active {
                app.completions.prev();
            }
        }
        KeyCode::Left => {
            app.completions.deactivate();
            app.buffer.move_left();
            app.update_scroll();
        }
        KeyCode::Right => {
            app.completions.deactivate();
            app.buffer.move_right();
            app.update_scroll();
        }
        KeyCode::Up => {
            if app.completions.active {
                app.completions.prev();
            } else {
                app.buffer.move_up();
                app.update_scroll();
            }
        }
        KeyCode::Down => {
            if app.completions.active {
                app.completions.next();
            } else {
                app.buffer.move_down();
                app.update_scroll();
            }
        }
        KeyCode::Backspace => {
            if is_pair_close_char(app.buffer.char_before_cursor())
                && is_pair_open_char(app.buffer.char_after_cursor())
            {
                if !app.buffer.delete_pair(
                    app.buffer.char_before_cursor().unwrap(),
                    pair_close_for(app.buffer.char_before_cursor().unwrap()),
                ) {
                    app.buffer.backspace();
                }
            } else {
                app.buffer.backspace();
            }
            app.completions.deactivate();
        }
        KeyCode::Char(')') => {
            if !app.buffer.skip_char_if_match(')') {
                app.buffer.insert_char(')');
            }
            app.completions.deactivate();
        }
        KeyCode::Char(']') => {
            if !app.buffer.skip_char_if_match(']') {
                app.buffer.insert_char(']');
            }
            app.completions.deactivate();
        }
        KeyCode::Char('}') => {
            if !app.buffer.skip_char_if_match('}') {
                app.buffer.insert_char('}');
            }
            app.completions.deactivate();
        }
        KeyCode::Char('\'') => {
            app.completions.deactivate();
            if app.buffer.skip_char_if_match('\'') {
            } else if should_auto_close_single_quote(app) {
                app.buffer.insert_char_pair('\'', '\'');
            } else {
                app.buffer.insert_char('\'');
            }
        }
        KeyCode::Char('"') => {
            app.completions.deactivate();
            if app.buffer.skip_char_if_match('"') {
            } else if should_auto_close_double_quote(app) {
                app.buffer.insert_char_pair('"', '"');
            } else {
                app.buffer.insert_char('"');
            }
        }
        KeyCode::Char('(') => {
            app.buffer.insert_char_pair('(', ')');
            app.completions.deactivate();
        }
        KeyCode::Char('[') => {
            app.buffer.insert_char_pair('[', ']');
            app.completions.deactivate();
        }
        KeyCode::Char('{') => {
            app.buffer.insert_char_pair('{', '}');
            app.completions.deactivate();
        }
        KeyCode::Char('<') => {
            app.buffer.insert_char_pair('<', '>');
            app.completions.deactivate();
        }
        KeyCode::Char('`') => {
            app.completions.deactivate();
            if app.buffer.skip_char_if_match('`') {
            } else {
                app.buffer.insert_char_pair('`', '`');
            }
        }
        KeyCode::Char(c) => {
            app.buffer.insert_char(c);
            app.update_scroll();
            auto_trigger_completion(app, c);
        }
        _ => {
            app.completions.deactivate();
        }
    }
}

fn handle_visual(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => app.enter_normal(),
        KeyCode::Char('d') => app.delete_selection(),
        KeyCode::Char('y') => app.yank_selection(),
        KeyCode::Char('h') | KeyCode::Left => app.move_left(),
        KeyCode::Char('l') | KeyCode::Right => app.move_right(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('w') => app.buffer.move_word_forward(),
        KeyCode::Char('b') => app.buffer.move_word_back(),
        KeyCode::Char('0') => app.buffer.move_to_line_start(),
        KeyCode::Char('$') => app.buffer.move_to_line_end(),
        KeyCode::Char('G') => {
            let last_row = app.buffer.line_count().saturating_sub(1);
            app.buffer.cursor.row = last_row;
            app.buffer.move_to_line_start();
            app.update_scroll();
        }
        _ => {}
    }
}

fn handle_explorer(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Char('j') | KeyCode::Down => app.explorer.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.explorer.move_up(),
        KeyCode::Char('h') => {
            if let Some(parent) = app.explorer.cwd.parent().map(|p| p.to_path_buf()) {
                app.explorer.cwd = parent;
                app.explorer.refresh();
            }
        }
        KeyCode::Enter | KeyCode::Char('l') => {
            if let Some(path) = app.explorer.select_current() {
                open_file(app, &path);
            }
        }
        _ => {}
    }
}

fn open_file(app: &mut App, path: &std::path::PathBuf) {
    let path_str = path.display().to_string();
    app.open_new_tab(&path_str);
}

fn handle_terminal(app: &mut App, code: KeyCode) {
    app.terminal.poll();

    match code {
        KeyCode::Esc => {
            app.terminal.open = false;
            app.terminal.shutdown();
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            app.terminal.write_input(b"\r");
        }
        KeyCode::Backspace => {
            app.terminal.write_input(&[0x7f]);
        }
        KeyCode::Tab => {
            app.terminal.write_input(b"\t");
        }
        KeyCode::Left => {
            app.terminal.write_input(b"\x1b[D");
        }
        KeyCode::Right => {
            app.terminal.write_input(b"\x1b[C");
        }
        KeyCode::Up => {
            app.terminal.write_input(b"\x1b[A");
        }
        KeyCode::Down => {
            app.terminal.write_input(b"\x1b[B");
        }
        KeyCode::Home => {
            app.terminal.write_input(b"\x1b[H");
        }
        KeyCode::End => {
            app.terminal.write_input(b"\x1b[F");
        }
        KeyCode::PageUp => {
            app.terminal.scroll_up(3);
        }
        KeyCode::PageDown => {
            app.terminal.scroll_down(3);
        }
        KeyCode::Delete => {
            app.terminal.write_input(b"\x1b[3~");
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            app.terminal.write_input(s.as_bytes());
        }
        _ => {}
    }
}

fn handle_xlc(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => app.close_xlc(),
        KeyCode::Enter => {
            let cmd = app.xlc.input.trim().to_string();
            if cmd == "wq" || cmd == "x" {
                app.save_file();
                app.xlc.add_output("Saved. Quitting.");
                app.quit();
            } else {
                app.execute_xlc();
            }
        }
        KeyCode::Backspace => {
            if app.xlc.input.is_empty() {
                app.close_xlc();
            } else {
                app.xlc.pop_char();
            }
        }
        KeyCode::Up => app.xlc.history_up(),
        KeyCode::Down => app.xlc.history_down(),
        KeyCode::PageUp => app.xlc.scroll_up(5),
        KeyCode::PageDown => app.xlc.scroll_down(5),
        KeyCode::Home => app.xlc.scroll_to_top(),
        KeyCode::End => app.xlc.scroll_to_bottom(),
        KeyCode::Char(c) => app.xlc.push_char(c),
        _ => {}
    }
}

fn handle_search_input(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.search_pattern = None;
            app.search_matches.clear();
            app.xlc.close();
            app.enter_normal();
        }
        KeyCode::Enter => {
            let pattern = app.xlc.input.trim().to_string();
            let pattern = if pattern.starts_with('/') {
                pattern[1..].to_string()
            } else {
                pattern
            };
            if !pattern.is_empty() {
                app.search_pattern = Some(pattern.clone());
                app.perform_search();
                app.message = format!("Search: /{}/  {} matches", pattern, app.search_matches.len());
            }
            app.xlc.close();
            app.enter_normal();
        }
        KeyCode::Backspace => {
            if app.xlc.input.is_empty() || app.xlc.input == "/" {
                app.search_pattern = None;
                app.search_matches.clear();
                app.xlc.close();
                app.enter_normal();
            } else {
                app.xlc.pop_char();
                let pattern = app.xlc.input.trim().to_string();
                let pattern = if pattern.starts_with('/') {
                    pattern[1..].to_string()
                } else {
                    pattern
                };
                if !pattern.is_empty() {
                    app.search_pattern = Some(pattern.clone());
                    app.perform_search();
                    app.message = format!("Search: /{}/  {} matches", pattern, app.search_matches.len());
                } else {
                    app.search_pattern = None;
                    app.search_matches.clear();
                }
            }
        }
        KeyCode::Char(c) => {
            app.xlc.push_char(c);
            let pattern = app.xlc.input.trim().to_string();
            let pattern = if pattern.starts_with('/') {
                pattern[1..].to_string()
            } else {
                pattern
            };
            if !pattern.is_empty() {
                app.search_pattern = Some(pattern.clone());
                app.perform_search();
                app.message = format!("Search: /{}/  {} matches", pattern, app.search_matches.len());
            }
        }
        _ => {}
    }
}

fn should_auto_close_single_quote(app: &App) -> bool {
    let before = app.buffer.char_before_cursor();
    let after = app.buffer.char_after_cursor();
    matches!(before, None | Some(' ') | Some('(') | Some('[') | Some('{') | Some(','))
        && matches!(after, None | Some(' ') | Some(')') | Some(']') | Some('}') | Some(',') | Some(';'))
}

fn should_auto_close_double_quote(app: &App) -> bool {
    should_auto_close_single_quote(app)
}

fn is_pair_open_char(c: Option<char>) -> bool {
    matches!(c, Some('(') | Some('[') | Some('{') | Some('"') | Some('\'') | Some('`') | Some('<'))
}

fn is_pair_close_char(c: Option<char>) -> bool {
    matches!(c, Some(')') | Some(']') | Some('}') | Some('"') | Some('\'') | Some('`') | Some('>'))
}

fn pair_close_for(open: char) -> char {
    match open {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '"' => '"',
        '\'' => '\'',
        '`' => '`',
        '<' => '>',
        _ => open,
    }
}

fn trigger_completion(app: &mut App) {
    let prefix = word_before_cursor(app);
    let ext = app.file_extension();
    app.completions.activate(&prefix, ext.as_deref());
    if app.lsp.server_running {
        if let Some(ref path) = app.filename {
            let c = app.buffer.cursor();
            app.lsp.request_completion(&path.display().to_string(), c.row, c.col);
        }
    }
}

fn auto_trigger_completion(app: &mut App, c: char) {
    if c.is_alphabetic() || c == '_' {
        let prefix = word_before_cursor(app);
        if prefix.len() >= 1 {
            let ext = app.file_extension();
            if app.completions.active {
                app.completions.refine(&prefix);
            } else {
                app.completions.activate(&prefix, ext.as_deref());
            }
        } else {
            app.completions.deactivate();
        }
    } else {
        app.completions.deactivate();
    }
}

fn apply_completion(app: &mut App) {
    if let Some(suggestion) = app.completions.selected_suggestion().cloned() {
        let prefix = app.completions.prefix.clone();
        if !prefix.is_empty() {
            for _ in 0..prefix.chars().count() {
                app.buffer.backspace();
            }
        }

        let text = &suggestion.insert_text;
        let last = text.chars().last();

        match last {
            Some('(') | Some('[') | Some('{') | Some('<') => {
                for ch in text.chars().take(text.chars().count().saturating_sub(1)) {
                    app.buffer.insert_char(ch);
                }
                let close = match last.unwrap() {
                    '(' => ')',
                    '[' => ']',
                    '{' => '}',
                    '<' => '>',
                    _ => unreachable!(),
                };
                app.buffer.insert_char_pair(last.unwrap(), close);
            }
            _ => {
                for ch in text.chars() {
                    app.buffer.insert_char(ch);
                }
            }
        }

        app.completions.deactivate();
    }
}

fn word_before_cursor(app: &App) -> String {
    let cursor = app.buffer.cursor();
    let line = app.buffer.line(cursor.row);
    let chars: Vec<char> = line.chars().collect();

    let mut start = cursor.col;
    while start > 0 {
        let c = chars[start - 1];
        if c.is_alphanumeric() || c == '_' {
            start -= 1;
        } else {
            break;
        }
    }

    chars[start..cursor.col].iter().collect()
}
