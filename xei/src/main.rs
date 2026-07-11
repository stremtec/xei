use std::env;
use std::io::{self, Write};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};

use xei_core::App;

mod event;
mod gpu_frame;
mod kitty_gfx;
mod term_caps;
mod ui;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-V" => {
                println!("xei {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("xei (晴) — a modern Vim-like terminal editor\n");
                println!("Usage: xei [FILE]       Open a file for editing");
                println!("       xei --version     Print version");
                println!("       xei --help        Show this help");
                println!("       xei --debug FILE  Open with debug logging\n");
                println!("Homepage: https://github.com/stremtec/xei");
                return Ok(());
            }
            _ => {}
        }
    }

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        gpu_frame::force_end_sync();
        let _ = disable_raw_mode();
        // Popping an empty enhancement stack is a no-op — safe unconditionally.
        let _ = execute!(
            io::stdout(),
            crossterm::event::PopKeyboardEnhancementFlags,
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        prev_hook(info);
    }));

    let mut app = if args.len() > 1 && !args[1].starts_with('-') {
        App::open_file(&args[1])
    } else if args.len() > 2 && args[1] == "--debug" {
        let mut a = App::open_file(&args[2]);
        a.debug = true;
        a
    } else {
        // No file args → restore last session if available
        let mut a = App::new();
        a.restore_session();
        a
    };

    if let Some(saved_theme) = xei_core::config::load_theme() {
        if let Some(t) = xei_core::theme::find(&saved_theme) {
            app.theme = t;
        }
    }

    // Progressive GPU-terminal caps (gated later by app.gpu_acc).
    let caps = term_caps::TerminalCaps::detect();
    app.set_term_caps(
        caps.summary(),
        caps.sync_output,
        caps.undercurl,
        caps.underline_color,
        caps.hyperlinks,
        caps.modern,
        caps.kitty_graphics,
    );
    // Re-apply pet now that Kitty caps are known (config load was earlier).
    {
        let cfg = xei_core::config::load();
        app.apply_pet_from_config(&cfg);
    }

    xei_core::set_cursor_esc(app.theme.cursor);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // Kitty keyboard protocol (CSI-u) when the terminal offers it — without
    // this, Ctrl+Shift chords are indistinguishable from plain Ctrl ones on
    // terminals that don't send fixterms sequences by default. Legacy
    // terminals keep working through the Space-leader / `:` fallbacks.
    let kbd_enhanced =
        crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if kbd_enhanced {
        let _ = execute!(
            stdout,
            crossterm::event::PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            )
        );
    } else {
        // Terminal.app / plain xterm can't encode Ctrl+Shift, Ctrl+, or
        // Ctrl+. at all — tell the user where the fallbacks live.
        app.message =
            "⌨ legacy terminal: Ctrl+Shift/Ctrl+,/Ctrl+. unavailable — use SPC leader · :settings · :help"
                .into();
    }

    // Clear any leftover underline-style SGR from a previous crashed session.
    // Never set session-wide undercurl (CSI 4:3) — it paints waves on every cell.
    gpu_frame::reset_underline_sgr_stdout();
    term_caps::drain_input_noise();

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    if app.gpu_acc && caps.modern {
        app.message = format!("xei · {}", caps.summary());
    }

    let result = run_app(&mut terminal, &mut app, caps);

    gpu_frame::force_end_sync();
    disable_raw_mode()?;
    if kbd_enhanced {
        let _ = execute!(
            terminal.backend_mut(),
            crossterm::event::PopKeyboardEnhancementFlags
        );
    }
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    let _ = std::panic::take_hook();
    result
}

/// Place / animate desktop pet GIF with Kitty graphics protocol.
///
/// **Cursor contract:** when we do not paint, we write **nothing** to stdout
/// (no cursor hide/show, no CUP). When we paint, restore the editor caret in
/// the same write as the image — never leave the caret on the pet cell.
///
/// Returns whether a paint happened (caller may re-assert cursor).
fn paint_pet(
    app: &mut App,
    caps: &term_caps::TerminalCaps,
    pet_placed: &mut bool,
    last_frame_idx: &mut usize,
    last_pos: &mut (u16, u16),
    editor_cursor: Option<(u16, u16)>,
) -> bool {
    const CELL_PX: u32 = 14;

    let ok = app.pet.enabled
        && app.pet.has_frames()
        && app.pet_graphics_ok()
        && kitty_gfx::available(app.gpu_acc, caps);

    if !ok {
        if *pet_placed && kitty_gfx::available(app.gpu_acc, caps) {
            let mut out = io::stdout();
            let _ = kitty_gfx::delete_image_flush(&mut out, app.pet.image_id);
        }
        *pet_placed = false;
        *last_frame_idx = usize::MAX;
        *last_pos = (u16::MAX, u16::MAX);
        if app.pet.enabled && !app.pet_graphics_ok() {
            app.pet.enabled = false;
        }
        return false;
    }

    app.pet.ensure_display_cache(CELL_PX);

    let frame_changed = app.pet.tick();
    let idx = app.pet.frame_idx();
    // Paint with clamped screen coords; keep configured x/y intact.
    let (sx, sy) = app.pet_screen_xy();
    let pos = (sx, sy);
    let pos_changed = pos != *last_pos;
    let need_paint = frame_changed || !*pet_placed || idx != *last_frame_idx || pos_changed;
    if !need_paint {
        return false;
    }

    let Some(disp) = app.pet.current_display() else {
        return false;
    };
    if disp.width == 0 || disp.height == 0 {
        return false;
    }

    let use_sync = gpu_frame::should_sync(app.gpu_acc, caps);
    let mut out = io::stdout();
    if kitty_gfx::place_rgba_rect_b64(
        &mut out,
        app.pet.image_id,
        disp.width,
        disp.height,
        Some(&disp.b64),
        &disp.rgba,
        sx as u32,
        sy as u32,
        editor_cursor,
        use_sync,
    )
    .is_ok()
    {
        *pet_placed = true;
        *last_frame_idx = idx;
        *last_pos = pos;
        return true;
    }
    false
}

/// Place image preview via Kitty when PreviewKind::Image is open.
/// Only rewrites stdout when size/open-state changes (cursor-safe).
fn paint_media_preview(
    app: &mut App,
    caps: &term_caps::TerminalCaps,
    editor_cursor: Option<(u16, u16)>,
    last_sig: &mut Option<(u32, u16, u32, u32)>,
) -> bool {
    let show = app.mode == xei_core::Mode::Preview
        && app.preview.open
        && !app.preview.closing
        && matches!(app.preview.kind, Some(xei_core::PreviewKind::Image))
        && app.preview_image.is_some()
        && kitty_gfx::available(app.gpu_acc, caps);

    if !show {
        if last_sig.is_some() && kitty_gfx::available(app.gpu_acc, caps) {
            if let Some((id, _, _, _)) = *last_sig {
                let mut out = io::stdout();
                let _ = kitty_gfx::delete_image_flush(&mut out, id);
            }
        }
        *last_sig = None;
        return false;
    }

    let Some(img) = app.preview_image.as_ref() else {
        return false;
    };
    if img.cached_w == 0 || img.cached_h == 0 || img.cached_b64.is_empty() {
        return false;
    }

    let sig = (img.kitty_id, img.width_cells, img.cached_w, img.cached_h);
    if *last_sig == Some(sig) {
        return false; // already placed at this size
    }

    let col = (app.viewport.x.saturating_add(6)) as u32;
    let row = (app.viewport.y.saturating_add(3)) as u32;

    let mut out = io::stdout();
    let use_sync = gpu_frame::should_sync(app.gpu_acc, caps);
    let ok = kitty_gfx::place_rgba_rect_b64(
        &mut out,
        img.kitty_id,
        img.cached_w,
        img.cached_h,
        Some(&img.cached_b64),
        &img.cached_rgba,
        col,
        row,
        editor_cursor,
        use_sync,
    )
    .is_ok();
    if ok {
        *last_sig = Some(sig);
    }
    ok
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    caps: term_caps::TerminalCaps,
) -> io::Result<()> {
    let mut lsp_sync_tick: u32 = 0;
    let mut pet_placed = false;
    let mut pet_last_frame = usize::MAX;
    let mut pet_last_pos: (u16, u16) = (u16::MAX, u16::MAX);
    let mut media_sig: Option<(u32, u16, u32, u32)> = None;
    while app.running {
        let use_sync = gpu_frame::should_sync(app.gpu_acc, &caps);
        gpu_frame::draw_synced(terminal, use_sync, |f| ui::draw(f, app))?;
        // Ratatui's caret after the frame — only needed when Kitty graphics
        // will paint. Skipping otherwise avoids a blocking CSI-6n round-trip
        // per frame (noticeable over slow SSH links).
        let editor_cursor = if kitty_gfx::available(app.gpu_acc, &caps) {
            terminal.get_cursor_position().ok().map(|p| (p.x, p.y))
        } else {
            None
        };
        // Do not clamp pet.x/y here — that used to wipe saved bottom-right coords
        // when screen size was still the default 80×24 on early frames.

        // Phase B: Kitty decorations for peek (rare path)
        if app.peek.open {
            if kitty_gfx::available(app.gpu_acc, &caps) {
                let mut out = io::stdout();
                let _ = kitty_gfx::place_shadow_bar(&mut out, 42, 56, 10, 3, 3);
            }
            if gpu_frame::should_hyperlink(app.gpu_acc, &caps) {
                let url = term_caps::file_url(&app.peek.path.display().to_string());
                let label = app
                    .peek
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file");
                let mut out = io::stdout();
                let _ = write!(
                    out,
                    "\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\",
                    url, label
                );
                let _ = out.flush();
            }
        } else if kitty_gfx::available(app.gpu_acc, &caps) {
            let mut out = io::stdout();
            let _ = kitty_gfx::delete_image(&mut out, 42);
            let _ = out.flush();
        }

        // Media image preview (Kitty) — same cursor-safe place path as pet
        let painted_media = paint_media_preview(app, &caps, editor_cursor, &mut media_sig);

        // Desktop pet — only writes stdout when a GIF frame actually advances
        let painted_pet = paint_pet(
            app,
            &caps,
            &mut pet_placed,
            &mut pet_last_frame,
            &mut pet_last_pos,
            editor_cursor,
        );
        // Re-assert caret only if something touched the terminal after ratatui.
        if (painted_pet || painted_media) && let Some((cx, cy)) = editor_cursor {
            let _ = terminal.set_cursor_position((cx, cy));
        }
        if !event::handle_events(app)? {
            break;
        }
        // Background Git workbench loads (PRs / Issues / Auth / Branches)
        let _ = app.git_wb.poll_loading();
        // Post-edit didChange sync, throttled to ~every 5 frames (≈50ms) so
        // fast typing coalesces into one full-text notification.
        lsp_sync_tick = lsp_sync_tick.wrapping_add(1);
        if lsp_sync_tick % 5 == 0 {
            app.sync_lsp_document();
        }
        app.lsp.poll();
        app.poll_call_hierarchy();
        if app.pr_review.poll() {
            // Surface fetch results (count / error) in the status line too.
            app.message = app.pr_review.message.clone();
        }
        app.poll_hook_messages();
        app.dap.poll();
        if app.dap.location_dirty {
            app.dap_apply_stopped_location();
        }
        let lsp_comps: Vec<_> = std::mem::take(&mut app.lsp.pending_completions);
        if !lsp_comps.is_empty() && app.completions.active {
            for item in lsp_comps {
                let exists = app.completions.suggestions.iter().any(|s| s.label == item.label);
                if !exists {
                    app.completions.suggestions.push(xei_core::completion::Suggestion {
                        label: item.label.clone(),
                        detail: item.detail.unwrap_or_else(|| "LSP".to_string()),
                        insert_text: item.label,
                    });
                }
            }
        }
        if let Some(loc) = app.lsp.pending_definition.take() {
            let path_str = loc.path.clone();
            let as_peek = app.lsp.definition_as_peek;
            app.lsp.definition_as_peek = false;
            // Convert UTF-16 col using file text when possible
            let file_text = std::fs::read_to_string(&path_str).ok();
            let line_for_col = file_text
                .as_ref()
                .and_then(|t| t.lines().nth(loc.row))
                .unwrap_or("");
            let col = xei_core::lsp::utf16_to_char_col(line_for_col, loc.col);
            if as_peek {
                app.open_peek_at(&path_str, loc.row, col);
            } else {
                app.open_new_tab(&path_str);
                app.buffer.cursor.row = loc.row.min(app.buffer.line_count().saturating_sub(1));
                let line = app.buffer.line(app.buffer.cursor.row);
                app.buffer.cursor.col = xei_core::lsp::utf16_to_char_col(line, loc.col);
                app.buffer.clamp_col();
                app.update_scroll();
                app.sync_split_from_active();
                app.message = format!("Jumped to definition: {}:{}", path_str, loc.row + 1);
            }
        }
        // Document / workspace symbols → palette
        if !app.lsp.pending_symbols.is_empty() {
            app.apply_pending_symbols();
        }
        // Inlay hints + code lens refresh
        if let Some(ref path) = app.filename.clone() {
            let path_s = path.display().to_string();
            if app.inlay_hints_enabled {
                let end = app.buffer.line_count().saturating_sub(1);
                app.lsp.maybe_request_inlays(&path_s, end);
            }
            if app.code_lens_enabled {
                app.lsp.maybe_request_code_lens(&path_s);
            }
        }
        if let Some(hover) = app.lsp.pending_hover.take() {
            // Keep hover compact for popup
            let short: String = hover.chars().take(800).collect();
            app.hover_text = Some(short);
            app.message = String::from("Hover (Esc to dismiss)");
        }
        if !app.lsp.pending_references.is_empty() {
            let refs = std::mem::take(&mut app.lsp.pending_references);
            // Jump to first; list rest in XLC
            if let Some(first) = refs.first() {
                app.push_jump();
                if app.filename.as_ref().map(|p| p.display().to_string()) != Some(first.path.clone())
                {
                    app.open_new_tab(&first.path);
                }
                app.buffer.cursor.row = first.row.min(app.buffer.line_count().saturating_sub(1));
                let line = app.buffer.line(app.buffer.cursor.row);
                app.buffer.cursor.col = xei_core::lsp::utf16_to_char_col(line, first.col);
                app.buffer.clamp_col();
                app.update_scroll();
            }
            app.xlc.open = true;
            app.xlc.add_output(&format!("=== {} reference(s) ===", refs.len()));
            for (i, r) in refs.iter().enumerate() {
                app.xlc.add_output(&format!(
                    "  {}. {}:{}:{}",
                    i + 1,
                    r.path,
                    r.row + 1,
                    r.col + 1
                ));
            }
            app.message = format!("{} reference(s) — see XLC panel", refs.len());
        }
        // Multi-file workspace edits (rename / format / code action)
        if !app.lsp.pending_edits.is_empty() {
            let edits = std::mem::take(&mut app.lsp.pending_edits);
            app.apply_file_edits(edits);
        }
        if let Some(msg) = app.lsp.pending_workspace_edit.take() {
            // Status-only notes (no APPLY payload)
            if !msg.starts_with("APPLY\n") {
                app.message = msg;
            }
        }
        // Code actions ready → palette
        if !app.lsp.pending_code_actions.is_empty() {
            app.open_code_actions_palette();
        }
        // Soft error → status once
        if let Some(soft) = app.lsp.soft_error.take() {
            if app.message.is_empty() || app.message.ends_with('…') {
                app.message = soft;
            }
        }
        // Status: recording indicator
        if let Some(reg) = app.macros.recording {
            if !app.message.contains("Recording") {
                app.message = format!("Recording @{}…", reg);
            }
        }
        app.check_external_change();
    }
    Ok(())
}
