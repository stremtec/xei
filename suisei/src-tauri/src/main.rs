use xei_core::App;

#[tauri::command]
fn get_state(app: tauri::State<'_, Mutex<App>>) -> Result<serde_json::Value, String> {
    let app = app.lock().map_err(|e| e.to_string())?;
    let state = serde_json::json!({
        "mode": format!("{:?}", app.mode),
        "cursor_row": app.buffer.cursor.row,
        "cursor_col": app.buffer.cursor.col,
        "line_count": app.buffer.line_count(),
        "filename": app.filename.as_ref().map(|p| p.display().to_string()),
        "text": app.buffer.text(),
        "search_pattern": app.search_pattern,
        "search_matches": app.search_matches.len(),
        "search_current": app.search_current,
        "explorer_open": app.explorer.open,
        "explorer_entries": app.explorer.entries.iter().map(|e| serde_json::json!({
            "name": e.name,
            "is_dir": e.is_dir,
            "path": e.path.display().to_string(),
        })).collect::<Vec<_>>(),
        "explorer_selected": app.explorer.selected,
        "explorer_cwd": app.explorer.cwd.display().to_string(),
        "xlc_open": app.xlc.open,
        "xlc_input": app.xlc.input,
        "xlc_output": app.xlc.output,
        "message": app.message,
        "modified": app.modified,
        "tab_count": app.buffers.len(),
        "current_tab": app.current_buffer,
        "tabs": app.buffers.iter().map(|t| serde_json::json!({
            "filename": t.filename.as_ref().map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or("[no name]")),
            "modified": t.modified,
        })).collect::<Vec<_>>(),
        "lsp_diagnostics": app.lsp.diagnostics.len(),
    });
    Ok(state)
}

#[tauri::command]
fn handle_key(app: tauri::State<'_, Mutex<App>>, key: String, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Result<serde_json::Value, String> {
    let mut app = app.lock().map_err(|e| e.to_string())?;
    // Simulate key handling through App methods
    handle_input(&mut app, &key, ctrl, alt, shift, meta);
    Ok(serde_json::json!({ "ok": true }))
}

fn handle_input(app: &mut App, key: &str, ctrl: bool, _alt: bool, _shift: bool, meta: bool) {
    if meta {
        match key {
            "c" => {
                if app.mode == xei_core::Mode::Visual || app.mode == xei_core::Mode::VisualLine {
                    app.yank_selection();
                    if let Some(ref yb) = app.yank_buffer {
                        xei_core::clipboard::copy(yb);
                    }
                }
                return;
            }
            "v" => {
                if let Some(text) = xei_core::clipboard::paste() {
                    app.yank_buffer = Some(text.clone());
                    app.paste();
                }
                return;
            }
            _ => {}
        }
    }

    if ctrl {
        match key {
            "f" => {
                let was_open = app.explorer.open;
                let filename = app.filename.clone();
                app.explorer.toggle(filename.as_ref());
                if app.explorer.open && !was_open { app.mode = xei_core::Mode::Explorer; }
                else if !app.explorer.open { app.mode = xei_core::Mode::Normal; }
                return;
            }
            "t" => {
                let filename = app.filename.clone();
                if app.terminal.open { app.terminal.shutdown(); app.terminal.open = false; app.mode = xei_core::Mode::Normal; }
                else { app.terminal.open = true; app.terminal.start(filename.as_ref()); app.mode = xei_core::Mode::Terminal; }
                return;
            }
            _ => {}
        }
    }

    match app.mode {
        xei_core::Mode::Explorer => {
            match key {
                "Escape" | "q" => { app.explorer.close(); app.mode = xei_core::Mode::Normal; }
                "j" | "ArrowDown" => { app.explorer.move_down(); }
                "k" | "ArrowUp" => { app.explorer.move_up(); }
                "h" | "ArrowLeft" => { if let Some(parent) = app.explorer.cwd.parent() { app.explorer.cwd = parent.to_path_buf(); app.explorer.refresh(); } }
                "l" | "ArrowRight" | "Enter" => {
                    if let Some(path) = app.explorer.select_current() {
                        if path.is_dir() { app.explorer.cwd = path; app.explorer.refresh(); }
                        else { app.explorer.close(); app.open_new_tab(&path.display().to_string()); app.mode = xei_core::Mode::Normal; }
                    }
                }
                _ => {}
            }
            return;
        }
        xei_core::Mode::Terminal => {
            match key {
                "Escape" => { app.mode = xei_core::Mode::Normal; }
                "Enter" => { app.terminal.write_input(b"\n"); app.terminal.poll(); }
                "Backspace" => { app.terminal.write_input(b"\x7f"); app.terminal.poll(); }
                "Tab" => { app.terminal.write_input(b"\t"); app.terminal.poll(); }
                _ if key.len() == 1 => { app.terminal.write_input(key.as_bytes()); app.terminal.poll(); }
                _ => {}
            }
            return;
        }
        xei_core::Mode::XlcInput => {
            match key {
                "Escape" => { app.xlc.close(); app.mode = xei_core::Mode::Normal; }
                "Enter" => { app.execute_xlc(); }
                "Backspace" => { app.xlc.pop_char(); }
                "ArrowUp" => { app.xlc.history_up(); }
                "ArrowDown" => { app.xlc.history_down(); }
                _ if key.len() == 1 => { app.xlc.push_char(key.chars().next().unwrap()); }
                _ => {}
            }
            return;
        }
        xei_core::Mode::Normal => {
            // Handle pending keys
            if let Some(px) = app.pending_key.take() {
                match (px, key) {
                    ('g', "g") => { app.buffer.cursor.row = 0; app.buffer.cursor.col = 0; app.scroll = 0; return; }
                    ('g', "t") => { app.next_tab(); return; }
                    ('g', "T") => { app.prev_tab(); return; }
                    ('d', "d") => { app.push_undo(); app.delete_line(); return; }
                    ('d', "w") => { app.push_undo(); app.delete_word(); return; }
                    ('y', "y") => { app.yank_buffer = Some(app.buffer.line(app.buffer.cursor.row).to_string()); app.message = "Yanked".into(); return; }
                    _ => {}
                }
            }
            match key {
                "i" => app.mode = xei_core::Mode::Insert,
                "a" => { app.buffer.move_right(); app.mode = xei_core::Mode::Insert; }
                "A" => { app.buffer.move_to_line_end(); app.mode = xei_core::Mode::Insert; }
                "I" => { app.buffer.cursor.col = 0; app.mode = xei_core::Mode::Insert; }
                "o" => { app.push_undo(); app.buffer.move_to_line_end(); app.buffer.insert_newline_with_indent(false); app.mode = xei_core::Mode::Insert; }
                "O" => { let row = app.buffer.cursor.row; let indent = app.buffer.leading_indent(row); app.push_undo(); app.buffer.insert_line_at(row, indent); app.mode = xei_core::Mode::Insert; }
                "v" => app.enter_visual(),
                "V" => app.enter_visual_line(),
                "h" => { app.buffer.move_left(); app.update_scroll(); }
                "j" => { app.buffer.move_down(); app.update_scroll(); }
                "k" => { app.buffer.move_up(); app.update_scroll(); }
                "l" => { app.buffer.move_right(); app.update_scroll(); }
                "w" => { app.buffer.move_word_forward(); app.update_scroll(); }
                "b" => { app.buffer.move_word_back(); app.update_scroll(); }
                "0" => app.buffer.cursor.col = 0,
                "$" => app.buffer.move_to_line_end(),
                "G" => { app.buffer.cursor.row = app.buffer.line_count().saturating_sub(1); app.buffer.cursor.col = 0; app.update_scroll(); }
                "ArrowLeft" => { app.buffer.move_left(); app.update_scroll(); }
                "ArrowRight" => { app.buffer.move_right(); app.update_scroll(); }
                "ArrowUp" => { app.buffer.move_up(); app.update_scroll(); }
                "ArrowDown" => { app.buffer.move_down(); app.update_scroll(); }
                "x" => { app.push_undo(); if app.buffer.cursor.col < app.buffer.current_line_len() { app.buffer.delete_char_at_cursor(); } }
                "u" => app.undo(),
                "p" => app.paste(),
                ":" => { app.mode = xei_core::Mode::XlcInput; app.xlc.open_panel(None); }
                "/" => { app.mode = xei_core::Mode::XlcInput; app.xlc.open_panel(Some("/")); }
                "g" | "d" | "y" => { app.pending_key = Some(key.chars().next().unwrap_or(' ')); }
                "n" => { app.search_next(); app.update_scroll(); }
                "N" => { app.search_prev(); app.update_scroll(); }
                "Escape" => {}
                _ => {}
            }
        }
        xei_core::Mode::Insert => {
            match key {
                "Escape" => { app.mode = xei_core::Mode::Normal; if app.buffer.cursor.col > 0 { app.buffer.cursor.col -= 1; } }
                "Backspace" => { app.buffer.backspace(); app.update_scroll(); }
                "Enter" => { app.buffer.insert_newline_with_indent(false); app.update_scroll(); }
                "Tab" => { for _ in 0..4 { app.buffer.insert_char(' '); }; app.update_scroll(); }
                "ArrowLeft" => { app.buffer.move_left(); app.update_scroll(); }
                "ArrowRight" => { app.buffer.move_right(); app.update_scroll(); }
                "ArrowUp" => { app.buffer.move_up(); app.update_scroll(); }
                "ArrowDown" => { app.buffer.move_down(); app.update_scroll(); }
                _ if key.len() == 1 => { app.buffer.insert_char(key.chars().next().unwrap()); app.update_scroll(); }
                _ => {}
            }
        }
        xei_core::Mode::Visual | xei_core::Mode::VisualLine => {
            match key {
                "Escape" => { app.mode = xei_core::Mode::Normal; app.visual_anchor = None; }
                "h" => { app.buffer.move_left(); app.update_scroll(); }
                "j" => { app.buffer.move_down(); app.update_scroll(); }
                "k" => { app.buffer.move_up(); app.update_scroll(); }
                "l" => { app.buffer.move_right(); app.update_scroll(); }
                "y" => { app.yank_selection(); app.mode = xei_core::Mode::Normal; }
                "d" => { app.delete_selection(); app.mode = xei_core::Mode::Normal; }
                _ => {}
            }
        }
        _ => {}
    }

    // After each key, sync the document to the LSP, then poll LSP and terminal
    app.sync_lsp_document();
    app.lsp.poll();
    if app.terminal.open { app.terminal.poll(); }
}

use std::sync::Mutex;

fn main() {
    let app = Mutex::new(App::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app)
        .invoke_handler(tauri::generate_handler![get_state, handle_key])
        .run(tauri::generate_context!())
        .expect("error while running suisei");
}
