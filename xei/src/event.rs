use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use std::io;

use xei_core::app::{App, Mode, ResizeTarget};
use xei_core::buffer::Position;
use xei_core::macros::MacroKey;
use xei_core::nav::FindKind;
use xei_core::ops::{motion_from_char, parse_textobject, Motion, Operator};

/// Returns (still_running, processed_any_event) — the caller uses the event
/// flag to keep rendering at full rate around user interaction.
pub fn handle_events(app: &mut App) -> io::Result<(bool, bool)> {
    if !event::poll(std::time::Duration::from_millis(10))? {
        return Ok((app.running, false));
    }
    let mut had_event = false;
    loop {
        match event::read()? {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Press {
                    had_event = true;
                    handle_key(app, key.code, key.modifiers);
                }
            }
            Event::Mouse(mouse) => {
                had_event = true;
                handle_mouse(app, mouse.kind, mouse.column, mouse.row, mouse.modifiers);
            }
            Event::Resize(_w, _h) => had_event = true,
            _ => {}
        }
        if !event::poll(std::time::Duration::from_millis(0))? {
            break;
        }
    }
    Ok((app.running, had_event))
}
fn rect_contains(rect: Option<(u16, u16, u16, u16)>, column: u16, row: u16) -> bool {
    rect.map(|(x, y, w, h)| {
        column >= x && column < x.saturating_add(w) && row >= y && row < y.saturating_add(h)
    })
    .unwrap_or(false)
}

/// Wheel routing: pointer position first, then the active surface, then the
/// editor pane under the pointer.
fn route_scroll(app: &mut App, column: u16, row: u16, delta: isize) {
    // Terminal (side panel, full window, or pane-bound) under the pointer.
    if rect_contains(app.terminal_rect, column, row) {
        if app.terminal.wants_mouse() {
            // Inner app owns the mouse (claude/vim/htop) — forward SGR wheel
            // at pane-local 1-based coordinates.
            if let Some((tx, ty, _, _)) = app.terminal_rect {
                let btn = if delta < 0 { 64 } else { 65 };
                let lx = column.saturating_sub(tx) as u32 + 1;
                let ly = row.saturating_sub(ty) as u32 + 1;
                let seq = format!("\x1b[<{btn};{lx};{ly}M");
                app.terminal.write_input(seq.as_bytes());
            }
        } else if app.terminal.is_alt_screen() {
            // Fullscreen TUI without mouse mode: wheel = arrows (tmux-style).
            let seq = app.terminal.arrow_seq(if delta < 0 { 'A' } else { 'B' });
            for _ in 0..3 {
                app.terminal.write_input(seq);
            }
        } else if delta < 0 {
            app.terminal.scroll_up((-delta) as usize);
        } else {
            app.terminal.scroll_down(delta as usize);
        }
        return;
    }
    // DAP panel under the pointer.
    if app.dap.panel_open && rect_contains(app.dap_panel_rect, column, row) {
        app.dap.move_focus(delta);
        return;
    }
    // XLC panel under the pointer (its output log scrolls; the editor above
    // keeps normal wheel behavior).
    if app.mode == Mode::XlcInput
        && app.xlc.open
        && app.xlc_separator_y > 0
        && row >= app.xlc_separator_y
    {
        if delta < 0 {
            app.xlc.scroll_up(3);
        } else {
            app.xlc.scroll_down(3);
        }
        return;
    }
    // Surface-active modes.
    match app.mode {
        Mode::Explorer => {
            for _ in 0..delta.unsigned_abs() {
                if delta < 0 {
                    app.explorer.move_up();
                } else {
                    app.explorer.move_down();
                }
            }
            return;
        }
        Mode::Preview => {
            app.preview.scroll_by(delta, 10);
            return;
        }
        Mode::GitWorkbench => {
            app.git_wb.move_sel(delta);
            return;
        }
        Mode::PrReview => {
            use xei_core::pr_review::PrReviewFocus;
            // Files view: left third is the list, the rest is the diff.
            if app.pr_review.focus == PrReviewFocus::Files {
                let left_w = (app.screen_width / 3).clamp(18, 36);
                if column >= left_w {
                    app.pr_review.diff_scroll = if delta < 0 {
                        app.pr_review.diff_scroll.saturating_sub((-delta) as usize)
                    } else {
                        app.pr_review.diff_scroll.saturating_add(delta as usize)
                    };
                    return;
                }
            }
            app.pr_review.move_sel(delta);
            return;
        }
        Mode::Settings => {
            app.settings.move_sel(delta);
            return;
        }
        Mode::SourceControl => {
            app.scm.move_sel(delta);
            return;
        }
        Mode::CallHierarchy => {
            app.call_hierarchy.move_sel(delta);
            return;
        }
        Mode::Rebase => {
            app.rebase.move_sel(delta);
            return;
        }
        _ => {}
    }
    // Split: scroll the pane under the pointer, not just the focused one.
    if app.split.is_split() {
        let n = app.split.panes.len();
        let focus = app.split.focus.min(n.saturating_sub(1));
        for &(px, py, pw, ph, idx) in &app.pane_hit_regions.clone() {
            if rect_contains(Some((px, py, pw, ph)), column, row) && idx != focus && idx < n {
                let pane_tab = app.split.panes[idx].tab_index;
                let line_count = app
                    .buffers
                    .get(pane_tab)
                    .map(|t| t.buffer.line_count())
                    .unwrap_or(0);
                let visible = ph.max(1) as usize;
                let pane = &mut app.split.panes[idx];
                pane.scroll = if delta < 0 {
                    pane.scroll.saturating_sub((-delta) as usize)
                } else {
                    (pane.scroll + delta as usize)
                        .min(line_count.saturating_sub(visible.min(line_count)))
                };
                return;
            }
        }
    }
    // Default: scroll the active editor view.
    if delta < 0 {
        app.scroll = app.scroll.saturating_sub((-delta) as usize);
    } else {
        let line_count = app.buffer.line_count();
        let visible = app.viewport.height.max(1) as usize;
        if app.scroll + visible < line_count {
            app.scroll = (app.scroll + delta as usize).min(line_count.saturating_sub(visible));
        }
    }
}

fn handle_mouse(
    app: &mut App,
    kind: MouseEventKind,
    column: u16,
    row: u16,
    _modifiers: KeyModifiers,
) {
    match kind {
        MouseEventKind::ScrollUp => route_scroll(app, column, row, -3),
        MouseEventKind::ScrollDown => route_scroll(app, column, row, 3),
        MouseEventKind::ScrollLeft => {
            if !app.wrap_lines {
                if app.mode == Mode::Preview {
                    app.preview.hscroll = app.preview.hscroll.saturating_sub(6);
                } else {
                    app.hscroll = app.hscroll.saturating_sub(6);
                }
            }
        }
        MouseEventKind::ScrollRight => {
            if !app.wrap_lines {
                if app.mode == Mode::Preview {
                    app.preview.hscroll = app.preview.hscroll.saturating_add(6);
                } else {
                    app.hscroll = app.hscroll.saturating_add(6);
                }
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // PR review surface: tabs + rows; swallow everything else so the
            // hidden editor underneath never sees the click.
            if app.mode == Mode::PrReview {
                use xei_core::pr_review::PrReviewFocus;
                for &(x, y, w, h, tab) in &app.pr_tab_hits.clone() {
                    if column >= x
                        && column < x.saturating_add(w)
                        && row >= y
                        && row < y.saturating_add(h)
                    {
                        app.pr_review.focus = match tab {
                            0 => PrReviewFocus::Files,
                            1 => PrReviewFocus::Comments,
                            _ => PrReviewFocus::Body,
                        };
                        return;
                    }
                }
                for &(x, y, w, h, idx) in &app.pr_row_hits.clone() {
                    if column >= x
                        && column < x.saturating_add(w)
                        && row >= y
                        && row < y.saturating_add(h)
                    {
                        match app.pr_review.focus {
                            PrReviewFocus::Files => app.pr_review.select_file(idx),
                            PrReviewFocus::Comments => app.pr_review.select_comment(idx),
                            PrReviewFocus::Body => {}
                        }
                        return;
                    }
                }
                return;
            }
            // DAP panel: tabs / list rows (before editor selection)
            if app.dap.panel_open {
                for &(x, y, w, h, pane_id) in &app.dap_tab_hits {
                    if column >= x
                        && column < x.saturating_add(w)
                        && row >= y
                        && row < y.saturating_add(h)
                    {
                        use xei_core::dap::DebugPane;
                        let pane = match pane_id {
                            0 => DebugPane::Stack,
                            1 => DebugPane::Variables,
                            2 => DebugPane::Breakpoints,
                            _ => DebugPane::Console,
                        };
                        app.dap.set_pane(pane);
                        app.mode = Mode::Debug;
                        return;
                    }
                }
                for &(x, y, w, h, idx) in &app.dap_row_hits {
                    if column >= x
                        && column < x.saturating_add(w)
                        && row >= y
                        && row < y.saturating_add(h)
                    {
                        use xei_core::dap::DebugPane;
                        app.mode = Mode::Debug;
                        app.dap.focus_row = idx;
                        match app.dap.pane {
                            DebugPane::Stack => {
                                app.dap.select_frame(idx);
                                app.dap.location_dirty = true;
                                app.dap_apply_stopped_location();
                            }
                            DebugPane::Variables => {
                                app.dap.toggle_var_at(idx);
                            }
                            DebugPane::Breakpoints => {
                                app.dap.selected_bp = idx;
                            }
                            DebugPane::Console => {}
                        }
                        return;
                    }
                }
            }

            // Gutter click → toggle breakpoint (column in line-number gutter)
            if matches!(
                app.mode,
                Mode::Normal | Mode::Insert | Mode::Visual | Mode::VisualLine | Mode::Debug
            ) && app.viewport.height > 0
                && app.filename.is_some()
            {
                let text_x = if app.viewport.text_x == 0 {
                    app.viewport.x.saturating_add(LINE_NO_WIDTH)
                } else {
                    app.viewport.text_x
                };
                if column >= app.viewport.x
                    && column < text_x
                    && row >= app.viewport.text_y
                    && row < app.viewport.text_y.saturating_add(app.viewport.height)
                {
                    let pos = screen_to_buffer_clamped(app, text_x, row);
                    app.buffer.cursor.row = pos.row;
                    app.dap_toggle_breakpoint();
                    return;
                }
            }

            // Editor context menu click / dismiss
            if app.editor_ctx.is_some() {
                if click_editor_ctx(app, column, row) {
                    return;
                }
                app.close_editor_ctx();
            }

            // Dismiss commit context menu on left click outside (or select item)
            if app.mode == Mode::GitWorkbench && app.git_wb.ctx_menu.is_some() {
                if let Some(ref menu) = app.git_wb.ctx_menu {
                    let h = menu.items.len() as u16 + 2;
                    let w = 28u16;
                    if column >= menu.x
                        && column < menu.x.saturating_add(w)
                        && row >= menu.y
                        && row < menu.y.saturating_add(h)
                    {
                        let inner_y = row.saturating_sub(menu.y.saturating_add(1));
                        if (inner_y as usize) < menu.items.len() {
                            if let Some(m) = app.git_wb.ctx_menu.as_mut() {
                                m.sel = inner_y as usize;
                            }
                            match app.git_wb.run_ctx_action() {
                                Ok(msg) => {
                                    if let Some(hash) = msg.strip_prefix("Copied ") {
                                        let _ = xei_core::clipboard::copy(hash);
                                    }
                                    app.message = msg;
                                }
                                Err(e) => app.message = e,
                            }
                        }
                        return;
                    }
                }
                app.git_wb.close_ctx_menu();
            }

            // Tab bar click (VS Code-like)
            if row == app.tab_bar_y {
                for &(x0, x1, idx) in &app.tab_hit_regions {
                    if column >= x0 && column < x1 {
                        if idx != app.current_buffer {
                            if app.mode == Mode::Preview {
                                app.close_preview();
                            }
                            app.save_state_to_tab();
                            app.current_buffer = idx;
                            app.restore_state_from_tab();
                            app.lsp_restart_for_current();
                            app.message = format!("Tab {}", idx + 1);
                        }
                        return;
                    }
                }
            }

            // Git workbench toolbar chips (1–8)
            if app.mode == Mode::GitWorkbench {
                for &(x, y, w, h, key) in &app.git_tab_hits {
                    if column >= x
                        && column < x.saturating_add(w)
                        && row >= y
                        && row < y.saturating_add(h)
                    {
                        git_switch_tab_key(app, key);
                        return;
                    }
                }
                // Docked pane focus by click
                for &(x, y, w, h, pane_id) in &app.git_pane_hits {
                    if column >= x
                        && column < x.saturating_add(w)
                        && row >= y
                        && row < y.saturating_add(h)
                    {
                        use xei_core::git_workbench::GitPane;
                        app.git_wb.pane = match pane_id {
                            0 => GitPane::Changes,
                            1 => GitPane::Log,
                            _ => GitPane::Files,
                        };
                        // Click log row → select commit
                        if pane_id == 1 {
                            for &(lx, ly, lw, lh, idx) in &app.git_log_hits {
                                if column >= lx
                                    && column < lx.saturating_add(lw)
                                    && row >= ly
                                    && row < ly.saturating_add(lh)
                                {
                                    app.git_wb.history_sel = idx;
                                    let _ = app.git_wb.load_selected_commit_detail();
                                    break;
                                }
                            }
                        }
                        return;
                    }
                }
            }

            if is_on_separator(app, column, row) {
                return;
            }

            // Explorer click
            if app.explorer.open && column < app.explorer_separator_x {
                app.mode = Mode::Explorer;
                return;
            }

            // Split pane focus on click
            if app.split.is_split() {
                for &(px, py, pw, ph, idx) in &app.pane_hit_regions {
                    if column >= px
                        && column < px.saturating_add(pw)
                        && row >= py
                        && row < py.saturating_add(ph)
                    {
                        if idx != app.split.focus {
                            app.focus_pane(idx);
                        }
                        break;
                    }
                }
            }

            // Preview owns the editor pane; clicks there shouldn't move the
            // hidden buffer cursor or start a drag-selection.
            if app.mode == Mode::Preview {
                return;
            }
            if app.mode == Mode::GitWorkbench {
                return;
            }

            if app.viewport.height == 0 || app.viewport.width == 0 {
                return;
            }
            let pos = screen_to_buffer_clamped(app, column, row);
            app.buffer.cursor = pos;

            // Double-click → select word (VS Code-like)
            let now = std::time::Instant::now();
            let is_double = app
                .last_click
                .map(|(c, r, t)| {
                    c == column && r == row && now.duration_since(t).as_millis() < 400
                })
                .unwrap_or(false);
            app.last_click = Some((column, row, now));
            if is_double {
                app.select_word_under_cursor();
                app.mouse.dragging = false;
                app.mouse.drag_anchor = None;
                return;
            }

            app.mouse.dragging = true;
            app.mouse.drag_anchor = Some(pos);
            app.hover_text = None;
            if app.mode == Mode::Explorer || app.mode == Mode::Palette {
                app.palette.close();
                app.mode = Mode::Normal;
            }
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
                        let new_height = app
                            .screen_height
                            .saturating_sub(row)
                            .saturating_sub(1)
                            .max(5)
                            .min(30);
                        app.xlc_height = new_height;
                    }
                    ResizeTarget::Split => {
                        if let Some(hit) = app.split_sep_hit {
                            if hit.vertical && hit.area_w > 0 {
                                let rel = column.saturating_sub(hit.area_x) as f32
                                    / hit.area_w as f32;
                                app.split.ratio = rel.clamp(0.2, 0.8);
                                app.message = format!("Split {:.0}%", app.split.ratio * 100.0);
                            } else if !hit.vertical && hit.area_h > 0 {
                                let rel = row.saturating_sub(hit.area_y) as f32
                                    / hit.area_h as f32;
                                app.split.ratio = rel.clamp(0.2, 0.8);
                                app.message = format!("Split {:.0}%", app.split.ratio * 100.0);
                            }
                        }
                    }
                }
                return;
            }

            if app.mouse.dragging {
                if app.mode == Mode::Normal {
                    app.visual_anchor = app.mouse.drag_anchor;
                    app.mode = Mode::Visual;
                    app.message = String::from("-- VISUAL --");
                    app.completions.deactivate();
                }
                // Insert mode drag → enter visual selection from anchor
                if app.mode == Mode::Insert {
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
        MouseEventKind::Down(MouseButton::Right) => {
            // Commit context menu in Git workbench Log pane
            if app.mode == Mode::GitWorkbench {
                if let Some(ref menu) = app.git_wb.ctx_menu {
                    let h = menu.items.len() as u16 + 2;
                    let w = 28u16;
                    if column >= menu.x
                        && column < menu.x.saturating_add(w)
                        && row >= menu.y
                        && row < menu.y.saturating_add(h)
                    {
                        let inner_y = row.saturating_sub(menu.y.saturating_add(1));
                        if (inner_y as usize) < menu.items.len() {
                            if let Some(m) = app.git_wb.ctx_menu.as_mut() {
                                m.sel = inner_y as usize;
                            }
                            match app.git_wb.run_ctx_action() {
                                Ok(msg) => {
                                    if msg.starts_with("Copied ") {
                                        if let Some(h) = msg.strip_prefix("Copied ") {
                                            let _ = xei_core::clipboard::copy(h);
                                        }
                                    }
                                    app.message = msg;
                                }
                                Err(e) => app.message = e,
                            }
                        }
                        return;
                    }
                    app.git_wb.close_ctx_menu();
                }
                for &(x, y, w, h, idx) in &app.git_log_hits {
                    if column >= x
                        && column < x.saturating_add(w)
                        && row >= y
                        && row < y.saturating_add(h)
                    {
                        app.git_wb.open_commit_ctx(column, row, idx);
                        app.message = "Commit menu · j/k · Enter · Esc".into();
                        return;
                    }
                }
                app.git_wb.close_ctx_menu();
                return;
            }

            // Editor right-click menu (Insert / Normal / Visual)
            if matches!(
                app.mode,
                Mode::Normal
                    | Mode::Insert
                    | Mode::Visual
                    | Mode::VisualLine
                    | Mode::VisualBlock
            ) {
                // Place cursor under right-click first
                if app.viewport.height > 0 && app.viewport.width > 0 {
                    let pos = screen_to_buffer_clamped(app, column, row);
                    // Keep visual selection if clicking inside it; else move caret
                    if !matches!(
                        app.mode,
                        Mode::Visual | Mode::VisualLine | Mode::VisualBlock
                    ) {
                        app.buffer.cursor = pos;
                    }
                }
                app.open_editor_ctx(column, row);
            }
        }
        _ => {}
    }
}

fn click_editor_ctx(app: &mut App, column: u16, row: u16) -> bool {
    let Some(ref menu) = app.editor_ctx else {
        return false;
    };
    let h = menu.items.len() as u16 + 2;
    let w = 32u16;
    if column >= menu.x
        && column < menu.x.saturating_add(w)
        && row >= menu.y
        && row < menu.y.saturating_add(h)
    {
        let inner_y = row.saturating_sub(menu.y.saturating_add(1));
        if (inner_y as usize) < menu.items.len() {
            if let Some(m) = app.editor_ctx.as_mut() {
                m.sel = inner_y as usize;
            }
            match app.run_editor_ctx_action() {
                Ok(msg) => app.message = msg,
                Err(e) => app.message = e,
            }
        }
        return true;
    }
    false
}

/// Switch Git workbench surface by toolbar key 1–8 (shared with keyboard path).
fn git_switch_tab_key(app: &mut App, key: u8) {
    use xei_core::git_workbench::{GitFocus, GitPane, GitTab};
    match key {
        1 => {
            app.git_wb.tab = GitTab::Status;
            app.git_wb.pane = GitPane::Changes;
            app.git_wb.focus = GitFocus::List;
            app.git_wb.ensure_tab_data();
            app.message = "Git · Changes".into();
        }
        2 => {
            app.git_wb.tab = GitTab::History;
            app.git_wb.pane = GitPane::Log;
            app.git_wb.focus = GitFocus::List;
            app.git_wb.ensure_tab_data();
            app.message = "Git · Log".into();
        }
        3 => {
            app.git_wb.tab = GitTab::Branches;
            app.git_wb.focus = GitFocus::List;
            app.git_wb.ensure_tab_data();
            app.message = "Git · Branches".into();
        }
        4 => match app.git_wb.focus_files_pane() {
            Ok(()) => {
                app.message = app
                    .git_wb
                    .message
                    .clone()
                    .unwrap_or_else(|| "Git · Files".into());
            }
            Err(e) => app.message = e,
        },
        5 => {
            if app.git_wb.tab == GitTab::Diff && app.git_wb.diff_path.is_some() {
                app.message = "Git · Diff".into();
            } else {
                match app.git_wb.open_context_diff() {
                    Ok(()) => {
                        app.message = app
                            .git_wb
                            .diff_path
                            .as_ref()
                            .map(|p| format!("Diff · {p}"))
                            .unwrap_or_else(|| "Git · Diff".into());
                    }
                    Err(e) => {
                        if app.git_wb.diff_path.is_some() {
                            app.git_wb.tab = GitTab::Diff;
                            app.git_wb.focus = GitFocus::Diff;
                            app.message = "Git · Diff".into();
                        } else {
                            app.message = e;
                        }
                    }
                }
            }
        }
        6 => {
            app.git_wb.tab = GitTab::PullRequests;
            app.git_wb.focus = GitFocus::List;
            app.git_wb.ensure_tab_data();
            app.message = "Git · PRs".into();
        }
        7 => {
            app.git_wb.tab = GitTab::Issues;
            app.git_wb.focus = GitFocus::List;
            app.git_wb.ensure_tab_data();
            app.message = "Git · Issues".into();
        }
        8 => {
            app.git_wb.tab = GitTab::Auth;
            app.git_wb.focus = GitFocus::List;
            app.git_wb.ensure_tab_data();
            app.message = "Git · Auth".into();
        }
        9 => {
            app.git_wb.tab = GitTab::Stash;
            app.git_wb.focus = GitFocus::List;
            app.git_wb.ensure_tab_data();
            app.message = "Git · Stash".into();
        }
        _ => {}
    }
}

fn is_on_separator(app: &mut App, column: u16, row: u16) -> bool {
    const HIT_MARGIN: u16 = 3;
    const SPLIT_MARGIN: u16 = 1;

    // Split divider (editor panes) — prefer this over text selection
    if let Some(hit) = app.split_sep_hit {
        if hit.vertical {
            if column >= hit.pos.saturating_sub(SPLIT_MARGIN)
                && column <= hit.pos.saturating_add(SPLIT_MARGIN)
                && row >= hit.area_y
                && row < hit.area_y.saturating_add(hit.area_h)
            {
                app.resize_target = Some(ResizeTarget::Split);
                app.mouse.dragging = false;
                return true;
            }
        } else if row >= hit.pos.saturating_sub(SPLIT_MARGIN)
            && row <= hit.pos.saturating_add(SPLIT_MARGIN)
            && column >= hit.area_x
            && column < hit.area_x.saturating_add(hit.area_w)
        {
            app.resize_target = Some(ResizeTarget::Split);
            app.mouse.dragging = false;
            return true;
        }
    }

    if app.explorer.open {
        let sep = app.explorer_separator_x;
        if column >= sep.saturating_sub(HIT_MARGIN) && column <= sep.saturating_add(HIT_MARGIN) {
            app.resize_target = Some(ResizeTarget::Explorer);
            app.mouse.dragging = false;
            return true;
        }
    }

    if app.terminal.open && !app.terminal.full_panel {
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
    // text_x / text_y are set during draw to the first content cell of the editor.
    let text_x = if vp.text_x == 0 && vp.width > 0 {
        vp.x.saturating_add(LINE_NO_WIDTH)
    } else {
        vp.text_x
    };
    let text_y = vp.text_y;

    let max_x = vp.x.saturating_add(vp.width.saturating_sub(1));
    let max_y = vp.y.saturating_add(vp.height.saturating_sub(1));

    let clamped_col = column.max(text_x).min(max_x);
    let clamped_row = row.max(text_y).min(max_y);

    let col_in_seg = clamped_col.saturating_sub(text_x) as usize;

    // Per-frame map: screen row → (buffer row, visual-col base within that line).
    // Built in draw_editor with the same soft-wrap geometry as rendering.
    let local_y = clamped_row.saturating_sub(text_y) as usize;
    let last = app.buffer.line_count().saturating_sub(1);
    let (buffer_row, visual_base) = if let Some(&br) = app.screen_row_to_buffer.get(local_y) {
        let base = app
            .screen_row_visual_base
            .get(local_y)
            .copied()
            .unwrap_or(0);
        (br, base)
    } else if let Some(&br) = app.screen_row_to_buffer.last() {
        let base = app.screen_row_visual_base.last().copied().unwrap_or(0);
        (br, base)
    } else {
        ((local_y + app.scroll).min(last), 0)
    };

    let visual_col = visual_base.saturating_add(col_in_seg);
    let col = app.buffer.screen_col_to_buffer_col(buffer_row, visual_col);
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
    // Record macro keys (after filtering pure recording control)
    let recording = app.macros.is_recording() && !app.replaying_macro;

    // ── Pane terminal (Ctrl+Shift+T): strict PTY policy ─────────────────
    // When the terminal *window* is focused, almost every key goes to the
    // child shell (Ctrl+C, arrows, …). Only a tiny allowlist is editor chrome.
    // Must run before clipboard / Ctrl+S / Cmd+C handlers.
    if app.terminal_window_focused() && matches!(app.mode, Mode::Normal | Mode::Insert) {
        if handle_pane_terminal_window(app, code, modifiers) {
            let was_recording = app.macros.is_recording() && !app.replaying_macro;
            if was_recording {
                if let Some(mk) = key_to_macro(code, modifiers) {
                    app.macros.push(mk);
                }
            }
            return;
        }
        // false → allowlisted editor action already handled (e.g. split chord
        // continues below for Ctrl+W second key). Fall through only for that.
    }

    // Cmd (macOS) or Ctrl+Shift (common terminal) clipboard shortcuts
    let cmd_like = modifiers.contains(KeyModifiers::SUPER)
        || (modifiers.contains(KeyModifiers::CONTROL) && modifiers.contains(KeyModifiers::SHIFT));
    if cmd_like {
        match code {
            KeyCode::Char('c') | KeyCode::Char('C') => {
                // cmd_like+C → copy. Shift handled upstream as paste-preview; C is plain copy.
                if !matches!(
                    app.mode,
                    Mode::Terminal
                        | Mode::XlcInput
                        | Mode::Search
                        | Mode::Palette
                        | Mode::SourceControl
                ) {
                    app.clipboard_copy();
                }
                return;
            }
            KeyCode::Char('v') | KeyCode::Char('V') => {
                // Shift+V under cmd_like → pretty preview toggle (VS Code Markdown preview).
                if modifiers.contains(KeyModifiers::SHIFT) {
                    if matches!(
                        app.mode,
                        Mode::Normal | Mode::Insert | Mode::Preview | Mode::Explorer
                    ) {
                        app.toggle_preview();
                    }
                    return;
                }
                // plain cmd_like+V → paste
                if !matches!(
                    app.mode,
                    Mode::Terminal
                        | Mode::XlcInput
                        | Mode::Search
                        | Mode::Palette
                        | Mode::SourceControl
                        | Mode::Preview
                ) {
                    app.clipboard_paste();
                }
                return;
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                if matches!(app.mode, Mode::Visual | Mode::VisualLine | Mode::VisualBlock) {
                    if app.mode == Mode::VisualBlock {
                        app.delete_block();
                    } else {
                        app.delete_selection();
                    }
                    app.message = String::from("Cut to clipboard");
                }
                return;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                // Cmd/Ctrl+Shift+P — command palette (VS Code)
                if modifiers.contains(KeyModifiers::SHIFT) {
                    app.open_command_palette();
                    return;
                }
                // Cmd+P alone → file palette
                if modifiers.contains(KeyModifiers::SUPER) {
                    app.open_file_palette();
                    return;
                }
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                // Ctrl+Shift+F — find in files
                if modifiers.contains(KeyModifiers::SHIFT)
                    && matches!(
                        app.mode,
                        Mode::Normal | Mode::Insert | Mode::WorkspaceSearch | Mode::Explorer
                    )
                {
                    app.open_workspace_search();
                    return;
                }
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                // Ctrl+Shift+T — full-panel terminal (editor slot)
                if modifiers.contains(KeyModifiers::SHIFT)
                    && matches!(
                        app.mode,
                        Mode::Normal
                            | Mode::Insert
                            | Mode::Terminal
                            | Mode::Explorer
                            | Mode::GitWorkbench
                    )
                {
                    app.toggle_terminal_full();
                    return;
                }
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                // Ctrl+Shift+O — document symbols
                if modifiers.contains(KeyModifiers::SHIFT)
                    && matches!(app.mode, Mode::Normal | Mode::Insert | Mode::Explorer)
                {
                    app.open_document_symbols();
                    return;
                }
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                // Ctrl+Shift+I — format document (VS Code-ish)
                if modifiers.contains(KeyModifiers::SHIFT)
                    && matches!(app.mode, Mode::Normal | Mode::Insert)
                {
                    app.format_document();
                    return;
                }
            }

            KeyCode::Char('g') | KeyCode::Char('G') => {
                // Ctrl+Shift+G — full Git workbench
                // Ctrl+G — light Source Control
                let git_modes = matches!(
                    app.mode,
                    Mode::Normal
                        | Mode::Insert
                        | Mode::Visual
                        | Mode::VisualLine
                        | Mode::VisualBlock
                        | Mode::SourceControl
                        | Mode::GitWorkbench
                        | Mode::Explorer
                );
                if !git_modes {
                    return;
                }
                if modifiers.contains(KeyModifiers::SHIFT) {
                    app.toggle_git_workbench();
                } else {
                    app.toggle_scm();
                }
                return;
            }
            KeyCode::Char(',') => {
                // Ctrl/Cmd+, — Settings (VS Code convention)
                if matches!(
                    app.mode,
                    Mode::Normal | Mode::Insert | Mode::Settings | Mode::Explorer
                ) {
                    app.open_settings();
                }
                return;
            }
            _ => {}
        }
    }

    // Editor right-click menu keyboard navigation
    if app.editor_ctx.is_some() {
        match code {
            KeyCode::Esc => {
                app.close_editor_ctx();
                app.message = String::new();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(m) = app.editor_ctx.as_mut() {
                    m.sel = m.sel.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(m) = app.editor_ctx.as_mut() {
                    let max = m.items.len().saturating_sub(1);
                    m.sel = (m.sel + 1).min(max);
                }
            }
            KeyCode::Enter => match app.run_editor_ctx_action() {
                Ok(msg) => app.message = msg,
                Err(e) => app.message = e,
            },
            _ => {}
        }
        return;
    }

    // Peek overlay captures keys while open (before mode dispatch).
    if app.peek.open {
        match code {
            KeyCode::Esc => {
                app.peek.close();
                app.message = String::new();
                return;
            }
            KeyCode::Enter => {
                app.promote_peek();
                return;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.peek.scroll_by(1);
                return;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.peek.scroll_by(-1);
                return;
            }
            _ => {}
        }
    }

    // Ctrl+W split chords (vim-style). Works even when a terminal *window*
    // is focused — pane terminal is not Mode::Terminal.
    if app.split.pending_chord
        && matches!(app.mode, Mode::Normal)
        && !modifiers.contains(KeyModifiers::CONTROL)
    {
        app.split.pending_chord = false;
        app.pending_hints.clear();
        match code {
            KeyCode::Char('v') => app.split_vertical(),
            KeyCode::Char('s') => app.split_horizontal(),
            KeyCode::Char('w') | KeyCode::Char('W') => app.focus_other_pane(),
            KeyCode::Char('q') => app.close_split(),
            KeyCode::Char('=') => {
                app.split.equalize();
                app.message = String::from("Split equalized");
            }
            KeyCode::Char('h') | KeyCode::Left => app.focus_dir('h'),
            KeyCode::Char('l') | KeyCode::Right => app.focus_dir('l'),
            KeyCode::Char('k') | KeyCode::Up => app.focus_dir('k'),
            KeyCode::Char('j') | KeyCode::Down => app.focus_dir('j'),
            KeyCode::Char('>') => app.split.adjust_ratio(0.05),
            KeyCode::Char('<') => app.split.adjust_ratio(-0.05),
            KeyCode::Esc => {
                app.message.clear();
            }
            _ => {
                app.message = String::from("Ctrl+W: v/s split · w cycle · q close · = equal");
            }
        }
        return;
    }

    // Normal: Ctrl+V → visual block (vim). Insert: Ctrl+V → paste.
    if modifiers.contains(KeyModifiers::CONTROL)
        && !modifiers.contains(KeyModifiers::SHIFT)
        && !modifiers.contains(KeyModifiers::SUPER)
    {
        if matches!(code, KeyCode::Char('w') | KeyCode::Char('W')) && app.mode == Mode::Normal {
            app.split.pending_chord = true;
            app.begin_chord(
                "Ctrl+W",
                xei_core::which_key::as_hints(xei_core::which_key::map_ctrl_w()),
            );
            app.message = String::from("Ctrl+W —");
            return;
        }
        if matches!(code, KeyCode::Char('.'))
            && matches!(app.mode, Mode::Normal | Mode::Insert)
        {
            // Ctrl+. — code actions / quick fix
            app.request_code_actions();
            return;
        }
        if matches!(code, KeyCode::Char('v') | KeyCode::Char('V')) {
            if app.mode == Mode::Insert {
                app.clipboard_paste();
                return;
            }
            if app.mode == Mode::Normal {
                app.enter_visual_block();
                return;
            }
        }
    }

    // Macro stop: second `q` in normal (not recording into push of stop key)
    // Handled in handle_normal — here we push recorded keys after handling

    let _ = recording; // used after dispatch

    // ── DAP debug function keys (any editor mode) ───────────────────
    if matches!(
        app.mode,
        Mode::Normal | Mode::Insert | Mode::Debug | Mode::Visual | Mode::VisualLine
    ) {
        match code {
            KeyCode::F(5) => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    app.dap_stop();
                } else {
                    app.dap_start_or_continue();
                }
                return;
            }
            KeyCode::F(6) => {
                app.dap_pause();
                return;
            }
            KeyCode::F(9) => {
                app.dap_toggle_breakpoint();
                return;
            }
            KeyCode::F(10) => {
                app.dap_step_over();
                return;
            }
            KeyCode::F(11) => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    app.dap_step_out();
                } else {
                    app.dap_step_into();
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
            // Ctrl+T alone (no Shift) — side panel terminal
            if !modifiers.contains(KeyModifiers::SHIFT) {
                app.toggle_terminal_side();
                return;
            }
        }

        if app.mode == Mode::Search {
            // Don't steal search input for panel shortcuts (except Esc via mode handler).
            // Ctrl+G style cancels.
            if matches!(code, KeyCode::Char('c') | KeyCode::Char('g')) {
                app.cancel_search();
                return;
            }
        } else if app.mode != Mode::Terminal {
            // Multi-cursor: Ctrl+D add next match · Ctrl+Alt+j/k column carets
            if matches!(code, KeyCode::Char('d') | KeyCode::Char('D'))
                && !modifiers.contains(KeyModifiers::SHIFT)
                && matches!(app.mode, Mode::Normal | Mode::Insert)
            {
                app.multi_cursor_add_next();
                return;
            }
            if modifiers.contains(KeyModifiers::ALT)
                && matches!(app.mode, Mode::Normal | Mode::Insert)
            {
                match code {
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.multi_cursor_add_below();
                        return;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.multi_cursor_add_above();
                        return;
                    }
                    _ => {}
                }
            }
            match code {
            KeyCode::Char('e') => {
                if app.mode == Mode::XlcInput {
                    app.close_xlc();
                    app.enter_normal();
                } else {
                    app.enter_xlc(None);
                }
                return;
            }
            KeyCode::Char('s') => {
                // Ctrl+S save
                if matches!(app.mode, Mode::Normal | Mode::Insert) {
                    app.save_file();
                }
                return;
            }
            KeyCode::Char('r') => {
                // Ctrl+R redo
                if app.mode == Mode::Normal {
                    app.redo();
                }
                return;
            }
            KeyCode::Char('o') => {
                // Ctrl+O jump back
                if app.mode == Mode::Normal {
                    app.jump_back();
                }
                return;
            }
            KeyCode::Char('i') => {
                // Ctrl+I jump forward
                if app.mode == Mode::Normal {
                    app.jump_forward();
                }
                return;
            }
            KeyCode::Char('p') => {
                // Ctrl+P — quick open files (VS Code)
                if app.mode == Mode::Normal || app.mode == Mode::Insert {
                    app.open_file_palette();
                }
                return;
            }
            KeyCode::Char('a') => {
                if app.mode == Mode::Insert {
                    trigger_completion(app);
                }
                return;
            }
            KeyCode::Char('g') => {
                // Ctrl+G — light SCM (from workbench: step back to SCM)
                if matches!(
                    app.mode,
                    Mode::Normal
                        | Mode::Insert
                        | Mode::SourceControl
                        | Mode::GitWorkbench
                        | Mode::Explorer
                ) {
                    app.toggle_scm();
                }
                return;
            }
            KeyCode::Char('d') | KeyCode::Char('D')
                if modifiers.contains(KeyModifiers::SHIFT) =>
            {
                // Ctrl+Shift+D — debug panel (VS Code-ish)
                if matches!(
                    app.mode,
                    Mode::Normal | Mode::Insert | Mode::Debug | Mode::Explorer
                ) {
                    app.toggle_debug_panel();
                }
                return;
            }
            KeyCode::Char(',') => {
                // Ctrl+, without requiring Shift (some terminals only send CONTROL)
                if matches!(
                    app.mode,
                    Mode::Normal | Mode::Insert | Mode::Settings | Mode::Explorer
                ) {
                    app.open_settings();
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
            KeyCode::Char('b') | KeyCode::Char('B') => {
                if app.mode == Mode::XlcInput {
                    app.xlc.scroll_up(8);
                    return;
                }
                // Ctrl+B — git blame side panel (slide-in, flame colors)
                if matches!(
                    app.mode,
                    Mode::Normal | Mode::Insert | Mode::Visual | Mode::VisualLine | Mode::Explorer
                ) {
                    app.toggle_blame();
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

    // Snapshot whether we were recording before this key
    let was_recording = app.macros.is_recording() && !app.replaying_macro;
    let reg_before = app.macros.recording;

    match app.mode {
        Mode::Normal => handle_normal(app, code),
        Mode::Insert => handle_insert(app, code),
        Mode::Visual | Mode::VisualLine | Mode::VisualBlock => handle_visual(app, code),
        Mode::XlcInput => handle_xlc(app, code),
        Mode::Search => handle_search_input(app, code),
        Mode::Explorer => handle_explorer(app, code),
        Mode::Terminal => handle_terminal(app, code),
        Mode::Palette => handle_palette(app, code),
        Mode::SourceControl => handle_scm(app, code),
        Mode::GitWorkbench => handle_git_workbench(app, code),
        Mode::Settings => handle_settings(app, code),
        Mode::Preview => handle_preview(app, code),
        Mode::WorkspaceSearch => handle_workspace_search(app, code),
        Mode::Screensaver => handle_screensaver(app, code),
        Mode::Debug => handle_debug(app, code),
        Mode::CallHierarchy => handle_call_hierarchy(app, code),
        Mode::Rebase => handle_rebase(app, code),
        Mode::PrReview => handle_pr_review(app, code),
    }

    // Append to macro buffer (skip the `q` that starts/stops recording)
    if was_recording && app.macros.is_recording() {
        if let Some(mk) = key_to_macro(code, modifiers) {
            // Don't record nested @ playback
            app.macros.push(mk);
        }
    } else if was_recording && !app.macros.is_recording() {
        // stopped — buffer already stored without the stopping q if we stop before push
        let _ = reg_before;
    }
}

fn key_to_macro(code: KeyCode, modifiers: KeyModifiers) -> Option<MacroKey> {
    if modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char(c) = code {
            return Some(MacroKey::Ctrl(c.to_ascii_lowercase()));
        }
    }
    if modifiers.contains(KeyModifiers::SUPER) {
        if let KeyCode::Char(c) = code {
            return Some(MacroKey::Super(c.to_ascii_lowercase()));
        }
    }
    Some(match code {
        KeyCode::Char(c) => MacroKey::Char(c),
        KeyCode::Esc => MacroKey::Esc,
        KeyCode::Enter => MacroKey::Enter,
        KeyCode::Backspace => MacroKey::Backspace,
        KeyCode::Tab => MacroKey::Tab,
        KeyCode::Up => MacroKey::Up,
        KeyCode::Down => MacroKey::Down,
        KeyCode::Left => MacroKey::Left,
        KeyCode::Right => MacroKey::Right,
        KeyCode::Home => MacroKey::Home,
        KeyCode::End => MacroKey::End,
        KeyCode::PageUp => MacroKey::PageUp,
        KeyCode::PageDown => MacroKey::PageDown,
        KeyCode::Delete => MacroKey::Delete,
        _ => return None,
    })
}

fn play_macro(app: &mut App, name: char) {
    let keys: Vec<MacroKey> = match app.macros.get(name) {
        Some(k) => k.to_vec(),
        None => {
            app.message = format!("Macro '{}' empty", name);
            return;
        }
    };
    app.macros.set_last_played(name);
    app.replaying_macro = true;
    for k in keys {
        let (code, mods) = macro_to_key(&k);
        handle_key(app, code, mods);
    }
    app.replaying_macro = false;
    app.message = format!("Played @{}", name);
}

fn macro_to_key(k: &MacroKey) -> (KeyCode, KeyModifiers) {
    match k {
        MacroKey::Char(c) => (KeyCode::Char(*c), KeyModifiers::NONE),
        MacroKey::Esc => (KeyCode::Esc, KeyModifiers::NONE),
        MacroKey::Enter => (KeyCode::Enter, KeyModifiers::NONE),
        MacroKey::Backspace => (KeyCode::Backspace, KeyModifiers::NONE),
        MacroKey::Tab => (KeyCode::Tab, KeyModifiers::NONE),
        MacroKey::Up => (KeyCode::Up, KeyModifiers::NONE),
        MacroKey::Down => (KeyCode::Down, KeyModifiers::NONE),
        MacroKey::Left => (KeyCode::Left, KeyModifiers::NONE),
        MacroKey::Right => (KeyCode::Right, KeyModifiers::NONE),
        MacroKey::Home => (KeyCode::Home, KeyModifiers::NONE),
        MacroKey::End => (KeyCode::End, KeyModifiers::NONE),
        MacroKey::PageUp => (KeyCode::PageUp, KeyModifiers::NONE),
        MacroKey::PageDown => (KeyCode::PageDown, KeyModifiers::NONE),
        MacroKey::Delete => (KeyCode::Delete, KeyModifiers::NONE),
        MacroKey::Ctrl(c) => (KeyCode::Char(*c), KeyModifiers::CONTROL),
        MacroKey::Super(c) => (KeyCode::Char(*c), KeyModifiers::SUPER),
    }
}

fn handle_palette(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.palette.close();
            app.mode = Mode::Normal;
            app.message = String::new();
        }
        KeyCode::Enter => app.execute_palette_selection(),
        KeyCode::Down | KeyCode::Char('j') if app.palette.query.is_empty() => {
            app.palette.move_down();
        }
        KeyCode::Up | KeyCode::Char('k') if app.palette.query.is_empty() => {
            app.palette.move_up();
        }
        KeyCode::Down => app.palette.move_down(),
        KeyCode::Up => app.palette.move_up(),
        KeyCode::Backspace => app.palette.pop_char(),
        KeyCode::Char(c) if !c.is_control() => app.palette.push_char(c),
        _ => {}
    }
}

fn handle_workspace_search(app: &mut App, code: KeyCode) {
    use xei_core::workspace_search::replace_in_file;

    match code {
        KeyCode::Esc => {
            app.workspace_search.close();
            app.mode = Mode::Normal;
            app.message = String::new();
        }
        KeyCode::Tab => {
            app.workspace_search.toggle_replace_focus();
        }
        KeyCode::Backspace => {
            app.workspace_search.pop_char();
            if !app.workspace_search.replace_focus {
                app.workspace_search.run_search();
            }
        }
        KeyCode::Down | KeyCode::Char('j')
            if app.workspace_search.replace_focus || app.workspace_search.query.is_empty() =>
        {
            app.workspace_search.move_sel(1);
        }
        KeyCode::Down => app.workspace_search.move_sel(1),
        KeyCode::Up | KeyCode::Char('k') => app.workspace_search.move_sel(-1),
        KeyCode::Enter => {
            if app.workspace_search.needs_search || app.workspace_search.hits.is_empty() {
                app.workspace_search.run_search();
            }
            if let Some(hit) = app.workspace_search.selected_hit().cloned() {
                app.workspace_search.close();
                app.mode = Mode::Normal;
                app.goto_file_location(
                    &hit.path.display().to_string(),
                    hit.row,
                    hit.col,
                );
            }
        }
        KeyCode::Char('r') if !app.workspace_search.replace_focus => {
            // replace one at selection
            let q = app.workspace_search.query.clone();
            let repl = app.workspace_search.replace.clone();
            if q.is_empty() {
                app.workspace_search.status = "Nothing to replace".into();
                return;
            }
            if let Some(hit) = app.workspace_search.selected_hit().cloned() {
                match replace_in_file(&hit.path, hit.row, &q, &repl) {
                    Ok(true) => {
                        // reload if open
                        if app.filename.as_ref() == Some(&hit.path) {
                            if let Ok(content) = std::fs::read_to_string(&hit.path) {
                                app.push_undo();
                                app.buffer = xei_core::buffer::Buffer::from_string(&content);
                                app.modified = false;
                            }
                        }
                        app.workspace_search.run_search();
                        app.workspace_search.status =
                            format!("Replaced in {}", hit.path.display());
                    }
                    Ok(false) => {
                        app.workspace_search.status = "Pattern not found on line".into();
                    }
                    Err(e) => {
                        app.workspace_search.status = e;
                    }
                }
            }
        }
        KeyCode::Char('R') if !app.workspace_search.replace_focus => {
            let q = app.workspace_search.query.clone();
            let repl = app.workspace_search.replace.clone();
            if q.is_empty() {
                return;
            }
            let hits = app.workspace_search.hits.clone();
            let mut n = 0usize;
            for hit in &hits {
                if replace_in_file(&hit.path, hit.row, &q, &repl).unwrap_or(false) {
                    n += 1;
                }
            }
            app.workspace_search.run_search();
            app.workspace_search.status = format!("Replaced {n} occurrence(s)");
        }
        KeyCode::Char(c) if !c.is_control() => {
            app.workspace_search.push_char(c);
            if !app.workspace_search.replace_focus {
                // live search for short queries; debounce by always running (rg is fast)
                if app.workspace_search.query.len() >= 2 {
                    app.workspace_search.run_search();
                }
            }
        }
        _ => {}
    }
}

fn handle_settings(app: &mut App, code: KeyCode) {
    use xei_core::settings::{SettingsAction, SettingsPage};

    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.close_settings();
        }
        KeyCode::Tab => app.settings.next_page(),
        KeyCode::BackTab => app.settings.prev_page(),
        KeyCode::Down | KeyCode::Char('j') => app.settings.move_sel(1),
        KeyCode::Up | KeyCode::Char('k') => app.settings.move_sel(-1),
        KeyCode::Left | KeyCode::Char('h') => {
            let (mx, my) = app.pet_pos_max();
            if matches!(
                app.settings.nudge_pet(-1, mx, my),
                xei_core::settings::SettingsAction::ApplyPet
            ) {
                app.apply_settings_draft();
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            let (mx, my) = app.pet_pos_max();
            if matches!(
                app.settings.nudge_pet(1, mx, my),
                xei_core::settings::SettingsAction::ApplyPet
            ) {
                app.apply_settings_draft();
            }
        }
        KeyCode::PageDown => {
            for _ in 0..8 {
                app.settings.move_sel(1);
            }
        }
        KeyCode::PageUp => {
            for _ in 0..8 {
                app.settings.move_sel(-1);
            }
        }
        KeyCode::Enter | KeyCode::Char(' ') => match app.settings.activate() {
            SettingsAction::ApplyTheme => app.apply_settings_draft(),
            SettingsAction::ApplyGpuAcc => {
                app.apply_settings_draft();
                // Never sticky-undercurl the whole session (paints waves on empty cells).
                crate::gpu_frame::reset_underline_sgr_stdout();
                app.message = if app.gpu_acc {
                    if app.gpu_active() {
                        format!("gpu_acc on · {}", app.term_caps_summary)
                    } else {
                        "gpu_acc on · host is basic (limited enhancements)".into()
                    }
                } else {
                    "gpu_acc off — plain cell TUI".into()
                };
            }
            SettingsAction::ApplyLsp => {
                app.apply_settings_draft();
                app.message = app
                    .settings
                    .status
                    .clone()
                    .unwrap_or_else(|| "LSP settings applied".into());
            }
            SettingsAction::ApplyPet => {
                let force = app
                    .settings
                    .status
                    .as_deref()
                    .is_some_and(|s| s.starts_with("Reloading"));
                // Keep draft x/y as saved — paint clamps for the live terminal.
                app.apply_settings_draft();
                if force {
                    let p = xei_core::pet::expand_path(&app.settings.draft.pet_path);
                    let ps = p.display().to_string();
                    if !app.settings.draft.pet_path.is_empty() {
                        app.pet.load_path(&ps);
                        app.pet.enabled =
                            app.settings.draft.pet_enabled && app.pet.has_frames();
                    }
                }
                app.message = app
                    .settings
                    .status
                    .clone()
                    .unwrap_or_else(|| "Pet settings applied".into());
            }
            SettingsAction::OpenWorkbench => {
                app.close_settings();
                app.open_git_workbench();
            }
            SettingsAction::OpenScm => {
                app.close_settings();
                app.toggle_scm();
            }
            SettingsAction::None => {}
        },
        KeyCode::Char('s') | KeyCode::Char('S') => {
            app.save_settings();
        }
        KeyCode::Char('1') => {
            app.settings.page = SettingsPage::About;
            app.settings.selected = 0;
        }
        KeyCode::Char('2') => {
            app.settings.page = SettingsPage::Setting;
            app.settings.selected = 1; // first theme
        }
        KeyCode::Char('3') => {
            app.settings.page = SettingsPage::Pet;
            app.settings.selected = 0;
        }
        KeyCode::Char('4') => {
            app.settings.page = SettingsPage::Help;
            app.settings.selected = xei_core::settings::help_entries()
                .iter()
                .position(|e| !e.is_header)
                .unwrap_or(0);
        }
        _ => {}
    }
}

fn handle_screensaver(app: &mut App, code: KeyCode) {
    // Cryptex password entry (`/` easter egg)
    if app.screensaver.cryptex_input {
        match code {
            KeyCode::Esc => {
                app.screensaver.cancel_cryptex_input();
                app.message = "cryptex locked".into();
            }
            KeyCode::Enter => {
                if app.screensaver.submit_cryptex() {
                    app.message = "god.".into();
                } else {
                    app.message = "…incorrect combination".into();
                }
            }
            KeyCode::Backspace => {
                app.screensaver.cryptex_backspace();
            }
            KeyCode::Char(c) if !c.is_control() => {
                app.screensaver.cryptex_push(c);
            }
            _ => {}
        }
        return;
    }

    match code {
        // `/` → type into the cryptex (Da Vinci Code easter egg)
        KeyCode::Char('/') => {
            app.screensaver.begin_cryptex_input();
            app.message = "cryptex · enter the combination  (Esc cancel)".into();
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.toggle_screensaver();
        }
        // Other keys still dismiss (classic screensaver UX)
        _ => {
            app.toggle_screensaver();
        }
    }
}

fn handle_pr_review(app: &mut App, code: KeyCode) {
    use xei_core::pr_review::PrReviewFocus;
    // Root captured at open time; fall back for sessions opened another way.
    let root = app.pr_review.root.clone().or_else(|| {
        app.filename
            .as_ref()
            .and_then(|p| xei_core::git_ops::find_git_root(Some(p)))
    });
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.pr_review.close();
            // return to git workbench if it was open
            if app.git_wb.open {
                app.mode = Mode::GitWorkbench;
            } else {
                app.mode = Mode::Normal;
            }
            app.message = String::new();
        }
        KeyCode::Tab => {
            app.pr_review.focus = app.pr_review.focus.next();
            app.message = match app.pr_review.focus {
                PrReviewFocus::Files => "PR · files".into(),
                PrReviewFocus::Comments => "PR · comments".into(),
                PrReviewFocus::Body => "PR · description".into(),
            };
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.pr_review.move_sel(1);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.pr_review.move_sel(-1);
        }
        KeyCode::PageDown | KeyCode::Char('d') if app.pr_review.focus == PrReviewFocus::Files => {
            app.pr_review.diff_scroll = app.pr_review.diff_scroll.saturating_add(8);
        }
        KeyCode::PageUp | KeyCode::Char('u') if app.pr_review.focus == PrReviewFocus::Files => {
            app.pr_review.diff_scroll = app.pr_review.diff_scroll.saturating_sub(8);
        }
        KeyCode::Char('J') => {
            app.pr_review.diff_scroll = app.pr_review.diff_scroll.saturating_add(3);
        }
        KeyCode::Char('K') => {
            app.pr_review.diff_scroll = app.pr_review.diff_scroll.saturating_sub(3);
        }
        KeyCode::Enter | KeyCode::Char('o') => {
            if let Some(path) = app.pr_review.selected_file_path().map(|s| s.to_string()) {
                // Try open file from worktree
                if let Some(ref r) = root {
                    let full = r.join(&path);
                    if full.is_file() {
                        app.open_new_tab(&full.display().to_string());
                        app.pr_review.close();
                        app.mode = Mode::Normal;
                        app.message = format!("Opened {path}");
                    } else {
                        app.message = format!("Not in worktree: {path}");
                    }
                }
            }
        }
        KeyCode::Char('c') => {
            // checkout this PR
            if app.pr_review.number > 0 {
                if let Some(ref r) = root {
                    match xei_core::gh::pr_checkout(r, app.pr_review.number) {
                        Ok(m) => {
                            app.message = m;
                            app.refresh_git();
                        }
                        Err(e) => app.message = e,
                    }
                }
            }
        }
        KeyCode::Char('b') => {
            if !app.pr_review.url.is_empty() {
                app.message = match xei_core::gh::open_in_browser(&app.pr_review.url) {
                    Ok(m) => m,
                    Err(e) => e,
                };
            }
        }
        KeyCode::Char('1') => app.pr_review.focus = PrReviewFocus::Files,
        KeyCode::Char('2') => app.pr_review.focus = PrReviewFocus::Comments,
        KeyCode::Char('3') => app.pr_review.focus = PrReviewFocus::Body,
        _ => {}
    }
}

fn handle_call_hierarchy(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.call_hierarchy.close();
            app.mode = Mode::Normal;
            app.message = String::new();
        }
        KeyCode::Down | KeyCode::Char('j') => app.call_hierarchy.move_sel(1),
        KeyCode::Up | KeyCode::Char('k') => app.call_hierarchy.move_sel(-1),
        KeyCode::Tab | KeyCode::Char('t') => {
            app.toggle_call_direction();
        }
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Char('o') => {
            if let Some(item) = app.call_hierarchy.selected_item().cloned() {
                if !item.path.is_empty() && std::path::Path::new(&item.path).is_file() {
                    app.push_jump();
                    app.open_new_tab(&item.path);
                    app.buffer.cursor.row =
                        item.row.min(app.buffer.line_count().saturating_sub(1));
                    app.buffer.cursor.col = item.col;
                    app.buffer.clamp_col();
                    app.update_scroll();
                    app.call_hierarchy.close();
                    app.mode = Mode::Normal;
                    app.message = format!("→ {} · {}:{}", item.name, item.path, item.row + 1);
                }
            }
        }
        KeyCode::Char('i') => {
            // force incoming
            app.call_hierarchy.direction = xei_core::call_hierarchy::CallDirection::Outgoing;
            app.toggle_call_direction();
        }
        KeyCode::Char('O') => {
            app.call_hierarchy.direction = xei_core::call_hierarchy::CallDirection::Incoming;
            app.toggle_call_direction();
        }
        _ => {}
    }
}

fn handle_rebase(app: &mut App, code: KeyCode) {
    use xei_core::rebase::RebaseAction;
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.rebase.close();
            app.mode = Mode::Normal;
            app.message = "Rebase cancelled".into();
        }
        KeyCode::Down | KeyCode::Char('j') => app.rebase.move_sel(1),
        KeyCode::Up | KeyCode::Char('k') => app.rebase.move_sel(-1),
        KeyCode::Tab | KeyCode::Char(' ') => app.rebase.cycle_action(),
        KeyCode::Char('K') => app.rebase.move_entry(-1),
        KeyCode::Char('J') => app.rebase.move_entry(1),
        KeyCode::Char(c) if RebaseAction::from_char(c).is_some() => {
            if let Some(a) = RebaseAction::from_char(c) {
                app.rebase.set_action(a);
            }
        }
        KeyCode::Enter => {
            app.run_rebase_plan();
        }
        KeyCode::Char('!') => {
            // force run
            app.run_rebase_plan();
        }
        _ => {}
    }
}

/// DAP debugger panel (Ctrl+Shift+D / F5).
/// Esc unfocuses (panel stays); `q` closes the panel.
fn handle_debug(app: &mut App, code: KeyCode) {
    use xei_core::dap::DebugPane;

    match code {
        KeyCode::Esc => {
            // Keep panel visible — just return focus to the editor.
            app.mode = Mode::Normal;
            app.message = "Debug unfocused · Ctrl+Shift+D refocus · q closes".into();
        }
        KeyCode::Char('q') => {
            app.close_debug_panel();
        }
        KeyCode::Tab => {
            app.dap.set_pane(app.dap.pane.next());
            app.message = format!("Debug · {}", app.dap.pane.label());
        }
        KeyCode::BackTab => {
            app.dap.set_pane(app.dap.pane.prev());
        }
        KeyCode::Down | KeyCode::Char('j') => app.dap.move_focus(1),
        KeyCode::Up | KeyCode::Char('k') => app.dap.move_focus(-1),
        KeyCode::Enter | KeyCode::Char('l') => match app.dap.pane {
            DebugPane::Stack => {
                let i = app.dap.focus_row;
                app.dap.select_frame(i);
                app.dap.location_dirty = true;
                app.dap_apply_stopped_location();
            }
            DebugPane::Variables => {
                let i = app.dap.focus_row;
                app.dap.toggle_var_at(i);
            }
            // Enter also handled below for console
            DebugPane::Breakpoints => {
                let bps = app.dap.flat_bps();
                if let Some((path, line, _)) = bps.get(app.dap.focus_row) {
                    let path = path.clone();
                    let line = *line;
                    if std::path::Path::new(&path).is_file() {
                        app.open_new_tab(&path);
                        app.buffer.cursor.row =
                            line.min(app.buffer.line_count().saturating_sub(1));
                        app.buffer.move_to_line_start();
                        app.update_scroll();
                    }
                }
            }
            DebugPane::Console => {
                let expr = app.dap.eval_input.clone();
                if !expr.is_empty() {
                    app.dap_evaluate(&expr);
                }
            }
        },
        // Collapse with `h` (tree navigation)
        KeyCode::Char('h') if app.dap.pane == DebugPane::Variables => {
            let i = app.dap.focus_row;
            if let Some(n) = app.dap.vars.get(i) {
                if n.expanded {
                    app.dap.toggle_var_at(i);
                }
            }
        }
        KeyCode::Char('c') if app.dap.pane != DebugPane::Console => app.dap_start_or_continue(),
        KeyCode::Char('n') if app.dap.pane != DebugPane::Console => app.dap_step_over(),
        KeyCode::Char('i') | KeyCode::Char('s')
            if app.dap.pane != DebugPane::Console =>
        {
            app.dap_step_into()
        }
        KeyCode::Char('o') if app.dap.pane != DebugPane::Console => app.dap_step_out(),
        KeyCode::Char('p') if app.dap.pane != DebugPane::Console => app.dap_pause(),
        KeyCode::Char('r') if app.dap.pane != DebugPane::Console => {
            if let Err(e) = app.dap.restart() {
                app.message = e;
            } else {
                app.message = "▶ restart".into();
            }
        }
        KeyCode::Char('x') | KeyCode::Char('S')
            if app.dap.pane != DebugPane::Console =>
        {
            app.dap_stop()
        }
        KeyCode::Char('b') if app.dap.pane != DebugPane::Console => app.dap_toggle_breakpoint(),
        KeyCode::Char('1') => app.dap.set_pane(DebugPane::Stack),
        KeyCode::Char('2') => app.dap.set_pane(DebugPane::Variables),
        KeyCode::Char('3') => app.dap.set_pane(DebugPane::Breakpoints),
        KeyCode::Char('4') => app.dap.set_pane(DebugPane::Console),
        // Console REPL typing
        KeyCode::Char(c) if app.dap.pane == DebugPane::Console && !c.is_control() => {
            app.dap.eval_input.push(c);
        }
        KeyCode::Backspace if app.dap.pane == DebugPane::Console => {
            app.dap.eval_input.pop();
        }
        _ => {}
    }
}

/// Full Git workbench (Ctrl+Shift+G) — mini GitHub surface
fn handle_git_workbench(app: &mut App, code: KeyCode) {
    use xei_core::git_workbench::{GitFocus, GitTab, InputMode};

    // Inline input (new branch / confirm discard) takes over typing.
    if app.git_wb.input_mode.is_some() {
        match code {
            KeyCode::Esc => {
                app.git_wb.cancel_input();
                app.message = app.git_wb.message.clone().unwrap_or_default();
            }
            KeyCode::Enter => match app.git_wb.submit_input() {
                Ok(()) => {
                    app.message = app.git_wb.message.clone().unwrap_or_default();
                    app.refresh_git();
                }
                Err(e) => app.message = e,
            },
            KeyCode::Backspace => {
                app.git_wb.input_buf.pop();
            }
            KeyCode::Char(c) if !c.is_control() => {
                if matches!(app.git_wb.input_mode, Some(InputMode::NewBranch)) {
                    app.git_wb.input_buf.push(c);
                }
            }
            _ => {}
        }
        return;
    }

    // Commit context menu (right-click on Log)
    if app.git_wb.ctx_menu.is_some() {
        match code {
            KeyCode::Esc => {
                app.git_wb.close_ctx_menu();
                app.message = String::new();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(m) = app.git_wb.ctx_menu.as_mut() {
                    m.sel = m.sel.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(m) = app.git_wb.ctx_menu.as_mut() {
                    let max = m.items.len().saturating_sub(1);
                    m.sel = (m.sel + 1).min(max);
                }
            }
            KeyCode::Enter => match app.git_wb.run_ctx_action() {
                Ok(msg) => {
                    if let Some(h) = msg.strip_prefix("Copied ") {
                        let _ = xei_core::clipboard::copy(h);
                    }
                    app.message = msg;
                }
                Err(e) => app.message = e,
            },
            KeyCode::Char(c) => {
                let pick = app.git_wb.ctx_menu.as_ref().and_then(|m| {
                    m.items.iter().position(|it| match c {
                        's' | 'S' => matches!(it, xei_core::GitCtxItem::ShowFiles),
                        'c' | 'C' => matches!(it, xei_core::GitCtxItem::CherryPick),
                        'v' | 'V' => matches!(it, xei_core::GitCtxItem::Revert),
                        'y' | 'Y' => matches!(it, xei_core::GitCtxItem::CopyHash),
                        'o' | 'O' | 'b' | 'B' => {
                            matches!(it, xei_core::GitCtxItem::BrowseOnGitHub)
                        }
                        _ => false,
                    })
                });
                if let Some(idx) = pick {
                    if let Some(m) = app.git_wb.ctx_menu.as_mut() {
                        m.sel = idx;
                    }
                    match app.git_wb.run_ctx_action() {
                        Ok(msg) => {
                            if let Some(h) = msg.strip_prefix("Copied ") {
                                let _ = xei_core::clipboard::copy(h);
                            }
                            app.message = msg;
                        }
                        Err(e) => app.message = e,
                    }
                }
            }
            _ => {}
        }
        return;
    }

    // Commit message editing (left pane)
    if app.git_wb.commit_editing {
        match code {
            KeyCode::Esc => {
                app.git_wb.commit_editing = false;
                app.message = "Commit message done".into();
            }
            KeyCode::Enter => {
                app.git_wb.commit_editing = false;
                match app.git_wb.commit_with_buf() {
                    Ok(()) => {
                        app.message = app
                            .git_wb
                            .message
                            .clone()
                            .unwrap_or_else(|| "Committed".into());
                        app.refresh_git();
                    }
                    Err(e) => app.message = e,
                }
            }
            KeyCode::Backspace => {
                app.git_wb.commit_buf.pop();
            }
            KeyCode::Char(c) if !c.is_control() => {
                app.git_wb.commit_buf.push(c);
            }
            _ => {}
        }
        return;
    }

    match code {
        KeyCode::Esc => {
            if app.git_wb.pr_filter_mode {
                app.git_wb.pr_filter_mode = false;
                app.git_wb.pr_filter.clear();
                app.git_wb.refilter_prs();
                app.message = "Filter cleared".into();
                return;
            }
            if app.git_wb.issue_filter_mode {
                app.git_wb.issue_filter_mode = false;
                app.git_wb.issue_filter.clear();
                app.git_wb.refilter_issues();
                app.message = "Filter cleared".into();
                return;
            }
            if app.git_wb.ctx_menu.is_some() {
                app.git_wb.close_ctx_menu();
            } else if !app.git_wb.go_back() {
                app.close_git_workbench();
            }
        }
        // JetBrains dock: Tab cycles Changes | Log | Files panes
        KeyCode::Tab => {
            app.git_wb.cycle_pane();
            app.message = format!("Pane: {:?}", app.git_wb.pane);
        }
        KeyCode::BackTab => app.git_wb.prev_tab(),
        // Number keys switch surfaces. Docked panes: 1=Changes 2=Log 4=Files.
        // 5 opens/focuses Diff for the active column context (no tab thrash).
        KeyCode::Char(d) if d.is_ascii_digit() && d >= '1' && d <= '8' => {
            let n = d as u8 - b'0';
            match n {
                1 => {
                    app.git_wb.tab = GitTab::Status;
                    app.git_wb.pane = xei_core::git_workbench::GitPane::Changes;
                    app.git_wb.focus = GitFocus::List;
                    app.git_wb.ensure_tab_data();
                    app.message = "Git · Changes".into();
                }
                2 => {
                    app.git_wb.tab = GitTab::History;
                    app.git_wb.pane = xei_core::git_workbench::GitPane::Log;
                    app.git_wb.focus = GitFocus::List;
                    app.git_wb.ensure_tab_data();
                    app.message = "Git · Log".into();
                }
                3 => {
                    app.git_wb.tab = GitTab::Branches;
                    app.git_wb.focus = GitFocus::List;
                    app.git_wb.ensure_tab_data();
                    app.message = "Git · Branches".into();
                }
                4 => {
                    // Focus docked Files column; load commit detail quietly
                    match app.git_wb.focus_files_pane() {
                        Ok(()) => {
                            app.message = app
                                .git_wb
                                .message
                                .clone()
                                .unwrap_or_else(|| "Git · Files".into());
                        }
                        Err(e) => app.message = e,
                    }
                }
                5 => {
                    // Context-aware Diff from the active docked column.
                    // If already on Diff, stay; else open from Changes/Files/Log.
                    if app.git_wb.tab == GitTab::Diff && app.git_wb.diff_path.is_some() {
                        app.message = "Git · Diff".into();
                    } else {
                        match app.git_wb.open_context_diff() {
                            Ok(()) => {
                                app.message = app
                                    .git_wb
                                    .diff_path
                                    .as_ref()
                                    .map(|p| format!("Diff · {p}"))
                                    .unwrap_or_else(|| "Git · Diff".into());
                            }
                            Err(e) => {
                                // Re-show last diff if one exists
                                if app.git_wb.diff_path.is_some() {
                                    app.git_wb.tab = GitTab::Diff;
                                    app.git_wb.focus = GitFocus::Diff;
                                    app.message = "Git · Diff".into();
                                } else {
                                    app.message = e;
                                }
                            }
                        }
                    }
                }
                6 => {
                    app.git_wb.tab = GitTab::PullRequests;
                    app.git_wb.focus = GitFocus::List;
                    app.git_wb.ensure_tab_data();
                    app.message = "Git · PRs".into();
                }
                7 => {
                    app.git_wb.tab = GitTab::Issues;
                    app.git_wb.focus = GitFocus::List;
                    app.git_wb.ensure_tab_data();
                    app.message = "Git · Issues".into();
                }
                8 => {
                    app.git_wb.tab = GitTab::Auth;
                    app.git_wb.focus = GitFocus::List;
                    app.git_wb.ensure_tab_data();
                    app.message = "Git · Auth".into();
                }
                9 => {
                    app.git_wb.tab = GitTab::Stash;
                    app.git_wb.focus = GitFocus::List;
                    app.git_wb.ensure_tab_data();
                    app.message = "Git · Stash".into();
                }
                _ => {}
            }
        }
        // ── PR / Issue filter typing ─────────────────────
        KeyCode::Char(c)
            if app.git_wb.pr_filter_mode && app.git_wb.tab == GitTab::PullRequests =>
        {
            if !c.is_control() {
                app.git_wb.pr_filter.push(c);
                app.git_wb.refilter_prs();
            }
            return;
        }
        KeyCode::Backspace
            if app.git_wb.pr_filter_mode && app.git_wb.tab == GitTab::PullRequests =>
        {
            app.git_wb.pr_filter.pop();
            app.git_wb.refilter_prs();
            return;
        }
        KeyCode::Char(c)
            if app.git_wb.issue_filter_mode && app.git_wb.tab == GitTab::Issues =>
        {
            if !c.is_control() {
                app.git_wb.issue_filter.push(c);
                app.git_wb.refilter_issues();
            }
            return;
        }
        KeyCode::Backspace
            if app.git_wb.issue_filter_mode && app.git_wb.tab == GitTab::Issues =>
        {
            app.git_wb.issue_filter.pop();
            app.git_wb.refilter_issues();
            return;
        }
        KeyCode::Down | KeyCode::Char('j') => app.git_wb.move_sel(1),
        KeyCode::Up | KeyCode::Char('k') => app.git_wb.move_sel(-1),
        KeyCode::PageDown => app.git_wb.move_sel(10),
        KeyCode::PageUp => app.git_wb.move_sel(-10),
        // JetBrains dock: 'i' edit commit message, 'c' commit
        KeyCode::Char('i')
            if matches!(
                app.git_wb.tab,
                GitTab::Status | GitTab::History | GitTab::Commit
            ) =>
        {
            app.git_wb.commit_editing = true;
            app.git_wb.pane = xei_core::git_workbench::GitPane::Changes;
            app.message = "Commit message — type, Enter commit, Esc done".into();
        }
        KeyCode::Char('c')
            if !matches!(app.git_wb.tab, GitTab::Branches | GitTab::PullRequests) =>
        {
            match app.git_wb.commit_with_buf() {
                Ok(()) => {
                    app.message = app
                        .git_wb
                        .message
                        .clone()
                        .unwrap_or_else(|| "Committed".into());
                    app.refresh_git();
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('v')
            if matches!(
                app.git_wb.tab,
                GitTab::History | GitTab::Status | GitTab::Commit
            ) =>
        {
            app.git_wb.tab = GitTab::History;
            app.git_wb.toggle_history_view();
            app.message = app
                .git_wb
                .message
                .clone()
                .unwrap_or_else(|| "Toggled history view".into());
        }
        // PR state: [ ] cycle  (also works when empty)
        KeyCode::Char(']') if app.git_wb.tab == GitTab::PullRequests => {
            app.git_wb.cycle_pr_state(true);
            app.message = app.git_wb.message.clone().unwrap_or_default();
        }
        KeyCode::Char('[') if app.git_wb.tab == GitTab::PullRequests => {
            app.git_wb.cycle_pr_state(false);
            app.message = app.git_wb.message.clone().unwrap_or_default();
        }
        KeyCode::Char('s') if app.git_wb.tab == GitTab::Issues => {
            app.git_wb.cycle_issue_state();
            app.message = app.git_wb.message.clone().unwrap_or_default();
        }
        KeyCode::Char('/') if app.git_wb.tab == GitTab::PullRequests => {
            app.git_wb.begin_pr_filter();
            app.message = app.git_wb.message.clone().unwrap_or_default();
        }
        KeyCode::Char('/') if app.git_wb.tab == GitTab::Issues => {
            app.git_wb.begin_issue_filter();
            app.message = app.git_wb.message.clone().unwrap_or_default();
        }
        KeyCode::Enter => match app.git_wb.tab {
            // Docked 3-col: Enter follows the focused column
            GitTab::Status | GitTab::History | GitTab::Commit => {
                use xei_core::git_workbench::GitPane;
                match app.git_wb.pane {
                    GitPane::Changes => {
                        if let Err(e) = app.git_wb.open_selected_diff() {
                            app.message = e;
                        }
                    }
                    GitPane::Log => {
                        // Load detail + move focus to Files (stay docked)
                        match app.git_wb.focus_files_pane() {
                            Ok(()) => {
                                app.message = app
                                    .git_wb
                                    .message
                                    .clone()
                                    .unwrap_or_else(|| "Files".into());
                            }
                            Err(e) => app.message = e,
                        }
                    }
                    GitPane::Files => {
                        if let Err(e) = app.git_wb.open_selected_commit_file_diff() {
                            app.message = e;
                        }
                    }
                }
            }
            GitTab::Branches => match app.git_wb.checkout_selected_branch() {
                Ok(()) => {
                    app.message = app
                        .git_wb
                        .message
                        .clone()
                        .unwrap_or_else(|| "Checked out".into());
                    app.refresh_git();
                }
                Err(e) => app.message = e,
            },
            GitTab::Diff => {}
            GitTab::PullRequests => {
                if app.git_wb.pr_filter_mode {
                    app.git_wb.pr_filter_mode = false;
                    app.message = format!("Filter: {} result(s)", app.git_wb.pr_filtered.len());
                } else {
                    // Enter → PR review surface (files + comments + diff)
                    app.open_pr_review_selected();
                }
            }
            GitTab::Issues => {
                if app.git_wb.issue_filter_mode {
                    app.git_wb.issue_filter_mode = false;
                    app.message =
                        format!("Filter: {} result(s)", app.git_wb.issue_filtered.len());
                } else if let Some(it) = app.git_wb.selected_issue() {
                    let n = it.number.to_string();
                    if let Some(ref root) = app.git_wb.root {
                        match xei_core::gh::browse(root, Some(&format!("issues/{n}")))
                            .or_else(|_| xei_core::gh::browse(root, Some(&n)))
                        {
                            Ok(m) => app.message = m,
                            Err(e) => app.message = e,
                        }
                    }
                }
            }
            GitTab::Auth => match app.git_wb.run_auth_action() {
                Ok(()) => {
                    app.message = app
                        .git_wb
                        .message
                        .clone()
                        .unwrap_or_else(|| "OK".into());
                }
                Err(e) => app.message = e,
            },
            GitTab::Stash => match app.git_wb.stash_apply_selected() {
                Ok(()) => {
                    app.message = app
                        .git_wb
                        .message
                        .clone()
                        .unwrap_or_else(|| "Stash applied".into());
                    app.refresh_git();
                }
                Err(e) => app.message = e,
            },
        },
        KeyCode::Char('d') if app.git_wb.tab == GitTab::Stash => {
            match app.git_wb.stash_drop_selected() {
                Ok(()) => {
                    app.message = app
                        .git_wb
                        .message
                        .clone()
                        .unwrap_or_else(|| "Stash dropped".into());
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('p') if app.git_wb.tab == GitTab::Stash => {
            match app.git_wb.stash_show_selected() {
                Ok(text) => {
                    app.xlc.open = true;
                    app.xlc.add_output("=== stash show ===");
                    for line in text.lines().take(80) {
                        app.xlc.add_output(line);
                    }
                    app.message = "Stash preview → XLC".into();
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('c') if app.git_wb.tab == GitTab::PullRequests => {
            match app.git_wb.checkout_selected_pr() {
                Ok(()) => {
                    app.message = app
                        .git_wb
                        .message
                        .clone()
                        .unwrap_or_else(|| "PR checked out".into());
                    app.refresh_git();
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('v') if app.git_wb.tab == GitTab::PullRequests => {
            app.open_pr_review_selected();
        }
        KeyCode::Char('M') if app.git_wb.tab == GitTab::PullRequests => {
            match app.git_wb.merge_selected_pr("squash") {
                Ok(()) => {
                    app.message = app
                        .git_wb
                        .message
                        .clone()
                        .unwrap_or_else(|| "Merged".into())
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char(' ') | KeyCode::Char('s') if app.git_wb.tab == GitTab::Status => {
            match app.git_wb.stage_selected() {
                Ok(()) => {
                    app.message = app
                        .git_wb
                        .message
                        .clone()
                        .unwrap_or_else(|| "Staged".into());
                    app.refresh_git();
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('a') if app.git_wb.tab == GitTab::Status => {
            match app.git_wb.stage_all() {
                Ok(()) => {
                    app.message = app.git_wb.message.clone().unwrap_or_default();
                    app.refresh_git();
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('A') if app.git_wb.tab == GitTab::Status => {
            match app.git_wb.unstage_all() {
                Ok(()) => {
                    app.message = app.git_wb.message.clone().unwrap_or_default();
                    app.refresh_git();
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('x') if app.git_wb.tab == GitTab::Status => {
            if let Err(e) = app.git_wb.begin_discard_selected() {
                app.message = e;
            } else {
                app.message = app.git_wb.message.clone().unwrap_or_default();
            }
        }
        KeyCode::Char('c') if app.git_wb.tab == GitTab::Branches => {
            app.git_wb.begin_new_branch();
            app.message = app.git_wb.message.clone().unwrap_or_default();
        }
        KeyCode::Char('d') if app.git_wb.tab == GitTab::Branches => {
            match app.git_wb.delete_selected_branch() {
                Ok(()) => app.message = app.git_wb.message.clone().unwrap_or_default(),
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('C') if matches!(app.git_wb.tab, GitTab::History | GitTab::Commit) => {
            match app.git_wb.cherry_pick_selected() {
                Ok(()) => {
                    app.message = app.git_wb.message.clone().unwrap_or_default();
                    app.refresh_git();
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('V') if matches!(app.git_wb.tab, GitTab::History | GitTab::Commit) => {
            match app.git_wb.revert_selected() {
                Ok(()) => {
                    app.message = app.git_wb.message.clone().unwrap_or_default();
                    app.refresh_git();
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('y') if matches!(app.git_wb.tab, GitTab::History | GitTab::Commit) => {
            if let Some(h) = app.git_wb.copy_commit_hash() {
                let _ = xei_core::clipboard::copy(&h);
                app.message = format!("Copied {}", &h[..7.min(h.len())]);
            }
        }
        KeyCode::Char('P') => match app.git_wb.create_pr_from_head() {
            Ok(()) => {
                app.message = app
                    .git_wb
                    .message
                    .clone()
                    .unwrap_or_else(|| "PR created".into())
            }
            Err(e) => app.message = e,
        },
        KeyCode::Char('f') if !matches!(app.git_wb.tab, GitTab::Auth) => {
            // Background — toolbar spinner plays; result lands via poll_loading.
            app.message = app.git_wb.remote_action(xei_core::git_workbench::RemoteAction::Fetch);
        }
        KeyCode::Char('p') if !matches!(app.git_wb.tab, GitTab::Auth | GitTab::PullRequests) => {
            app.message = app.git_wb.remote_action(xei_core::git_workbench::RemoteAction::Pull);
        }
        KeyCode::Char('R') if !matches!(app.git_wb.tab, GitTab::Auth) => {
            app.message =
                app.git_wb.remote_action(xei_core::git_workbench::RemoteAction::PullRebase);
        }
        KeyCode::Char('u') => {
            app.message = app.git_wb.remote_action(xei_core::git_workbench::RemoteAction::Push);
        }
        KeyCode::Char('r') => {
            let hint = app.filename.as_deref();
            if app.git_wb.tab == GitTab::Auth {
                // Async refresh — loading spinner (no UI freeze)
                app.git_wb.refresh_auth();
                app.message = app
                    .git_wb
                    .message
                    .clone()
                    .unwrap_or_else(|| "Refreshing GitHub account…".into());
            } else {
                app.git_wb.refresh(hint);
                if app.git_wb.tab == GitTab::PullRequests {
                    app.git_wb.reload_prs();
                }
                app.message = "Git refreshed".into();
            }
        }
        KeyCode::Char('m') if app.git_wb.tab == GitTab::History => {
            match app.git_wb.load_more_history() {
                Ok(n) => app.message = format!("Loaded +{n} commits"),
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('z') => match app.git_wb.stash() {
            Ok(()) => {
                app.message = app.git_wb.message.clone().unwrap_or_else(|| "Stashed".into());
                app.refresh_git();
            }
            Err(e) => app.message = e,
        },
        KeyCode::Char('Z') => match app.git_wb.stash_pop() {
            Ok(()) => {
                app.message = app
                    .git_wb
                    .message
                    .clone()
                    .unwrap_or_else(|| "Stash popped".into());
                app.refresh_git();
            }
            Err(e) => app.message = e,
        },
        KeyCode::Char('o') => {
            let r: Result<String, String> =
                if matches!(app.git_wb.tab, GitTab::History | GitTab::Commit) {
                    app.git_wb.browse_commit().map(|()| {
                        app.git_wb
                            .message
                            .clone()
                            .unwrap_or_else(|| "Opened commit".into())
                    })
                } else if app.git_wb.tab == GitTab::PullRequests {
                    if let Some(pr) = app.git_wb.selected_pr() {
                        let n = pr.number.to_string();
                        if let Some(ref root) = app.git_wb.root {
                            xei_core::gh::browse(root, Some(&n))
                        } else {
                            Err("No git root".into())
                        }
                    } else {
                        app.git_wb.browse_repo().map(|()| {
                            app.git_wb
                                .message
                                .clone()
                                .unwrap_or_else(|| "Browser".into())
                        })
                    }
                } else if app.git_wb.tab == GitTab::Issues {
                    if let Some(it) = app.git_wb.selected_issue() {
                        let n = it.number.to_string();
                        if let Some(ref root) = app.git_wb.root {
                            xei_core::gh::browse(root, Some(&format!("issues/{n}")))
                                .or_else(|_| xei_core::gh::browse(root, Some(&n)))
                        } else {
                            Err("No git root".into())
                        }
                    } else {
                        app.git_wb.browse_repo().map(|()| {
                            app.git_wb
                                .message
                                .clone()
                                .unwrap_or_else(|| "Browser".into())
                        })
                    }
                } else {
                    app.git_wb.browse_repo().map(|()| {
                        app.git_wb
                            .message
                            .clone()
                            .unwrap_or_else(|| "Browser".into())
                    })
                };
            match r {
                Ok(msg) => app.message = msg,
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('g') | KeyCode::Char('c')
            if !matches!(
                app.git_wb.tab,
                GitTab::Commit | GitTab::Diff | GitTab::Auth
            ) =>
        {
            app.git_wb.from_scm = true;
            app.close_git_workbench();
        }
        _ => {}
    }
}

/// Pretty document preview (Ctrl+Shift+V / :preview / :pr)
fn handle_preview(app: &mut App, code: KeyCode) {
    // While the reverse transform plays, only Esc force-dismisses.
    if app.preview.closing {
        if matches!(code, KeyCode::Esc | KeyCode::Char('q')) {
            app.close_preview_immediate();
            app.message = String::new();
        }
        return;
    }

    // Image: arrow keys resize
    if matches!(app.preview.kind, Some(xei_core::PreviewKind::Image)) {
        let cell_px = app.cell_px_or_default();
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.close_preview();
                app.message = String::new();
                return;
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('-') => {
                if let Some(img) = app.preview_image.as_mut() {
                    img.adjust_width(-4, cell_px);
                    app.message = format!("Image width {} cells", img.width_cells);
                } else if !app.wrap_lines {
                    // Text preview panning (wrap_lines = false)
                    app.preview.hscroll = app.preview.hscroll.saturating_sub(6);
                }
                return;
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('+') | KeyCode::Char('=') => {
                if let Some(img) = app.preview_image.as_mut() {
                    img.adjust_width(4, cell_px);
                    app.message = format!("Image width {} cells", img.width_cells);
                } else if !app.wrap_lines {
                    app.preview.hscroll = app.preview.hscroll.saturating_add(6);
                }
                return;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.preview.scroll_by(1, 1);
                return;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.preview.scroll_by(-1, 1);
                return;
            }
            _ => {}
        }
    }

    // Audio: Space toggles playback
    if matches!(app.preview.kind, Some(xei_core::PreviewKind::Audio)) {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.close_preview();
                app.message = String::new();
                return;
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(player) = app.preview_audio.as_mut() {
                    match player.toggle() {
                        Ok(msg) => {
                            let playing = player.playing();
                            if let Some(ref path) = app.preview.media_path.clone() {
                                app.preview.lines =
                                    xei_core::media::audio_info_lines(path, playing);
                            }
                            app.message = msg;
                        }
                        Err(e) => app.message = e,
                    }
                }
                return;
            }
            _ => {}
        }
    }

    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.close_preview();
            app.message = String::new();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.preview.scroll_by(1, 1);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.preview.scroll_by(-1, 1);
        }
        KeyCode::PageDown | KeyCode::Char('f') => {
            app.preview.scroll_by(12, 12);
        }
        KeyCode::PageUp | KeyCode::Char('b') => {
            app.preview.scroll_by(-12, 12);
        }
        KeyCode::Home | KeyCode::Char('g') => {
            app.preview.scroll = 0;
        }
        KeyCode::End | KeyCode::Char('G') => {
            app.preview.scroll = app.preview.lines.len().saturating_sub(1);
        }
        KeyCode::Char('r') => {
            app.refresh_preview_if_open();
            app.message = String::from("Preview refreshed");
        }
        _ => {}
    }
}

/// VS Code Source Control panel (Ctrl+G)
fn handle_scm(app: &mut App, code: KeyCode) {
    use xei_core::scm::ScmFocus;

    // While sliding out, only Esc force-dismisses.
    if app.scm.closing {
        if code == KeyCode::Esc {
            app.close_scm_immediate();
            app.message = String::new();
        }
        return;
    }
    match code {
        KeyCode::Esc => {
            app.close_scm();
            app.message = String::new();
        }
        // Full Git workbench from light SCM
        KeyCode::Char('G') => {
            app.open_git_workbench();
        }
        KeyCode::Tab => {
            app.scm.cycle_focus(true);
        }
        KeyCode::BackTab => {
            app.scm.cycle_focus(false);
        }
        KeyCode::Enter => match app.scm.focus {
            ScmFocus::Message | ScmFocus::CommitButton => app.scm_commit(),
            ScmFocus::Changes => app.scm_open_selected_file(),
            ScmFocus::Graph => {
                // Flash selected commit detail in status
                if let Some(row) = app.scm.selected_graph_row() {
                    app.message = format!(
                        "{}  {} — {} ({})",
                        row.short, row.subject, row.author, row.when
                    );
                }
            }
        },
        KeyCode::Down | KeyCode::Char('j') if app.scm.focus != ScmFocus::Message => {
            if app.scm.focus == ScmFocus::CommitButton {
                app.scm.focus = ScmFocus::Changes;
            } else if app.scm.focus == ScmFocus::Graph {
                app.scm.move_graph_sel(1);
            } else {
                app.scm.focus = ScmFocus::Changes;
                app.scm.move_sel(1);
            }
        }
        KeyCode::Up | KeyCode::Char('k') if app.scm.focus != ScmFocus::Message => {
            if app.scm.focus == ScmFocus::Changes {
                if app.scm.selected == 0 {
                    app.scm.focus = ScmFocus::CommitButton;
                } else {
                    app.scm.move_sel(-1);
                }
            } else if app.scm.focus == ScmFocus::CommitButton {
                app.scm.focus = ScmFocus::Message;
            } else if app.scm.focus == ScmFocus::Graph {
                if app.scm.graph_selected == 0 {
                    app.scm.focus = ScmFocus::Changes;
                } else {
                    app.scm.move_graph_sel(-1);
                }
            }
        }
        KeyCode::Down if app.scm.focus == ScmFocus::Message => {
            app.scm.focus = ScmFocus::CommitButton;
        }
        KeyCode::Char(' ') | KeyCode::Char('s') if app.scm.focus != ScmFocus::Message => {
            app.scm.focus = ScmFocus::Changes;
            app.scm_stage_selected();
        }
        KeyCode::Char('a') if app.scm.focus != ScmFocus::Message => {
            app.scm_stage_all();
        }
        KeyCode::Char('u') if app.scm.focus != ScmFocus::Message => {
            if let Err(e) = app.scm.unstage_all() {
                app.message = e;
            } else {
                app.message = "Unstaged all".into();
                app.refresh_git();
            }
        }
        KeyCode::Char('r') if app.scm.focus != ScmFocus::Message => {
            app.scm_refresh();
            app.message = "SCM refreshed".into();
        }
        KeyCode::Char('m') | KeyCode::Char('L') if app.scm.focus == ScmFocus::Graph => {
            match app.scm.load_more_graph() {
                Ok(n) => {
                    app.message = format!(
                        "Loaded +{} commits (showing {}, limit {})",
                        n,
                        app.scm.graph.len(),
                        app.scm.graph_limit
                    );
                }
                Err(e) => app.message = e,
            }
        }
        KeyCode::Char('c') if app.scm.focus != ScmFocus::Message => {
            app.scm.focus = ScmFocus::Message;
        }
        KeyCode::Char('x') | KeyCode::Delete if app.scm.focus == ScmFocus::Changes => {
            if let Err(e) = app.scm.discard_selected() {
                app.message = e;
            } else {
                app.message = app
                    .scm
                    .last_result
                    .clone()
                    .unwrap_or_else(|| "Discarded".into());
                app.refresh_git();
            }
        }
        KeyCode::Backspace if app.scm.focus == ScmFocus::Message => {
            app.scm.message.pop();
        }
        KeyCode::Char(ch) if app.scm.focus == ScmFocus::Message && !ch.is_control() => {
            app.scm.message.push(ch);
        }
        KeyCode::Char('g') if app.scm.focus != ScmFocus::Message => {
            app.scm.focus = ScmFocus::Graph;
        }
        _ => {}
    }
}

fn handle_normal(app: &mut App, code: KeyCode) {
    // Esc with multi-cursors: clear them first
    if matches!(code, KeyCode::Esc) && app.multi.is_active() {
        app.clear_multi_cursors();
        return;
    }

    // ── Space leader (which-key) ─────────────────────────
    if app.which_key.is_leader() {
        handle_leader(app, code);
        return;
    }

    // ── Register / mark pending ─────────────────────────
    if app.pending_register {
        if let KeyCode::Char(c) = code {
            if app.registers.select(c) {
                app.message = format!("Register {}", app.registers.active_label());
                app.begin_chord(
                    &format!("\"{}", app.registers.active_label()),
                    vec![
                        ("y", "yank into reg"),
                        ("d", "delete into reg"),
                        ("p", "put from reg"),
                    ],
                );
            } else {
                app.message = String::from("Invalid register");
                app.clear_which_key();
            }
        }
        app.pending_register = false;
        return;
    }
    if app.pending_mark_set {
        if let KeyCode::Char(c) = code {
            app.set_mark(c);
            app.clear_which_key();
        } else {
            app.pending_mark_set = false;
            app.clear_which_key();
            app.message = String::from("Mark cancelled");
        }
        return;
    }
    if let Some(linewise) = app.pending_mark_jump.take() {
        if let KeyCode::Char(c) = code {
            app.jump_to_mark(c, linewise);
            app.clear_which_key();
        } else {
            app.clear_which_key();
            app.message = String::from("Mark jump cancelled");
        }
        return;
    }

    // ── Operator-pending resolution ─────────────────────
    if let Some(op) = app.pending_operator {
        handle_operator_pending(app, op, code);
        return;
    }

    if let Some(pending) = app.pending_key.take() {
        handle_pending(app, pending, code);
        return;
    }

    if let KeyCode::Char(c) = code {
        // f/F/t/T/r: pending char target
        if let Some(ft) = app.pending_ft.take() {
            match ft {
                'f' => {
                    app.buffer.find_char_forward(c);
                    app.record_find(FindKind::Find, true, c);
                }
                'F' => {
                    app.buffer.find_char_backward(c);
                    app.record_find(FindKind::Find, false, c);
                }
                't' => {
                    app.buffer.till_char_forward(c);
                    app.record_find(FindKind::Till, true, c);
                }
                'T' => {
                    app.buffer.till_char_backward(c);
                    app.record_find(FindKind::Till, false, c);
                }
                'r' => {
                    app.push_undo();
                    app.buffer.replace_char(c);
                    app.last_change = Some(xei_core::LastChange::ReplaceChar { ch: c });
                }
                _ => {}
            }
            app.update_scroll();
            return;
        }
        // Count accumulation
        if c.is_ascii_digit() {
            if c == '0' && app.count.is_none() {
                // bare 0
            } else {
                let d = c.to_digit(10).unwrap() as usize;
                app.count = Some(app.count.unwrap_or(0) * 10 + d);
                return;
            }
        }
        let has_count = app.count.is_some();
        let n = app.count.take().unwrap_or(1);
        match c {
            'f' | 'F' | 'T' | 'r' => {
                app.pending_ft = Some(c);
                return;
            }
            't' => {
                app.pending_ft = Some('t');
                return;
            }
            'd' => {
                app.begin_operator(Operator::Delete);
                return;
            }
            'c' => {
                app.begin_operator(Operator::Change);
                return;
            }
            'y' => {
                app.begin_operator(Operator::Yank);
                return;
            }
            'C' => {
                app.apply_operator_motion(Operator::Change, Motion::LineEnd, 1);
                return;
            }
            'D' => {
                app.apply_operator_motion(Operator::Delete, Motion::LineEnd, 1);
                return;
            }
            '.' => {
                app.repeat_last_change();
                return;
            }
            'h' | 'j' | 'k' | 'l' | 'w' | 'b' | 'e' => {
                for _ in 0..n {
                    match c {
                        'h' => app.buffer.move_left(),
                        'j' => app.buffer.move_down(),
                        'k' => app.buffer.move_up(),
                        'l' => app.buffer.move_right(),
                        'w' => app.buffer.move_word_forward(),
                        'b' => app.buffer.move_word_back(),
                        'e' => {
                            // reuse motion helper via range end
                            let r = xei_core::ops::range_for_motion(
                                &app.buffer,
                                Motion::WordEnd,
                                1,
                            );
                            app.buffer.cursor = Position::new(
                                r.end.row,
                                r.end.col.saturating_sub(1),
                            );
                        }
                        _ => {}
                    }
                }
                app.update_scroll();
                return;
            }
            'x' => {
                app.push_undo();
                for _ in 0..n {
                    if app.buffer.cursor.col < app.buffer.current_line_len() {
                        app.buffer.delete_char_at_cursor();
                    }
                }
                app.last_change = Some(xei_core::LastChange::DeleteChar { count: n });
                return;
            }
            'G' => {
                if has_count {
                    app.goto_line(n);
                } else {
                    app.push_jump();
                    app.buffer.cursor.row = app.buffer.line_count().saturating_sub(1);
                    app.buffer.move_to_line_start();
                    app.update_scroll();
                }
                return;
            }
            'g' => {
                app.pending_key = Some('g');
                app.begin_chord(
                    "g",
                    xei_core::which_key::as_hints(xei_core::which_key::map_g()),
                );
                app.message = String::from("-- g --");
                return;
            }
            'z' => {
                app.pending_key = Some('z');
                app.begin_chord(
                    "z",
                    xei_core::which_key::as_hints(xei_core::which_key::map_z()),
                );
                app.message = String::from("-- z --");
                return;
            }
            ']' => {
                app.pending_key = Some(']');
                app.begin_chord(
                    "]",
                    xei_core::which_key::as_hints(xei_core::which_key::map_bracket_close()),
                );
                app.message = String::from("-- ] --");
                return;
            }
            '[' => {
                app.pending_key = Some('[');
                app.begin_chord(
                    "[",
                    xei_core::which_key::as_hints(xei_core::which_key::map_bracket_open()),
                );
                app.message = String::from("-- [ --");
                return;
            }
            ' ' => {
                // Space — leader key (which-key full map)
                app.begin_leader();
                return;
            }
            'K' => {
                app.request_hover();
                return;
            }
            'q' => {
                if app.macros.is_recording() {
                    app.macros.stop();
                    app.message = String::from("Macro recorded");
                } else {
                    app.pending_key = Some('q');
                    app.begin_chord(
                        "q",
                        xei_core::which_key::as_hints(xei_core::which_key::map_macro_record()),
                    );
                    app.message = String::from("-- record macro --");
                }
                return;
            }
            '@' => {
                app.pending_key = Some('@');
                app.begin_chord(
                    "@",
                    xei_core::which_key::as_hints(xei_core::which_key::map_macro_play()),
                );
                app.message = String::from("-- play macro --");
                return;
            }
            'm' => {
                app.pending_mark_set = true;
                app.begin_chord(
                    "m",
                    xei_core::which_key::as_hints(xei_core::which_key::map_mark_set()),
                );
                app.message = String::from("-- mark --");
                return;
            }
            '"' => {
                app.pending_register = true;
                app.begin_chord(
                    "\"",
                    xei_core::which_key::as_hints(xei_core::which_key::map_register()),
                );
                app.message = String::from("-- register --");
                return;
            }
            ';' => {
                app.repeat_find(false);
                return;
            }
            ',' => {
                app.repeat_find(true);
                return;
            }
            '\'' => {
                app.pending_mark_jump = Some(true);
                app.begin_chord(
                    "'",
                    xei_core::which_key::as_hints(xei_core::which_key::map_mark_jump_line()),
                );
                app.message = String::from("-- jump ' --");
                return;
            }
            '`' => {
                app.pending_mark_jump = Some(false);
                app.begin_chord(
                    "`",
                    xei_core::which_key::as_hints(xei_core::which_key::map_mark_jump_exact()),
                );
                app.message = String::from("-- jump ` --");
                return;
            }
            _ => {}
        }
    }

    match code {
        KeyCode::Char(':') => app.enter_xlc(None),
        KeyCode::Char('/') => app.enter_search(),
        KeyCode::Char('?') => app.enter_search_backward(),
        KeyCode::Char('i') => app.enter_insert(),
        KeyCode::Char('a') => {
            app.buffer.move_right();
            app.enter_insert();
        }
        KeyCode::Char('A') => {
            app.buffer.move_to_line_end();
            app.enter_insert();
        }
        KeyCode::Char('I') => {
            app.buffer.move_to_first_non_blank();
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
        KeyCode::Char('P') => app.paste_before(),
        KeyCode::Char('w') => app.buffer.move_word_forward(),
        KeyCode::Char('b') => app.buffer.move_word_back(),
        KeyCode::Char('o') => {
            app.push_undo();
            app.buffer.move_to_line_end();
            app.buffer.insert_newline_with_indent(false);
            app.mode = Mode::Insert;
            app.visual_anchor = None;
            app.message = String::from("-- INSERT --");
        }
        KeyCode::Char('O') => {
            app.push_undo();
            let row = app.buffer.cursor.row;
            let indent = app.buffer.leading_indent(row);
            let indent_cols = indent.chars().count();
            app.buffer.insert_line_at(row, indent);
            app.buffer.cursor.col = indent_cols;
            app.mode = Mode::Insert;
            app.visual_anchor = None;
            app.message = String::from("-- INSERT --");
        }
        KeyCode::Char('p') => app.paste(),
        KeyCode::Char('u') => app.undo(),
        KeyCode::Char('n') => app.search_next(),
        KeyCode::Char('N') => app.search_prev(),
        KeyCode::Char('*') => app.search_word_under_cursor(),
        KeyCode::Char('#') => app.search_word_under_cursor_backward(),
        KeyCode::Char('G') => {
            app.push_jump();
            app.buffer.cursor.row = app.buffer.line_count().saturating_sub(1);
            app.buffer.move_to_line_start();
            app.update_scroll();
        }
        KeyCode::PageDown => {
            let step = (app.viewport.height as usize).saturating_sub(1).max(1);
            let max = app.buffer.line_count().saturating_sub(1);
            app.buffer.cursor.row = (app.buffer.cursor.row + step).min(max);
            app.buffer.clamp_col();
            app.update_scroll();
        }
        KeyCode::PageUp => {
            let step = (app.viewport.height as usize).saturating_sub(1).max(1);
            app.buffer.cursor.row = app.buffer.cursor.row.saturating_sub(step);
            app.buffer.clamp_col();
            app.update_scroll();
        }
        KeyCode::Home => app.buffer.move_to_line_start(),
        KeyCode::End => app.buffer.move_to_line_end(),
        KeyCode::Tab => {
            // Tab without ctrl = jump forward (vim Ctrl+I often equals Tab)
            app.jump_forward();
        }
        KeyCode::Esc => {
            app.pending_key = None;
            app.pending_ft = None;
            app.count = None;
            app.pending_register = false;
            app.pending_mark_set = false;
            app.pending_mark_jump = None;
            app.registers.clear_active();
            app.clear_operator_pending();
            app.clear_which_key();
            app.hover_text = None;
            app.message = String::new();
        }
        _ => {}
    }
}

/// Space-leader dispatch (which-key nested maps).
fn handle_leader(app: &mut App, code: KeyCode) {
    let path = app.which_key.leader.clone().unwrap_or_default();
    match code {
        KeyCode::Esc => {
            app.clear_which_key();
            app.message = String::new();
            return;
        }
        KeyCode::Backspace if !path.is_empty() => {
            // Back to root
            app.begin_leader();
            return;
        }
        KeyCode::Char(c) => {
            if path.is_empty() {
                // Root menu
                match c {
                    'f' => {
                        app.leader_enter_sub('f', "f");
                        return;
                    }
                    'b' => {
                        app.leader_enter_sub('b', "b");
                        return;
                    }
                    'g' => {
                        app.leader_enter_sub('g', "g");
                        return;
                    }
                    'l' => {
                        app.leader_enter_sub('l', "l");
                        return;
                    }
                    'w' => {
                        app.leader_enter_sub('w', "w");
                        return;
                    }
                    's' => {
                        app.leader_enter_sub('s', "s");
                        return;
                    }
                    'c' => {
                        app.leader_enter_sub('c', "c");
                        return;
                    }
                    'd' => {
                        app.leader_enter_sub('d', "d");
                        return;
                    }
                    't' => {
                        app.leader_enter_sub('t', "t");
                        return;
                    }
                    'h' => {
                        app.leader_enter_sub('h', "h");
                        return;
                    }
                    'p' => {
                        app.clear_which_key();
                        app.open_command_palette();
                        return;
                    }
                    '/' => {
                        app.clear_which_key();
                        app.open_workspace_search();
                        return;
                    }
                    ',' => {
                        app.clear_which_key();
                        app.open_settings();
                        return;
                    }
                    'e' => {
                        app.clear_which_key();
                        if app.explorer.open {
                            app.explorer.close();
                            app.mode = Mode::Normal;
                        } else {
                            app.explorer.toggle_at(app.filename.as_ref());
                            app.mode = Mode::Explorer;
                        }
                        return;
                    }
                    ';' => {
                        app.clear_which_key();
                        app.enter_xlc(None);
                        return;
                    }
                    _ => {
                        app.clear_which_key();
                        app.message = format!("SPC {c} — unknown");
                        return;
                    }
                }
            }
            // Submenus
            app.clear_which_key();
            match (path.as_str(), c) {
                // files
                ("f", 'f') => app.open_file_palette(),
                ("f", 'e') => {
                    if app.explorer.open {
                        app.explorer.close();
                        app.mode = Mode::Normal;
                    } else {
                        app.explorer.toggle_at(app.filename.as_ref());
                        app.mode = Mode::Explorer;
                    }
                }
                ("f", 's') => app.save_file(),
                ("f", 'S') => app.enter_xlc(Some("w ")),
                ("f", 'p') => app.toggle_preview(),
                ("f", 'r') => app.reload_from_disk(),
                // buffers
                ("b", 'n') => app.next_tab(),
                ("b", 'p') => app.prev_tab(),
                ("b", 'd') => app.close_current_tab(),
                ("b", 'b') => app.open_file_palette(),
                ("b", d) if d.is_ascii_digit() => {
                    let n = d.to_digit(10).unwrap_or(0) as usize;
                    if n >= 1 {
                        app.goto_tab(n - 1);
                    }
                }
                // git
                ("g", 'g') => app.open_git_workbench(),
                ("g", 's') => app.toggle_scm(),
                ("g", 'b') => app.toggle_blame(),
                ("g", 'f') => app.git_remote("fetch"),
                ("g", 'p') => app.git_remote("pull"),
                ("g", 'P') => app.git_remote("push"),
                ("g", 'r') => app.open_rebase(8),
                ("g", 'v') => app.open_pr_review_selected(),
                // lsp
                ("l", 'd') => {
                    if let Some(path) = app.filename.as_ref().map(|p| p.display().to_string()) {
                        app.push_jump();
                        app.sync_lsp_document();
                        let cursor = app.buffer.cursor();
                        app.lsp.request_definition(&path, cursor.row, cursor.col);
                        app.message = String::from("Requested go-to-definition");
                    }
                }
                ("l", 'r') => app.request_references(),
                ("l", 'h') => app.request_hover(),
                ("l", 'a') => app.request_code_actions(),
                ("l", 'f') => app.format_document(),
                ("l", 'o') => app.open_document_symbols(),
                ("l", 'R') => app.prompt_rename(),
                ("l", 'n') => app.diag_next(),
                ("l", 'p') => app.diag_prev(),
                ("l", 'c') => app.open_call_hierarchy(false),
                // window
                ("w", 'v') => app.split_vertical(),
                ("w", 's') => app.split_horizontal(),
                ("w", 'w') => app.focus_other_pane(),
                ("w", 'q') => app.close_split(),
                ("w", '=') => {
                    app.split.equalize();
                    app.message = String::from("Split equalized");
                }
                ("w", 't') => app.toggle_terminal_full(),
                // search
                ("s", 's') => app.enter_search(),
                ("s", 'S') => app.enter_search_backward(),
                ("s", 'f') => app.open_workspace_search(),
                ("s", 'o') => app.open_document_symbols(),
                ("s", 'w') => app.open_workspace_symbols(),
                // code
                ("c", 'a') => app.request_code_actions(),
                ("c", 'f') => app.format_document(),
                ("c", 'r') => app.prompt_rename(),
                ("c", 'd') => {
                    if let Some(path) = app.filename.as_ref().map(|p| p.display().to_string()) {
                        app.push_jump();
                        app.sync_lsp_document();
                        let cursor = app.buffer.cursor();
                        app.lsp.request_definition(&path, cursor.row, cursor.col);
                    }
                }
                ("c", 'R') => app.request_references(),
                // debug
                ("d", 'd') => app.toggle_debug_panel(),
                ("d", 's') => app.dap_start_or_continue(),
                ("d", 'b') => app.dap_toggle_breakpoint(),
                ("d", 'n') => app.dap_step_over(),
                ("d", 'i') => app.dap_step_into(),
                ("d", 'o') => app.dap_step_out(),
                ("d", 'p') => app.dap_pause(),
                ("d", 'x') => app.dap_stop(),
                ("d", 'c') => app.dap_list_configs(),
                ("d", 'a') => {
                    app.message =
                        "Attach: :DapAttach pid <n> | :DapAttach port <n> [python|node]".into();
                }
                ("d", 'r') => {
                    if let Err(e) = app.dap.restart() {
                        app.message = e;
                    } else {
                        app.message = "▶ restart".into();
                    }
                }
                // toggle
                ("t", 'b') => app.toggle_blame(),
                ("t", 'e') => {
                    if app.explorer.open {
                        app.explorer.close();
                        app.mode = Mode::Normal;
                    } else {
                        app.explorer.toggle_at(app.filename.as_ref());
                        app.mode = Mode::Explorer;
                    }
                }
                ("t", 't') => app.toggle_terminal_side(),
                ("t", 'T') => app.toggle_terminal_full(),
                ("t", 'i') => app.toggle_inlay_hints(),
                ("t", 'l') => app.toggle_code_lens(),
                ("t", 'r') => app.toggle_relative_number(),
                ("t", 'p') => app.toggle_preview(),
                // help
                ("h", 'h') | ("h", ',') => app.open_settings(),
                ("h", 'k') => {
                    app.key_hints = !app.key_hints;
                    app.message = if app.key_hints {
                        "key_hints on".into()
                    } else {
                        "key_hints off".into()
                    };
                }
                ("h", 's') => app.toggle_screensaver(),
                _ => {
                    app.message = format!("SPC {path}{c} — unknown");
                }
            }
        }
        _ => {
            app.clear_which_key();
        }
    }
}

fn handle_operator_pending(app: &mut App, op: Operator, code: KeyCode) {
    // Allow count after operator: d2w, c3iw, ...
    if let KeyCode::Char(c) = code {
        if c.is_ascii_digit() {
            if c == '0' && app.count.is_none() {
                // d0 = delete to line start
            } else {
                let d = c.to_digit(10).unwrap() as usize;
                app.count = Some(app.count.unwrap_or(0) * 10 + d);
                return;
            }
        }
    }
    let n = app.count.take().unwrap_or(1);

    // df{x} / dt{x} etc.
    if let Some(ft) = app.pending_ft.take() {
        if let KeyCode::Char(ch) = code {
            let motion = match ft {
                'f' => Motion::FindForward(ch),
                'F' => Motion::FindBackward(ch),
                't' => Motion::TillForward(ch),
                'T' => Motion::TillBackward(ch),
                _ => {
                    app.clear_operator_pending();
                    return;
                }
            };
            app.apply_operator_motion(op, motion, n);
        } else {
            app.clear_operator_pending();
        }
        return;
    }

    // Text-object modifier pending (i / a)
    if let Some(mod_c) = app.pending_to_mod.take() {
        if let KeyCode::Char(obj_c) = code {
            if let Some(obj) = parse_textobject(mod_c, obj_c) {
                app.apply_operator_textobject(op, obj, n);
                return;
            }
        }
        app.clear_operator_pending();
        app.message = String::from("Unknown text object");
        return;
    }

    match code {
        KeyCode::Esc => {
            app.clear_operator_pending();
            app.message = String::new();
        }
        KeyCode::Char('i') | KeyCode::Char('a') => {
            if let KeyCode::Char(m) = code {
                app.pending_to_mod = Some(m);
                let prefix = match op {
                    Operator::Delete => 'd',
                    Operator::Change => 'c',
                    Operator::Yank => 'y',
                };
                app.begin_chord(
                    &format!("{prefix}{m}"),
                    xei_core::which_key::as_hints(xei_core::which_key::map_textobject()),
                );
                app.message = format!("-- {}{} --", prefix, m);
            }
        }
        KeyCode::Char('d') if op == Operator::Delete => {
            app.apply_operator_motion(op, Motion::WholeLine, n);
        }
        KeyCode::Char('c') if op == Operator::Change => {
            app.apply_operator_motion(op, Motion::WholeLine, n);
        }
        KeyCode::Char('y') if op == Operator::Yank => {
            app.apply_operator_motion(op, Motion::WholeLine, n);
        }
        KeyCode::Char('g') => {
            app.pending_key = Some('@');
            app.pending_hints = vec![("g", "to buffer start")];
            app.message = String::from("-- press g for buffer start --");
        }
        KeyCode::Char('G') => {
            app.apply_operator_motion(op, Motion::BufferEnd, 1);
        }
        KeyCode::Char(c) => {
            if let Some(motion) = motion_from_char(c) {
                app.apply_operator_motion(op, motion, n);
            } else if matches!(c, 'f' | 'F' | 't' | 'T') {
                app.pending_ft = Some(c);
                let prefix = match op {
                    Operator::Delete => 'd',
                    Operator::Change => 'c',
                    Operator::Yank => 'y',
                };
                app.message = format!("-- {}{}{{char}} --", prefix, c);
            } else {
                app.clear_operator_pending();
                app.message = String::from("Unknown motion");
            }
        }
        _ => {
            app.clear_operator_pending();
        }
    }
}

fn handle_pending(app: &mut App, pending: char, code: KeyCode) {
    app.pending_hints.clear();
    app.which_key.clear();
    // Operator + g + g
    if pending == '@' {
        if let Some(op) = app.pending_operator {
            if matches!(code, KeyCode::Char('g')) {
                app.apply_operator_motion(op, Motion::BufferStart, 1);
                return;
            }
        }
        app.clear_operator_pending();
        return;
    }
    // f/t after operator
    if let Some(op) = app.pending_operator {
        if let Some(ft) = app.pending_ft.take() {
            if let KeyCode::Char(ch) = code {
                let motion = match ft {
                    'f' => Motion::FindForward(ch),
                    'F' => Motion::FindBackward(ch),
                    't' => Motion::TillForward(ch),
                    'T' => Motion::TillBackward(ch),
                    _ => {
                        app.clear_operator_pending();
                        return;
                    }
                };
                app.apply_operator_motion(op, motion, 1);
                return;
            }
        }
    }

    match (pending, code) {
        ('g', KeyCode::Char('g')) => {
            app.push_jump();
            app.buffer.cursor.row = 0;
            app.buffer.cursor.col = 0;
            app.scroll = 0;
            app.message = String::new();
        }
        ('g', KeyCode::Char('d')) => {
            let path = app.filename.as_ref().map(|p| p.display().to_string());
            if let Some(path) = path {
                app.push_jump();
                app.sync_lsp_document();
                let cursor = app.buffer.cursor();
                app.lsp.request_definition(&path, cursor.row, cursor.col);
                app.message = String::from("Requested go-to-definition");
            }
        }
        ('g', KeyCode::Char('p')) => {
            app.request_peek_definition();
        }
        ('g', KeyCode::Char('O')) => {
            app.open_document_symbols();
        }
        ('g', KeyCode::Char('t')) => app.next_tab(),
        ('g', KeyCode::Char('T')) => app.prev_tab(),
        ('g', KeyCode::Char('r')) => {
            app.request_references();
        }
        ('g', KeyCode::Char('b')) => {
            app.toggle_blame();
        }
        ('g', KeyCode::Char('C')) => {
            app.open_call_hierarchy(false);
        }
        ('g', KeyCode::Char('I')) => {
            // incoming calls
            app.open_call_hierarchy(false);
        }
        ('g', KeyCode::Char('H')) => {
            // outgoing helpers (call hierarchy outgoing)
            app.open_call_hierarchy(true);
        }
        ('g', _) => {
            app.message = String::from("g: gg gd gp gO gr gb gC gI gH gt gT");
        }
        ('z', KeyCode::Char('a')) => app.fold_toggle(),
        ('z', KeyCode::Char('c')) => app.fold_close(),
        ('z', KeyCode::Char('o')) => app.fold_open(),
        ('z', KeyCode::Char('M')) => app.fold_close_all(),
        ('z', KeyCode::Char('R')) => app.fold_open_all(),
        ('z', KeyCode::Char('h')) if !app.wrap_lines => {
            app.hscroll = app.hscroll.saturating_sub(6);
        }
        ('z', KeyCode::Char('l')) if !app.wrap_lines => {
            app.hscroll = app.hscroll.saturating_add(6);
        }
        ('z', KeyCode::Char('H')) if !app.wrap_lines => {
            let half = (app.viewport.width as usize / 2).max(1);
            app.hscroll = app.hscroll.saturating_sub(half);
        }
        ('z', KeyCode::Char('L')) if !app.wrap_lines => {
            let half = (app.viewport.width as usize / 2).max(1);
            app.hscroll = app.hscroll.saturating_add(half);
        }
        ('z', _) => {
            app.message = if app.wrap_lines {
                String::from("z: za zc zo zM zR")
            } else {
                String::from("z: za zc zo zM zR · zh/zl/zH/zL pan")
            };
        }
        (']', KeyCode::Char('d')) => app.diag_next(),
        (']', KeyCode::Char('c')) => app.git_change_next(),
        (']', _) => {
            app.message = String::from("]d diag · ]c git change");
        }
        ('[', KeyCode::Char('d')) => app.diag_prev(),
        ('[', KeyCode::Char('c')) => app.git_change_prev(),
        ('[', _) => {
            app.message = String::from("[d diag · [c git change");
        }
        ('q', KeyCode::Char(c)) if c.is_ascii_lowercase() => {
            if app.macros.start(c) {
                app.message = format!("Recording @{0}… (q to stop)", c);
            }
        }
        ('q', _) => {
            app.message = String::from("q{a-z} to record");
        }
        ('@', KeyCode::Char('@')) => {
            if let Some(name) = app.macros.last_played {
                play_macro(app, name);
            } else {
                app.message = String::from("No previous macro");
            }
        }
        ('@', KeyCode::Char(c)) if c.is_ascii_lowercase() => {
            play_macro(app, c);
        }
        ('@', _) => {
            app.message = String::from("@{a-z} to play");
        }
        _ => {}
    }
}

fn handle_insert(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.completions.deactivate();
            if app.multi.is_active() {
                // First Esc clears multi-cursors; second leaves insert
                app.clear_multi_cursors();
                return;
            }
            // Vim-like: leave insert on the last typed char
            if app.buffer.cursor.col > 0 {
                app.buffer.move_left();
            }
            app.enter_normal();
            app.message = String::new();
        }
        KeyCode::Enter => {
            if app.completions.active {
                apply_completion(app);
            } else {
                app.multi_newline();
                app.update_scroll();
            }
        }
        KeyCode::Tab => {
            if app.completions.active && !app.completions.suggestions.is_empty() {
                apply_completion(app);
            } else if app.multi.is_active() {
                for _ in 0..4 {
                    app.multi_insert_char(' ');
                }
                app.update_scroll();
            } else {
                // Snippet expand (fn, for, if, …) then fall back to indent spaces
                let ext = app.file_extension();
                if let Some(msg) = xei_core::snippets::try_expand(&mut app.buffer, ext.as_deref()) {
                    app.modified = true;
                    app.rebuild_folds();
                    app.update_scroll();
                    app.message = msg;
                } else {
                    for _ in 0..4 {
                        app.buffer.insert_char(' ');
                    }
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
            app.multi_move_each(|b| {
                b.move_left();
            });
            app.update_scroll();
        }
        KeyCode::Right => {
            app.completions.deactivate();
            app.multi_move_each(|b| {
                b.move_right();
            });
            app.update_scroll();
        }
        KeyCode::Up => {
            if app.completions.active {
                app.completions.prev();
            } else {
                app.multi_move_each(|b| {
                    b.move_up();
                });
                app.update_scroll();
            }
        }
        KeyCode::Down => {
            if app.completions.active {
                app.completions.next();
            } else {
                app.multi_move_each(|b| {
                    b.move_down();
                });
                app.update_scroll();
            }
        }
        KeyCode::Backspace => {
            if app.multi.is_active() {
                app.multi_backspace();
            } else if is_pair_close_char(app.buffer.char_before_cursor())
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
            if app.multi.is_active() {
                app.multi_insert_char(')');
            } else if !app.buffer.skip_char_if_match(')') {
                app.buffer.insert_char(')');
            }
            app.completions.deactivate();
        }
        KeyCode::Char(']') => {
            if app.multi.is_active() {
                app.multi_insert_char(']');
            } else if !app.buffer.skip_char_if_match(']') {
                app.buffer.insert_char(']');
            }
            app.completions.deactivate();
        }
        KeyCode::Char('}') => {
            if app.multi.is_active() {
                app.multi_insert_char('}');
            } else if !app.buffer.skip_char_if_match('}') {
                app.buffer.insert_char('}');
            }
            app.completions.deactivate();
        }
        KeyCode::Char('\'') => {
            app.completions.deactivate();
            if app.multi.is_active() {
                app.multi_insert_char('\'');
            } else if app.buffer.skip_char_if_match('\'') {
            } else if should_auto_close_single_quote(app) {
                app.buffer.insert_char_pair('\'', '\'');
            } else {
                app.buffer.insert_char('\'');
            }
        }
        KeyCode::Char('"') => {
            app.completions.deactivate();
            if app.multi.is_active() {
                app.multi_insert_char('"');
            } else if app.buffer.skip_char_if_match('"') {
            } else if should_auto_close_double_quote(app) {
                app.buffer.insert_char_pair('"', '"');
            } else {
                app.buffer.insert_char('"');
            }
        }
        KeyCode::Char('(') => {
            if app.multi.is_active() {
                app.multi_insert_char('(');
            } else {
                app.buffer.insert_char_pair('(', ')');
            }
            app.completions.deactivate();
        }
        KeyCode::Char('[') => {
            if app.multi.is_active() {
                app.multi_insert_char('[');
            } else {
                app.buffer.insert_char_pair('[', ']');
            }
            app.completions.deactivate();
        }
        KeyCode::Char('{') => {
            if app.multi.is_active() {
                app.multi_insert_char('{');
            } else {
                app.buffer.insert_char_pair('{', '}');
            }
            app.completions.deactivate();
        }
        KeyCode::Char('<') => {
            if app.multi.is_active() {
                app.multi_insert_char('<');
            } else {
                app.buffer.insert_char_pair('<', '>');
            }
            app.completions.deactivate();
        }
        KeyCode::Char('`') => {
            app.completions.deactivate();
            if app.multi.is_active() {
                app.multi_insert_char('`');
            } else if app.buffer.skip_char_if_match('`') {
            } else {
                app.buffer.insert_char_pair('`', '`');
            }
        }
        KeyCode::Char(c) => {
            app.multi_insert_char(c);
            app.update_scroll();
            if !app.multi.is_active() {
                auto_trigger_completion(app, c);
            }
        }
        _ => {
            app.completions.deactivate();
        }
    }
}

fn handle_visual(app: &mut App, code: KeyCode) {
    if app.pending_register {
        if let KeyCode::Char(c) = code {
            if app.registers.select(c) {
                app.message = format!("Register {} (visual)", app.registers.active_label());
            }
        }
        app.pending_register = false;
        return;
    }
    let is_block = app.mode == Mode::VisualBlock;
    match code {
        KeyCode::Esc => app.enter_normal(),
        KeyCode::Char('"') => {
            app.pending_register = true;
            app.message = String::from("-- register (visual) --");
        }
        KeyCode::Char('d') | KeyCode::Char('x') => {
            if is_block {
                app.delete_block();
            } else {
                app.delete_selection();
            }
        }
        KeyCode::Char('y') => {
            if is_block {
                app.yank_block();
            } else {
                app.yank_selection();
            }
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            if is_block {
                app.delete_block();
            } else {
                app.delete_selection();
            }
            app.paste_before();
        }
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
        KeyCode::Esc | KeyCode::Char('q') => {
            app.explorer.close();
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
    // Media / data files → pretty preview (images, csv, npy, audio)
    if xei_core::media::is_media_path(path) {
        app.explorer.close();
        match app.open_media_preview(path) {
            Ok(()) => {}
            Err(e) => {
                app.message = e;
                app.mode = Mode::Normal;
            }
        }
        return;
    }
    let path_str = path.display().to_string();
    app.open_new_tab(&path_str);
    app.explorer.close();
    app.mode = Mode::Normal;
}

/// Handle keys for the Ctrl+Shift+T terminal *window* (not side Ctrl+T mode).
///
/// **Strict PTY policy:** when this returns `true`, the key was fully handled
/// (almost always sent to the child). Returns `false` only for the tiny
/// allowlist that must reach editor chrome (Ctrl+W split chord second key, etc.).
fn handle_pane_terminal_window(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> bool {
    app.terminal.poll();

    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let shift = modifiers.contains(KeyModifiers::SHIFT);
    let alt = modifiers.contains(KeyModifiers::ALT);
    let super_key = modifiers.contains(KeyModifiers::SUPER);

    // Close confirmation dialog owns y/n/Esc
    if app.terminal.close_confirm {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.confirm_close_pane_terminal(true);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.confirm_close_pane_terminal(false);
            }
            _ => {
                app.message =
                    "Close terminal?  [y]es  /  [n]o · Ctrl+Shift+W cancels".into();
            }
        }
        return true;
    }

    // ── Editor allowlist (escape hatches only) ──────────────────────────
    // Ctrl+Shift+T — toggle/close terminal window
    if ctrl
        && shift
        && matches!(code, KeyCode::Char('t') | KeyCode::Char('T'))
    {
        app.toggle_terminal_full();
        return true;
    }
    // Ctrl+Shift+W — close confirm
    if ctrl
        && shift
        && matches!(code, KeyCode::Char('w') | KeyCode::Char('W'))
    {
        app.request_close_pane_terminal();
        return true;
    }
    // Ctrl+W alone — start split chord so user can focus the other pane
    if ctrl
        && !shift
        && !alt
        && !super_key
        && matches!(code, KeyCode::Char('w') | KeyCode::Char('W'))
    {
        app.split.pending_chord = true;
        app.begin_chord(
            "Ctrl+W",
            xei_core::which_key::as_hints(xei_core::which_key::map_ctrl_w()),
        );
        app.message = String::from("Ctrl+W — (terminal) focus other pane with w");
        return true;
    }
    // Second key of Ctrl+W chord while terminal still focused
    if app.split.pending_chord && !ctrl {
        // Let the normal Ctrl+W chord handler process this (returns false)
        return false;
    }

    // ── Everything else → PTY ───────────────────────────────────────────
    // Ctrl+C / Ctrl+D / Ctrl+Z / Ctrl+L … as real control bytes
    if ctrl && !super_key {
        if let KeyCode::Char(c) = code {
            let lower = c.to_ascii_lowercase();
            if lower.is_ascii_lowercase() {
                let byte = (lower as u8) - b'a' + 1;
                app.terminal.write_input(&[byte]);
                return true;
            }
        }
        // Ctrl+Arrow etc. — still useful in some REPLs
        match code {
            KeyCode::Left => {
                app.terminal.write_input(b"\x1b[1;5D");
                return true;
            }
            KeyCode::Right => {
                app.terminal.write_input(b"\x1b[1;5C");
                return true;
            }
            KeyCode::Up => {
                app.terminal.write_input(b"\x1b[1;5A");
                return true;
            }
            KeyCode::Down => {
                app.terminal.write_input(b"\x1b[1;5B");
                return true;
            }
            _ => {}
        }
    }

    // Alt+char → ESC + char (readline / fish bindings)
    if alt && !ctrl {
        if let KeyCode::Char(c) = code {
            let mut buf = [0u8; 8];
            buf[0] = 0x1b;
            let s = c.encode_utf8(&mut buf[1..]);
            let n = 1 + s.len();
            app.terminal.write_input(&buf[..n]);
            return true;
        }
    }

    write_terminal_key(app, code);
    true
}

fn write_terminal_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Enter => app.terminal.write_input(b"\r"),
        KeyCode::Backspace => app.terminal.write_input(&[0x7f]),
        KeyCode::Tab => app.terminal.write_input(b"\t"),
        // Arrows honor DECCKM (vim/less switch to application cursor keys).
        KeyCode::Left => {
            let seq = app.terminal.arrow_seq('D');
            app.terminal.write_input(seq);
        }
        KeyCode::Right => {
            let seq = app.terminal.arrow_seq('C');
            app.terminal.write_input(seq);
        }
        KeyCode::Up => {
            let seq = app.terminal.arrow_seq('A');
            app.terminal.write_input(seq);
        }
        KeyCode::Down => {
            let seq = app.terminal.arrow_seq('B');
            app.terminal.write_input(seq);
        }
        KeyCode::Home => app.terminal.write_input(b"\x1b[H"),
        KeyCode::End => app.terminal.write_input(b"\x1b[F"),
        KeyCode::PageUp => app.terminal.scroll_up(3),
        KeyCode::PageDown => app.terminal.scroll_down(3),
        KeyCode::Delete => app.terminal.write_input(b"\x1b[3~"),
        KeyCode::Esc => app.terminal.write_input(b"\x1b"),
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            app.terminal.write_input(s.as_bytes());
        }
        _ => {}
    }
}

/// Side-panel terminal (Ctrl+T) — still Mode::Terminal.
fn handle_terminal(app: &mut App, code: KeyCode) {
    app.terminal.poll();

    match code {
        KeyCode::Esc => {
            // Side terminal: Esc closes. Full-panel shouldn't land here, but if
            // it does, send Esc to PTY rather than stealing it.
            if app.terminal.full_panel {
                app.terminal.write_input(b"\x1b");
                return;
            }
            app.terminal.open = false;
            app.terminal.shutdown();
            app.mode = Mode::Normal;
        }
        other => write_terminal_key(app, other),
    }
}

fn handle_xlc(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => app.close_xlc(),
        KeyCode::Enter => {
            // All commands (including :wq / :x) go through App::execute_xlc
            app.execute_xlc();
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
            app.cancel_search();
        }
        KeyCode::Enter => {
            app.commit_search();
        }
        KeyCode::Backspace => {
            if app.search_input.is_empty() {
                app.cancel_search();
            } else {
                app.search_input.pop();
                app.update_search_input();
            }
        }
        KeyCode::Delete => {
            // same as backspace for single-line search
            if !app.search_input.is_empty() {
                app.search_input.pop();
                app.update_search_input();
            }
        }
        KeyCode::Down => {
            // Jump to next live match while still typing
            if !app.search_matches.is_empty() {
                app.search_current = (app.search_current + 1) % app.search_matches.len();
                let pos = app.search_matches[app.search_current];
                app.buffer.cursor = pos;
                app.update_scroll();
                app.message = format!(
                    "/{}/  {}/{}",
                    app.search_input,
                    app.search_current + 1,
                    app.search_matches.len()
                );
            }
        }
        KeyCode::Up => {
            if !app.search_matches.is_empty() {
                app.search_current = if app.search_current == 0 {
                    app.search_matches.len() - 1
                } else {
                    app.search_current - 1
                };
                let pos = app.search_matches[app.search_current];
                app.buffer.cursor = pos;
                app.update_scroll();
                app.message = format!(
                    "/{}/  {}/{}",
                    app.search_input,
                    app.search_current + 1,
                    app.search_matches.len()
                );
            }
        }
        KeyCode::Char(c) => {
            if !c.is_control() {
                app.search_input.push(c);
                app.update_search_input();
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
        // Flush pending edits first so completions are computed at the
        // position the user actually sees.
        app.sync_lsp_document();
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
