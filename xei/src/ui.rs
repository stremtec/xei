use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, BorderType, Clear, List, ListItem, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

use xei_core::app::{App, EditorViewport, Mode};
use xei_core::highlight;

const LINE_NO_WIDTH: u16 = 5;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Full-screen canvas
    f.render_widget(
        Block::default().style(Style::default().bg(app.theme.bg).fg(app.theme.fg)),
        area,
    );

    // `:screensaver` / xeifetch — exclusive full-screen overlay
    if app.mode == Mode::Screensaver {
        draw_screensaver(f, app, area);
        app.screen_width = area.width;
        app.screen_height = area.height;
        return;
    }

    let ext = app.file_extension();
    let text = app.buffer.text();
    app.syntax.parse(&text, ext.as_deref());

    // Wheel-routing rects are re-recorded by whichever surfaces draw this frame.
    app.terminal_rect = None;
    if !app.dap.panel_open {
        app.dap_panel_rect = None;
    }

    let explorer_open = app.explorer.open;
    let terminal_open = app.terminal.open && !app.terminal.full_panel;
    // Search is a lightweight bar — never steals the XLC panel.
    let xlc_open = app.xlc.open && app.mode == Mode::XlcInput;
    let search_open = app.mode == Mode::Search;

    // Outer: tab bar + breadcrumbs + body + optional search bar + status
    let show_crumbs = app.filename.is_some() || app.buffers.len() > 1;
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints({
            let mut c = vec![Constraint::Length(1)]; // tab bar
            if show_crumbs {
                c.push(Constraint::Length(1)); // breadcrumbs
            }
            c.push(Constraint::Min(1)); // body
            if search_open {
                c.push(Constraint::Length(1)); // search
            }
            c.push(Constraint::Length(1)); // status
            c
        })
        .split(area);

    let mut idx = 0usize;
    draw_tabbar(f, app, outer[idx]);
    idx += 1;
    if show_crumbs {
        draw_breadcrumbs(f, app, outer[idx]);
        idx += 1;
    }
    let body = outer[idx];
    idx += 1;
    let status_area = if search_open {
        draw_search_bar(f, app, outer[idx]);
        outer[idx + 1]
    } else {
        outer[idx]
    };
    let (explorer_rect, main_rect, term_rect) = {
        let mut constraints = vec![];
        let mut has_explorer = false;
        let mut has_terminal = false;

        if explorer_open {
            constraints.push(Constraint::Length(app.explorer_width));
            has_explorer = true;
        }
        constraints.push(Constraint::Min(1));
        if terminal_open {
            constraints.push(Constraint::Length(app.terminal_width));
            has_terminal = true;
        }

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(body);

        let mut idx = 0;
        let expl = if has_explorer {
            idx += 1;
            chunks[idx - 1]
        } else {
            Rect::default()
        };
        let main = chunks[idx];
        idx += 1;
        let term = if has_terminal {
            chunks[idx]
        } else {
            Rect::default()
        };

        (expl, main, term)
    };

    // Flat mode switches take over the editor pane (same z-layer — not overlays).
    let preview_active = app.preview.open && app.mode == Mode::Preview;
    let git_wb_active = app.git_wb.visible() && matches!(app.mode, Mode::GitWorkbench);
    // Whole-main full terminal only when not bound to a split pane.
    // Pane terminal is a window (not Mode::Terminal) — draw whenever open.
    let term_full_main = app.terminal.open
        && app.terminal.full_panel
        && app.terminal.pane_bound.is_none();
    // Pane-local terminal window is drawn inside the split layout.
    let term_full_pane = app.terminal.open
        && app.terminal.full_panel
        && app.terminal.pane_bound.is_some();

    let debug_open = app.dap.panel_open
        && !git_wb_active
        && !preview_active
        && !term_full_main;

    let editor_area = if xlc_open {
        let xlc_total = app.xlc_height;
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(xlc_total)])
            .split(main_rect);

        if term_full_main {
            app.terminal.poll();
            draw_terminal(f, app, chunks[0]);
        } else if git_wb_active {
            draw_git_workbench(f, app, chunks[0]);
        } else if debug_open {
            draw_editor_with_debug(f, app, chunks[0]);
        } else {
            if term_full_pane {
                app.terminal.poll();
            }
            // Preview renders inside the focused pane (split-aware) here.
            draw_editor_split_or_single(f, app, chunks[0]);
        }
        draw_xlc(f, app, chunks[1]);
        app.xlc_separator_y = chunks[1].y;
        chunks[0]
    } else {
        if term_full_main {
            app.terminal.poll();
            draw_terminal(f, app, main_rect);
        } else if git_wb_active {
            draw_git_workbench(f, app, main_rect);
        } else if debug_open {
            draw_editor_with_debug(f, app, main_rect);
        } else {
            if term_full_pane {
                app.terminal.poll();
            }
            // Preview renders inside the focused pane (split-aware) here.
            draw_editor_split_or_single(f, app, main_rect);
        }
        main_rect
    };

    if explorer_open {
        draw_explorer(f, app, explorer_rect);
    }

    // Side terminal only when not in full-panel mode
    if terminal_open && !app.terminal.full_panel {
        app.terminal.poll();
        draw_terminal(f, app, term_rect);
    }

    draw_statusline(f, app, status_area);

    app.screen_width = area.width;
    app.screen_height = area.height;
    app.explorer_separator_x = if explorer_open {
        explorer_rect.x + explorer_rect.width
    } else {
        0
    };
    app.terminal_separator_x = if terminal_open { term_rect.x } else { 0 };

    // Viewport geometry for mouse hit-testing (text origin = first content cell)
    app.viewport = EditorViewport {
        x: editor_area.x,
        y: editor_area.y,
        width: editor_area.width,
        height: editor_area.height,
        text_x: editor_area.x + LINE_NO_WIDTH,
        text_y: editor_area.y,
    };

    if app.peek.open {
        draw_peek(f, app, area);
    }

    if app.mode == Mode::WorkspaceSearch && app.workspace_search.open {
        draw_workspace_search(f, app, area);
    }

    if app.completions.active {
        draw_completions(f, app, area);
    }

    if app.which_key_visible() {
        draw_pending_hints(f, app, area);
    }

    if app.mode == Mode::Palette && app.palette.open {
        draw_palette(f, app, area);
    }

    if app.mode == Mode::CallHierarchy && app.call_hierarchy.open {
        draw_call_hierarchy(f, app, area);
    }

    if app.mode == Mode::Rebase && app.rebase.open {
        draw_rebase_panel(f, app, area);
    }

    if app.mode == Mode::PrReview && app.pr_review.open {
        draw_pr_review(f, app, body);
    }

    if app.editor_ctx.is_some() {
        draw_editor_ctx_menu(f, app, area);
    }

    // SCM stays open=true through the close slide; settle mode after anim ends.
    // (Git workbench is drawn in the editor slot above — not a high-z overlay.)
    if app.scm.visible() && matches!(app.mode, Mode::SourceControl) {
        draw_scm(f, app, area);
    }
    if app.settings.visible() && matches!(app.mode, Mode::Settings) {
        draw_settings(f, app, area);
    }
    app.settle_anims();

    if let Some(ref hover) = app.hover_text {
        draw_hover(f, app, area, hover);
    }

    // Pet cell fallback when Kitty graphics aren't available
    // Marker only when enabled-but-no-graphics (defensive; pet should stay off then).
    if app.pet.enabled && app.pet.has_frames() && !app.pet_graphics_ok() {
        draw_pet_marker(f, app, area);
    }
}

/// Small 🐾 marker only when pet is on but Kitty graphics is unavailable.
fn draw_pet_marker(f: &mut Frame, app: &App, area: Rect) {
    // Real GIF path needs GPU+Kitty; marker is a fallback, not a second copy.
    if app.pet_graphics_ok() {
        return;
    }
    let x = app.pet.x.min(area.width.saturating_sub(1));
    let y = app.pet.y.min(area.height.saturating_sub(1));
    f.render_widget(
        Paragraph::new(Span::styled(
            "🐾",
            Style::default().fg(app.theme.accent),
        )),
        Rect::new(area.x + x, area.y + y, 2.min(area.width.saturating_sub(x)), 1),
    );
}

/// `:screensaver` — xeifetch: welcome logo + neofetch facts + cryptex + weather.
/// Easter egg: `/` then type `fakers` → everything becomes **god** (until close).
fn draw_screensaver(f: &mut Frame, app: &mut App, area: Rect) {
    use xei_core::screensaver::godify;

    app.screensaver.poll();
    let god = app.screensaver.god_mode;

    if !god {
        if let Some(ref w) = app.screensaver.weather {
            if app.message.contains("weather loading") || app.message.starts_with("xeifetch · Esc")
            {
                let t = w
                    .temp_c
                    .map(|c| format!(" {c:.0}°C"))
                    .unwrap_or_default();
                app.message = format!("xeifetch · {} {}{} · Esc exit", w.emoji, w.city, t);
            }
        }
    } else if app.message != "god." {
        app.message = "god.".into();
    }

    let accent = app.theme.mode_normal;
    let dim = app.theme.line_no;
    let fg = app.theme.fg;
    let muted = app.theme.muted;
    let brass = app.theme.accent;
    let brass_dim = dim;

    f.render_widget(
        Block::default().style(Style::default().bg(app.theme.editor_bg).fg(fg)),
        area,
    );

    let (h, m, s) = xei_core::screensaver::local_hms();
    let spin = app.screensaver.opened_at.elapsed().as_millis() as u64;
    let input = if app.screensaver.cryptex_input {
        Some(app.screensaver.cryptex_buf.as_str())
    } else {
        None
    };
    let cryptex = xei_core::screensaver::cryptex_lines(h, m, s, spin, input, god);
    let info = xei_core::screensaver::system_info();

    let clock_w = xei_core::screensaver::CRYPTEX_WIDTH as u16;
    let gap = 4u16;
    let content_w = area.width.saturating_sub(6).min(84).max(48);
    let content_h = area.height.saturating_sub(4).min(24).max(16);
    let cx = area.x + (area.width.saturating_sub(content_w)) / 2;
    let cy = area.y + (area.height.saturating_sub(content_h)) / 2;
    let left_w = content_w.saturating_sub(clock_w + gap).max(20);

    // ── Logo block ─────────────────────────────────────────────────────
    // Left edge of shade art (first ░) lines up with info keys ("user", …)
    // which use a 2-space indent. Brand/tag stay centered under the logo width
    // so 晴 still sits above "x e i" / "xeifetch".
    let logo = if god {
        "god  god  晴  god  god"
    } else {
        "░ ▒ ▓ █  晴  █ ▓ ▒ ░"
    };
    let brand = if god { "g  o  d" } else { "x  e  i" };
    let tag = if god { "god" } else { "xeifetch" };
    let logo_dw = ss_display_width(logo);
    let brand_line = ss_center_in(brand, logo_dw);
    let tag_line = ss_center_in(tag, logo_dw);
    let indent = "  "; // same as info rows: `  {key:<8}`
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("{indent}{logo}"),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        )),
        Rect::new(cx, cy, left_w, 1),
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("{indent}{brand_line}"),
            Style::default().fg(fg).add_modifier(Modifier::BOLD),
        )),
        Rect::new(cx, cy + 1, left_w, 1),
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("{indent}{tag_line}"),
            Style::default()
                .fg(muted)
                .add_modifier(Modifier::ITALIC),
        )),
        Rect::new(cx, cy + 2, left_w, 1),
    );

    // ── Info rows ──────────────────────────────────────────────────────
    let label_style = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let val_style = Style::default().fg(fg);
    let mut row_y = cy + 4;
    for (k, v) in &info {
        if row_y >= cy + content_h.saturating_sub(4) {
            break;
        }
        let key = godify(k, god);
        let val = if god {
            "god".into()
        } else {
            v.chars()
                .take(left_w.saturating_sub(12) as usize)
                .collect::<String>()
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {key:<8}"), label_style),
                Span::styled(val, val_style),
            ])),
            Rect::new(cx, row_y, left_w, 1),
        );
        row_y += 1;
    }

    // Weather
    let weather_line = if god {
        "  god     god".into()
    } else if let Some(ref w) = app.screensaver.weather {
        let t = w
            .temp_c
            .map(|c| format!(" {c:.0}°C"))
            .unwrap_or_default();
        format!("  {:<8}{} {} · {}{}", "weather", w.emoji, w.city, w.label, t)
    } else if app.screensaver.open {
        "  weather … locating".into()
    } else {
        String::new()
    };
    if !weather_line.is_empty() && row_y < cy + content_h.saturating_sub(2) {
        f.render_widget(
            Paragraph::new(Span::styled(
                weather_line,
                Style::default().fg(if god {
                    accent
                } else {
                    app.theme.success
                }),
            )),
            Rect::new(cx, row_y, left_w, 1),
        );
        row_y += 1;
    }

    // Time
    if row_y < cy + content_h.saturating_sub(1) {
        let tlabel = godify("time", god);
        let tval = if god {
            "god".into()
        } else {
            format!("{h:02}:{m:02}:{s:02}")
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {tlabel:<8}"), label_style),
                Span::styled(
                    tval,
                    Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD),
                ),
            ])),
            Rect::new(cx, row_y, left_w, 1),
        );
    }

    // ── Cryptex ────────────────────────────────────────────────────────
    let clock_x = cx + left_w + gap;
    let clock_y = cy;
    for (i, line) in cryptex.iter().enumerate() {
        let y = clock_y + i as u16;
        if y >= area.y + area.height.saturating_sub(2) {
            break;
        }
        let w = (line.chars().count() as u16).min(area.width.saturating_sub(clock_x));
        let style = match i {
            0 => Style::default()
                .fg(brass)
                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
            1 | 3 | 9 | 11 => Style::default().fg(brass),
            2 | 10 => Style::default().fg(brass_dim),
            6 => Style::default()
                .fg(app.theme.accent_fg)
                .bg(if app.screensaver.cryptex_input {
                    app.theme.mode_insert
                } else {
                    brass
                })
                .add_modifier(Modifier::BOLD),
            12 => Style::default().fg(muted),
            _ => Style::default().fg(dim),
        };
        f.render_widget(
            Paragraph::new(Span::styled(line.clone(), style)),
            Rect::new(clock_x, y, w, 1),
        );
    }

    // Footer
    let foot_y = area.y + area.height.saturating_sub(2);
    let foot = if god {
        "god · god · god"
    } else if app.screensaver.cryptex_input {
        "cryptex open · type combination · Enter  ·  Esc cancel"
    } else {
        "Esc exit  ·  / cryptex  ·  any other key exit"
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            foot,
            Style::default().fg(muted).add_modifier(Modifier::ITALIC),
        ))
        .alignment(Alignment::Center),
        Rect::new(area.x, foot_y, area.width, 1),
    );
}

/// VS Code–style Source Control panel (Ctrl+G).
fn draw_scm(f: &mut Frame, app: &mut App, area: Rect) {
    use xei_core::scm::ScmFocus;

    // Deferred graph load — first open paints status immediately.
    app.scm.ensure_graph();

    let full_w = area.width.saturating_sub(4).min(48).max(36);
    let height = area.height.saturating_sub(2).min(area.height.saturating_sub(1));
    // Openness 0 = fully off-screen right, 1 = docked. Pure **translate**
    // (not width-scale) so the panel keeps its shape while sliding.
    let linear = app.scm.anim_progress();
    let t = if app.scm.closing {
        // Ease-in as it leaves (accelerate off-screen).
        ease_in_cubic(linear)
    } else {
        ease_out_cubic(linear)
    };
    if t <= 0.001 {
        return;
    }
    let max_off = full_w as f32 + 6.0;
    let off = ((1.0 - t) * max_off).round() as i32;
    let dock_x = (area.x + area.width.saturating_sub(full_w).saturating_sub(1)) as i32;
    let x = (dock_x + off).max(area.x as i32) as u16;
    // Clip if partially off the right edge
    let right = area.x + area.width;
    if x >= right {
        return;
    }
    let width = full_w.min(right.saturating_sub(x)).max(1);
    let y = area.y + 1;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };

    // Soft shadow that fades with openness
    if t > 0.15 && width > 4 {
        let shadow_a = (t * 0.55).clamp(0.0, 0.55);
        let shadow_bg = lerp_color(app.theme.bg, Color::Black, shadow_a);
        f.render_widget(
            Block::default().style(Style::default().bg(shadow_bg)),
            Rect {
                x: x.saturating_add(1),
                y: y.saturating_add(1),
                width: width.saturating_sub(1),
                height: height.saturating_sub(1),
            },
        );
    }

    let branch = if app.scm.branch.is_empty() {
        "git".to_string()
    } else {
        let mut b = app.scm.branch.clone();
        if app.scm.ahead > 0 || app.scm.behind > 0 {
            b.push_str(&format!(" ↑{} ↓{}", app.scm.ahead, app.scm.behind));
        }
        b
    };
    let title = format!(" SOURCE CONTROL · {} ", branch);

    // Fade border/title in with the slide
    let scm_accent = lerp_color(
        app.theme.completion_bg,
        app.theme.accent,
        (0.25 + 0.75 * t).clamp(0.0, 1.0),
    );
    let panel_bg = lerp_color(app.theme.bg, app.theme.completion_bg, (0.55 + 0.45 * t).min(1.0));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(scm_accent))
        .style(Style::default().bg(panel_bg).fg(app.theme.fg))
        .title(Span::styled(
            title,
            Style::default().fg(scm_accent).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);
    // Skip dense content until the panel has mostly slid in (keeps motion clean)
    if t < 0.2 || inner.width < 8 {
        return;
    }

    // message + commit + changes + graph (VS Code-like split; graph gets more room)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // message
            Constraint::Length(1), // commit button
            Constraint::Length(1), // blank
            Constraint::Min(4),    // changes lists
            Constraint::Length(1), // graph header
            Constraint::Min(10),   // pretty graph
            Constraint::Length(2), // commit detail + hints
        ])
        .split(inner);

    // ── Commit message (VS Code Message box) ──
    let msg_focused = app.scm.focus == ScmFocus::Message;
    let msg_style = if msg_focused {
        Style::default()
            .fg(app.theme.fg)
            .bg(app.theme.panel_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(app.theme.line_no).bg(app.theme.panel_bg)
    };
    let msg_display = if app.scm.message.is_empty() && !msg_focused {
        " Message (Enter to commit on …".to_string()
    } else if msg_focused {
        format!(" {}█", app.scm.message)
    } else {
        format!(" {}", app.scm.message)
    };
    f.render_widget(
        Paragraph::new(msg_display).style(msg_style),
        chunks[0],
    );

    // ── Commit button ──
    let btn_focused = app.scm.focus == ScmFocus::CommitButton;
    let btn_style = if btn_focused {
        Style::default()
            .fg(Color::Black)
            .bg(app.theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(app.theme.accent)
            .bg(app.theme.panel_sel_bg)
    };
    let staged_n = app.scm.staged.len();
    let btn_label = if staged_n > 0 {
        format!("  ✓ Commit ({})  ", staged_n)
    } else {
        "  ✓ Commit  ".to_string()
    };
    f.render_widget(Paragraph::new(btn_label).style(btn_style), chunks[1]);

    // ── Changes list ──
    let list_h = chunks[3].height as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut flat_idx = 0usize;

    let push_header = |lines: &mut Vec<Line>, title: &str, count: usize| {
        lines.push(Line::from(Span::styled(
            format!(" ▾ {}  {}", title, count),
            Style::default()
                .fg(app.theme.line_no)
                .add_modifier(Modifier::BOLD),
        )));
    };

    if !app.scm.staged.is_empty() {
        push_header(&mut lines, "Staged Changes", app.scm.staged.len());
        for e in &app.scm.staged {
            let selected = app.scm.focus == ScmFocus::Changes && app.scm.selected == flat_idx;
            let (fg, bg) = if selected {
                (Color::Black, app.theme.completion_selected)
            } else {
                (app.theme.fg, app.theme.completion_bg)
            };
            let st_color = status_color(e.status, app.theme);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", e.status.letter()),
                    Style::default().fg(if selected { Color::Black } else { st_color }).bg(bg),
                ),
                Span::styled(
                    truncate_path(&e.path, (width as usize).saturating_sub(8)),
                    Style::default().fg(fg).bg(bg),
                ),
            ]));
            flat_idx += 1;
        }
    }

    push_header(&mut lines, "Changes", app.scm.changes.len());
    if app.scm.changes.is_empty() && app.scm.staged.is_empty() {
        if let Some(ref err) = app.scm.error {
            lines.push(Line::from(Span::styled(
                format!("  {}", err),
                Style::default().fg(app.theme.error),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "  No changes",
                Style::default().fg(app.theme.line_no),
            )));
        }
    }
    for e in &app.scm.changes {
        let selected = app.scm.focus == ScmFocus::Changes && app.scm.selected == flat_idx;
        let (fg, bg) = if selected {
            (Color::Black, app.theme.completion_selected)
        } else {
            (app.theme.fg, app.theme.completion_bg)
        };
        let st_color = status_color(e.status, app.theme);
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", e.status.letter()),
                Style::default()
                    .fg(if selected { Color::Black } else { st_color })
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                truncate_path(&e.path, (width as usize).saturating_sub(8)),
                Style::default().fg(fg).bg(bg),
            ),
        ]));
        flat_idx += 1;
    }

    // Scroll so selected is visible
    let sel_line = {
        // approximate: headers + selected index
        let staged_headers = if app.scm.staged.is_empty() { 0 } else { 1 };
        let changes_header = 1;
        staged_headers + changes_header + app.scm.selected
    };
    let start = sel_line
        .saturating_sub(list_h.saturating_sub(1) / 2)
        .min(lines.len().saturating_sub(list_h));
    let visible: Vec<Line> = lines.into_iter().skip(start).take(list_h).collect();
    f.render_widget(Paragraph::new(visible), chunks[3]);

    // ── Pretty GRAPH (lane-colored ● / │) ──
    let graph_focused = app.scm.focus == ScmFocus::Graph;
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(
                " ▾ GRAPH  {} commits",
                app.scm.graph.len()
            ),
            Style::default()
                .fg(if graph_focused {
                    app.theme.accent
                } else {
                    app.theme.line_no
                })
                .add_modifier(Modifier::BOLD),
        )),
        chunks[4],
    );
    let g_h = chunks[5].height as usize;
    let g_sel = app.scm.graph_selected;
    let g_start = g_sel
        .saturating_sub(g_h.saturating_sub(1) / 2)
        .min(app.scm.graph.len().saturating_sub(g_h));
    let text_budget = (width as usize).saturating_sub(4);
    let mut g_lines: Vec<Line> = Vec::new();
    for (abs_i, row) in app
        .scm
        .graph
        .iter()
        .enumerate()
        .skip(g_start)
        .take(g_h)
    {
        let selected = graph_focused && abs_i == g_sel;
        let bg = if selected {
            app.theme.panel_sel_bg
        } else {
            app.theme.completion_bg
        };
        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        // Graph strip with per-lane colors
        for g in &row.glyphs {
            let ch = g.ch().to_string();
            let fg = if let Some(id) = g.color_id() {
                let (r, gr, b) = xei_core::git_graph::lane_rgb(id);
                Color::Rgb(r, gr, b)
            } else {
                Color::Rgb(60, 60, 70)
            };
            let mut st = Style::default().fg(fg).bg(bg);
            if matches!(g, xei_core::git_graph::GraphGlyph::Node(_)) {
                st = st.add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(ch, st));
        }
        // gap after graph
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        // subject + optional ref badges
        let graph_w = row.glyphs.len() + 2;
        let remain = text_budget.saturating_sub(graph_w);
        let mut label = row.subject.clone();
        if !row.refs.is_empty() {
            // compact ref: take first ref token
            let first_ref = row
                .refs
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches("HEAD -> ")
                .trim_start_matches("tag: ");
            if !first_ref.is_empty() {
                label = format!("{}  ·{}", label, first_ref);
            }
        }
        let label = truncate_path(&label, remain.max(8));
        let lab_style = if selected {
            Style::default()
                .fg(app.theme.panel_sel_fg)
                .bg(bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.fg).bg(bg)
        };
        spans.push(Span::styled(label, lab_style));
        g_lines.push(Line::from(spans));
    }
    if g_lines.is_empty() {
        g_lines.push(Line::from(Span::styled(
            "  (no commits)",
            Style::default().fg(app.theme.line_no),
        )));
    }
    f.render_widget(Paragraph::new(g_lines), chunks[5]);

    // ── Detail (selected commit) + hints ──
    let detail_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(chunks[6]);
    if let Some(row) = app.scm.selected_graph_row() {
        let (r, gr, b) = xei_core::git_graph::lane_rgb(row.color);
        let detail = xei_core::git_graph::detail_line(row);
        f.render_widget(
            Paragraph::new(Span::styled(
                format!(" {}", truncate_path(&detail, (width as usize).saturating_sub(3))),
                Style::default().fg(Color::Rgb(r, gr, b)),
            )),
            detail_chunks[0],
        );
    } else {
        f.render_widget(Paragraph::new(""), detail_chunks[0]);
    }
    f.render_widget(
        Paragraph::new(Span::styled(
            " ⇧G/C-S-G full Git · Space stage · j/k · Enter · Esc",
            Style::default().fg(app.theme.line_no),
        )),
        detail_chunks[1],
    );
}

/// Unified Settings (Ctrl+,) — About first, then config pages.
fn draw_settings(f: &mut Frame, app: &mut App, area: Rect) {
    use xei_core::settings::{help_entries, SettingRow, SettingsPage};

    // Larger panel so Help shortcuts fit comfortably.
    let width = area.width.saturating_sub(2).min(78).max(52);
    let height = area.height.saturating_sub(1).min(28).max(16);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };

    // Soft shadow
    f.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        Rect {
            x: x.saturating_add(1),
            y: y.saturating_add(1),
            width,
            height: height.saturating_sub(1),
        },
    );

    let title = match app.settings.page {
        SettingsPage::About => " ABOUT ",
        SettingsPage::Setting => " SETTING ",
        SettingsPage::Pet => " PET ",
        SettingsPage::Help => " HELP ",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.mode_settings))
        .style(Style::default().bg(app.theme.completion_bg).fg(app.theme.fg))
        .title(Span::styled(
            title,
            Style::default()
                .fg(app.theme.mode_settings)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // nav
            Constraint::Min(6),
            Constraint::Length(1), // status
            Constraint::Length(1), // hints
        ])
        .split(inner);

    // Page tabs
    let mut nav: Vec<Span> = Vec::new();
    for p in SettingsPage::all() {
        let active = app.settings.page == *p;
        nav.push(Span::styled(
            format!(" {} ", p.label()),
            if active {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.mode_settings)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.line_no)
            },
        ));
        nav.push(Span::raw(" "));
    }
    if app.settings.dirty {
        nav.push(Span::styled(
            " •",
            Style::default().fg(Color::Rgb(255, 180, 100)),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(nav)), chunks[0]);

    let body = chunks[1];
    let mut lines: Vec<Line> = Vec::new();
    let w = width as usize;
    let body_h = body.height as usize;

    match app.settings.page {
        SettingsPage::About => {
            // Match empty-buffer welcome: centered shade-art + stacked lines.
            let accent = app.theme.mode_normal;
            let dim = app.theme.line_no;
            let about_rows: [(&str, Color, bool); 9] = [
                ("░ ▒ ▓ █  晴  █ ▓ ▒ ░", accent, true),
                ("", dim, false),
                ("x  e  i", app.theme.fg, true),
                (
                    concat!("v", env!("CARGO_PKG_VERSION"), "  ·  about"),
                    dim,
                    false,
                ),
                ("", dim, false),
                (
                    "i insert · Ctrl+P files · Ctrl+Shift+G git",
                    dim,
                    false,
                ),
                (
                    "Ctrl+G scm · Ctrl+Shift+V preview · Ctrl+, settings",
                    dim,
                    false,
                ),
                ("", dim, false),
                ("developed by stremtec", app.theme.muted, false),
            ];
            let pad_top = body_h.saturating_sub(about_rows.len() + 2) / 2;
            for _ in 0..pad_top {
                lines.push(Line::from(""));
            }
            for (text, color, bold) in about_rows {
                let mut style = Style::default().fg(color);
                if bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if text == "developed by stremtec" {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                lines.push(Line::from(Span::styled(
                    format!("{:^width$}", text, width = (w.saturating_sub(2)).max(8)),
                    style,
                )));
            }
            lines.push(Line::from(Span::styled(
                format!(
                    "{:^width$}",
                    "https://github.com/stremtec/xei",
                    width = (w.saturating_sub(2)).max(8)
                ),
                Style::default().fg(app.theme.muted),
            )));
            lines.push(Line::from(Span::styled(
                format!(
                    "{:^width$}",
                    "config · ~/.xei.toml  ·  Help for shortcuts",
                    width = (w.saturating_sub(2)).max(8)
                ),
                Style::default().fg(dim),
            )));
            let gpu_line = if app.gpu_acc {
                if app.term_caps_summary.is_empty() {
                    "gpu_acc on".to_string()
                } else {
                    format!("gpu_acc on · {}", app.term_caps_summary)
                }
            } else {
                "gpu_acc off · plain cell TUI".into()
            };
            lines.push(Line::from(Span::styled(
                format!(
                    "{:^width$}",
                    gpu_line,
                    width = (w.saturating_sub(2)).max(8)
                ),
                Style::default().fg(if app.gpu_active() {
                    app.theme.success
                } else {
                    dim
                }),
            )));
        }
        SettingsPage::Setting => {
            let rows = app.settings.setting_rows();
            let themes = xei_core::theme::all_themes();
            // Scroll so selection stays in view
            let view_h = body_h.max(1);
            let sel = app.settings.selected;
            let scroll = sel.saturating_sub(view_h.saturating_sub(2));
            for (i, row) in rows.iter().enumerate().skip(scroll).take(view_h) {
                let sel_here = i == sel;
                match row {
                    SettingRow::ThemeHeader => {
                        lines.push(Line::from(Span::styled(
                            "  Theme  (Enter apply · s save)",
                            Style::default().fg(app.theme.line_no),
                        )));
                    }
                    SettingRow::Theme(ti) => {
                        let t = themes.get(*ti);
                        let name = t.map(|t| t.name).unwrap_or("?");
                        let current = app.settings.draft.theme == name;
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        let mark = if current { "●" } else { " " };
                        lines.push(Line::from(Span::styled(
                            format!("  {mark} {name}"),
                            Style::default()
                                .fg(if sel_here {
                                    Color::Black
                                } else {
                                    app.theme.fg
                                })
                                .bg(bg),
                        )));
                    }
                    SettingRow::EditorHeader => {
                        lines.push(Line::from(Span::styled(
                            "  Editor  (Enter cycles / toggles)",
                            Style::default().fg(app.theme.line_no),
                        )));
                    }
                    SettingRow::TabWidth => {
                        let text = format!("  tab_width          {}", app.settings.draft.tab_width);
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        lines.push(Line::from(Span::styled(
                            text,
                            Style::default()
                                .fg(if sel_here {
                                    Color::Black
                                } else {
                                    app.theme.fg
                                })
                                .bg(bg),
                        )));
                    }
                    SettingRow::WrapLines => {
                        let on = if app.settings.draft.wrap_lines {
                            "wrap"
                        } else {
                            "scroll"
                        };
                        let text = format!("  long lines         {on}");
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        lines.push(Line::from(Span::styled(
                            text,
                            Style::default()
                                .fg(if sel_here {
                                    Color::Black
                                } else {
                                    app.theme.fg
                                })
                                .bg(bg),
                        )));
                    }
                    SettingRow::RelativeNumber => {
                        let on = if app.settings.draft.relative_number {
                            "on"
                        } else {
                            "off"
                        };
                        let text = format!("  relative_number    {on}");
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        lines.push(Line::from(Span::styled(
                            text,
                            Style::default()
                                .fg(if sel_here {
                                    Color::Black
                                } else {
                                    app.theme.fg
                                })
                                .bg(bg),
                        )));
                    }
                    SettingRow::ClipboardSync => {
                        let on = if app.settings.draft.clipboard_sync {
                            "on"
                        } else {
                            "off"
                        };
                        let text = format!("  clipboard_sync     {on}");
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        lines.push(Line::from(Span::styled(
                            text,
                            Style::default()
                                .fg(if sel_here {
                                    Color::Black
                                } else {
                                    app.theme.fg
                                })
                                .bg(bg),
                        )));
                    }
                    SettingRow::GpuAcc => {
                        let on = if app.settings.draft.gpu_acc {
                            "true"
                        } else {
                            "false"
                        };
                        let text = format!("  gpu_acc            {on}");
                        let hint = if app.settings.draft.gpu_acc {
                            "  · Ghostty/Kitty enhancements"
                        } else {
                            "  · plain cell TUI only"
                        };
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        let fg = if sel_here {
                            Color::Black
                        } else {
                            app.theme.fg
                        };
                        lines.push(Line::from(vec![
                            Span::styled(text, Style::default().fg(fg).bg(bg)),
                            Span::styled(
                                hint,
                                Style::default()
                                    .fg(if sel_here {
                                        Color::Black
                                    } else {
                                        app.theme.line_no
                                    })
                                    .bg(bg),
                            ),
                        ]));
                    }
                    SettingRow::KeyHints => {
                        let on = if app.settings.draft.key_hints {
                            "on"
                        } else {
                            "off"
                        };
                        let text = format!("  key_hints          {on}");
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        lines.push(Line::from(Span::styled(
                            text,
                            Style::default()
                                .fg(if sel_here {
                                    Color::Black
                                } else {
                                    app.theme.fg
                                })
                                .bg(bg),
                        )));
                    }
                    SettingRow::LspHeader => {
                        lines.push(Line::from(Span::styled(
                            "  LSP  (Enter cycles default ↔ off)",
                            Style::default().fg(app.theme.line_no),
                        )));
                    }
                    SettingRow::LspEnabled => {
                        let on = if app.settings.draft.lsp_enabled {
                            "on"
                        } else {
                            "off"
                        };
                        let text = format!("  lsp_enabled        {on}");
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        lines.push(Line::from(Span::styled(
                            text,
                            Style::default()
                                .fg(if sel_here {
                                    Color::Black
                                } else {
                                    app.theme.fg
                                })
                                .bg(bg),
                        )));
                    }
                    SettingRow::LspLang(i) => {
                        let catalog = xei_core::config::lsp_lang_catalog();
                        let (key, label, default_cmd) =
                            catalog.get(*i).copied().unwrap_or(("", "?", ""));
                        let (state, detail) = match app.settings.draft.lsp_servers.get(key) {
                            None => ("●", default_cmd),
                            Some(s) if s.is_empty() => ("○", "off"),
                            Some(s) => ("◆", s.as_str()),
                        };
                        let text = format!("  {state} {label:<12} {detail}");
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        let fg = if sel_here {
                            Color::Black
                        } else if detail == "off" {
                            app.theme.line_no
                        } else {
                            app.theme.fg
                        };
                        lines.push(Line::from(Span::styled(
                            text,
                            Style::default().fg(fg).bg(bg),
                        )));
                    }
                    SettingRow::GitHeader => {
                        lines.push(Line::from(Span::styled(
                            "  Git  (open panels)",
                            Style::default().fg(app.theme.line_no),
                        )));
                    }
                    SettingRow::OpenWorkbench => {
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        lines.push(Line::from(Span::styled(
                            "  Open Git workbench (Ctrl+Shift+G)",
                            Style::default()
                                .fg(if sel_here {
                                    Color::Black
                                } else {
                                    app.theme.fg
                                })
                                .bg(bg),
                        )));
                    }
                    SettingRow::OpenScm => {
                        let bg = if sel_here {
                            app.theme.completion_selected
                        } else {
                            app.theme.completion_bg
                        };
                        lines.push(Line::from(Span::styled(
                            "  Open light SCM (Ctrl+G)",
                            Style::default()
                                .fg(if sel_here {
                                    Color::Black
                                } else {
                                    app.theme.fg
                                })
                                .bg(bg),
                        )));
                    }
                    // Pet rows only appear on Pet page
                    SettingRow::PetEnabled
                    | SettingRow::PetPath
                    | SettingRow::PetX
                    | SettingRow::PetY
                    | SettingRow::PetWidth
                    | SettingRow::PetSpeed
                    | SettingRow::PetReload => {}
                }
            }
        }
        SettingsPage::Pet => {
            let rows = app.settings.pet_rows();
            let view_h = body_h.max(1);
            let sel = app.settings.selected;
            let scroll = sel.saturating_sub(view_h.saturating_sub(2));

            let gfx_ok = app.pet_graphics_ok();
            lines.push(Line::from(Span::styled(
                "  Desktop pet — looping GIF via Kitty graphics",
                Style::default().fg(app.theme.line_no),
            )));
            lines.push(Line::from(Span::styled(
                if gfx_ok {
                    "  GPU ready · place with x/y · h/l nudge · speed slider"
                } else {
                    "  ⚠ needs gpu_acc + Kitty/Ghostty (Setting tab)"
                },
                Style::default().fg(if gfx_ok {
                    app.theme.success
                } else {
                    app.theme.warning
                }),
            )));
            lines.push(Line::from(""));

            for (i, row) in rows.iter().enumerate().skip(scroll).take(view_h.saturating_sub(3)) {
                let sel_here = i == sel;
                let bg = if sel_here {
                    app.theme.completion_selected
                } else {
                    app.theme.completion_bg
                };
                let fg = if sel_here {
                    Color::Black
                } else {
                    app.theme.fg
                };
                match row {
                    SettingRow::PetEnabled => {
                        let on = if app.settings.draft.pet_enabled {
                            "on"
                        } else {
                            "off"
                        };
                        lines.push(Line::from(Span::styled(
                            format!("  enabled            {on}"),
                            Style::default().fg(fg).bg(bg),
                        )));
                    }
                    SettingRow::PetPath => {
                        let p = if app.settings.draft.pet_path.is_empty() {
                            "(none — :pet ~/pic.gif)".into()
                        } else {
                            app.settings.draft.pet_path.clone()
                        };
                        let short: String = p.chars().take(40).collect();
                        lines.push(Line::from(Span::styled(
                            format!("  path               {short}"),
                            Style::default().fg(fg).bg(bg),
                        )));
                    }
                    SettingRow::PetX => {
                        let (mx, _) = app.pet_pos_max();
                        lines.push(Line::from(Span::styled(
                            format!(
                                "  x (col)            {} / {}  (h/l)",
                                app.settings.draft.pet_x, mx
                            ),
                            Style::default().fg(fg).bg(bg),
                        )));
                    }
                    SettingRow::PetY => {
                        let (_, my) = app.pet_pos_max();
                        lines.push(Line::from(Span::styled(
                            format!(
                                "  y (row)            {} / {}  (h/l)",
                                app.settings.draft.pet_y, my
                            ),
                            Style::default().fg(fg).bg(bg),
                        )));
                    }
                    SettingRow::PetWidth => {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "  width_cells        {}  (h/l)",
                                app.settings.draft.pet_width_cells
                            ),
                            Style::default().fg(fg).bg(bg),
                        )));
                    }
                    SettingRow::PetSpeed => {
                        let sp = app.settings.draft.pet_speed;
                        let bar = xei_core::pet::PetState::speed_slider(sp, 12);
                        let label = xei_core::pet::PetState::speed_label(sp);
                        lines.push(Line::from(Span::styled(
                            format!("  speed  {bar}  {label}  ({sp}%)"),
                            Style::default().fg(fg).bg(bg),
                        )));
                    }
                    SettingRow::PetReload => {
                        lines.push(Line::from(Span::styled(
                            "  Reload GIF from path",
                            Style::default().fg(fg).bg(bg),
                        )));
                    }
                    _ => {}
                }
            }

            lines.push(Line::from(""));
            let live = if app.pet.enabled && app.pet.has_frames() && gfx_ok {
                let (sx, sy) = app.pet_screen_xy();
                format!(
                    "  live · {} frames · screen {sx},{sy} · cfg {},{} · w={} · {}",
                    app.pet.frame_count(),
                    app.pet.x,
                    app.pet.y,
                    app.pet.width_cells,
                    xei_core::pet::PetState::speed_label(app.pet.speed)
                )
            } else if let Some(ref e) = app.pet.load_error {
                format!("  error · {e}")
            } else if !gfx_ok {
                "  blocked · enable gpu_acc on a Kitty/Ghostty host".into()
            } else {
                "  idle · load a GIF with :pet path.gif".into()
            };
            lines.push(Line::from(Span::styled(
                live,
                Style::default().fg(if app.pet.enabled && gfx_ok {
                    app.theme.success
                } else {
                    app.theme.muted
                }),
            )));
        }
        SettingsPage::Help => {
            let entries = help_entries();
            let view_h = body_h.max(1);
            let sel = app.settings.selected.min(entries.len().saturating_sub(1));
            let scroll = sel.saturating_sub(view_h.saturating_sub(2));
            let key_col = 22usize;
            for (i, e) in entries.iter().enumerate().skip(scroll).take(view_h) {
                let sel_here = i == sel;
                if e.is_header {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", e.desc),
                        Style::default()
                            .fg(app.theme.mode_settings)
                            .add_modifier(Modifier::BOLD),
                    )));
                } else {
                    let bg = if sel_here {
                        app.theme.completion_selected
                    } else {
                        app.theme.completion_bg
                    };
                    let key_style = Style::default()
                        .fg(if sel_here {
                            Color::Black
                        } else {
                            app.theme.accent
                        })
                        .bg(bg)
                        .add_modifier(Modifier::BOLD);
                    let desc_style = Style::default()
                        .fg(if sel_here {
                            Color::Black
                        } else {
                            app.theme.fg
                        })
                        .bg(bg);
                    let key_pad = format!("{:<w$}", e.keys, w = key_col);
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {key_pad}"), key_style),
                        Span::styled(format!("  {}", e.desc), desc_style),
                    ]));
                }
            }
        }
    }

    let _ = w;
    f.render_widget(Paragraph::new(lines), body);

    let st = app.settings.status.clone().unwrap_or_default();
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" {st}"),
            Style::default().fg(app.theme.mode_settings),
        )),
        chunks[2],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            " Tab pages · 1/2/3/4 · j/k · Enter · s save · Esc close",
            Style::default().fg(app.theme.line_no),
        )),
        chunks[3],
    );
}

/// JetBrains-style Git workbench (Ctrl+Shift+G).
///
/// Docked into the **editor pane** (same z-layer as the buffer — not a
/// floating modal). Three columns: Changes | Log+graph | Commit files.
fn draw_git_workbench(f: &mut Frame, app: &mut App, area: Rect) {
    use xei_core::git_workbench::GitTab;

    // Center log always needs history (lazy).
    app.git_wb.ensure_history();
    app.git_wb.ensure_tab_data();

    // Fill the entire given area (already the main editor rect) — no margin,
    // no shadow, no elevated popup. Flat IDE panel.
    let panel = area;
    let bg = app.theme.editor_bg;
    f.render_widget(Block::default().style(Style::default().bg(bg)), panel);

    let branch = if app.git_wb.branch.is_empty() {
        "HEAD".to_string()
    } else {
        let mut b = app.git_wb.branch.clone();
        if app.git_wb.ahead > 0 || app.git_wb.behind > 0 {
            b.push_str(&format!(" ↑{}↓{}", app.git_wb.ahead, app.git_wb.behind));
        }
        b
    };

    // Vertical: toolbar | body | status
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // toolbar
            Constraint::Min(6),    // 3-col body
            Constraint::Length(1), // status
        ])
        .split(panel);

    // ── Toolbar (flat, no rounded chrome) ──
    let auth = match app.git_wb.auth.state {
        xei_core::GhAuthState::LoggedIn => format!("gh:{}", app.git_wb.auth.user),
        xei_core::GhAuthState::LoggedOut => "gh:out".into(),
        xei_core::GhAuthState::NotInstalled => "gh:—".into(),
    };
    let tab = app.git_wb.tab;
    let loading_spin = app.git_wb.is_loading().then(|| {
        format!(
            " {} {} ",
            app.git_wb.spinner_frame(),
            app.git_wb.loading_label().unwrap_or("Loading…")
        )
    });
    let tab_chip = |name: &str, on: bool| {
        if on {
            Span::styled(
                format!(" {name} "),
                Style::default()
                    .fg(app.theme.accent_fg)
                    .bg(app.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(format!(" {name} "), Style::default().fg(app.theme.muted).bg(bg))
        }
    };
    // Docked 3-col: highlight by active column. Special surfaces: by tab.
    use xei_core::git_workbench::GitPane;
    let docked = matches!(tab, GitTab::Status | GitTab::History | GitTab::Commit);
    let on_status = docked && app.git_wb.pane == GitPane::Changes;
    let on_log = docked && app.git_wb.pane == GitPane::Log;
    let on_files = docked && app.git_wb.pane == GitPane::Files;
    let chips: [(&str, bool, u8); 9] = [
        ("1 Status", on_status, 1),
        ("2 Log", on_log, 2),
        ("3 Branches", tab == GitTab::Branches, 3),
        ("4 Files", on_files, 4),
        ("5 Diff", tab == GitTab::Diff, 5),
        ("6 PRs", tab == GitTab::PullRequests, 6),
        ("7 Issues", tab == GitTab::Issues, 7),
        ("8 Auth", tab == GitTab::Auth, 8),
        ("9 Stash", tab == GitTab::Stash, 9),
    ];
    let mut tb = vec![
        Span::styled(
            format!("  {branch} "),
            Style::default()
                .fg(app.theme.accent)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│", Style::default().fg(app.theme.border).bg(bg)),
    ];
    // Hit regions for mouse clicks on toolbar chips
    app.git_tab_hits.clear();
    let mut chip_x = v[0].x
        + unicode_width::UnicodeWidthStr::width(tb[0].content.as_ref()) as u16
        + unicode_width::UnicodeWidthStr::width(tb[1].content.as_ref()) as u16;
    for (name, on, key) in chips {
        let label = format!(" {name} ");
        let wchip = unicode_width::UnicodeWidthStr::width(label.as_str()) as u16;
        app.git_tab_hits
            .push((chip_x, v[0].y, wchip, 1, key));
        chip_x = chip_x.saturating_add(wchip);
        tb.push(tab_chip(name, on));
    }
    tb.push(Span::styled(
        format!("  f↑p↓u  ·  {auth}  ·  Tab panes  Esc "),
        Style::default().fg(app.theme.muted).bg(bg),
    ));
    if let Some(ref spin) = loading_spin {
        tb.push(Span::styled(
            spin.clone(),
            Style::default()
                .fg(app.theme.accent)
                .bg(app.theme.panel_bg)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if tab == GitTab::Diff {
        tb.push(Span::styled(
            " DIFF ",
            Style::default()
                .fg(app.theme.accent_fg)
                .bg(app.theme.warning)
                .add_modifier(Modifier::BOLD),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(tb)), v[0]);

    // Special full-width modes (PRs / Issues / Auth / Diff / Branches / Stash)
    let special = matches!(
        tab,
        GitTab::PullRequests
            | GitTab::Issues
            | GitTab::Auth
            | GitTab::Diff
            | GitTab::Branches
            | GitTab::Stash
    );

    if special {
        if app.git_wb.is_loading() {
            draw_git_wb_loading(f, app, v[1]);
        } else {
            draw_git_wb_special(f, app, v[1], tab);
        }
    } else {
        // ── JetBrains 3-column dock ──
        // Left ~26% changes, Center log, Right ~22% files
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(26),
                Constraint::Percentage(52),
                Constraint::Percentage(22),
            ])
            .split(v[1]);

        app.git_pane_hits.clear();
        app.git_pane_hits.push((
            cols[0].x,
            cols[0].y,
            cols[0].width,
            cols[0].height,
            0, // Changes
        ));
        app.git_pane_hits.push((
            cols[1].x,
            cols[1].y,
            cols[1].width,
            cols[1].height,
            1, // Log
        ));
        app.git_pane_hits.push((
            cols[2].x,
            cols[2].y,
            cols[2].width,
            cols[2].height,
            2, // Files
        ));
        draw_git_wb_changes(f, app, cols[0]);
        draw_git_wb_log(f, app, cols[1]);
        draw_git_wb_files(f, app, cols[2]);
    }

    // Status bar
    let msg = if app.git_wb.is_loading() {
        format!(
            "{} {}",
            app.git_wb.spinner_frame(),
            app.git_wb.loading_label().unwrap_or("Loading…")
        )
    } else {
        app.git_wb
            .message
            .clone()
            .or_else(|| app.git_wb.error.clone())
            .unwrap_or_else(|| {
                "Space stage · c commit · Enter open · RMB commit menu · Tab pane · Esc".into()
            })
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" {msg}"),
            Style::default()
                .fg(if app.git_wb.is_loading() {
                    app.theme.accent
                } else {
                    app.theme.muted
                })
                .bg(bg),
        )),
        v[2],
    );

    // Commit context menu (right-click)
    if app.git_wb.ctx_menu.is_some() {
        draw_git_ctx_menu(f, app, panel);
    }
}

/// Loading screen matching the empty-buffer welcome: shade blocks around 晴 / xei
/// that pulse, plus a rotating tip underneath.
fn draw_git_wb_loading(f: &mut Frame, app: &App, area: Rect) {
    let bg = app.theme.editor_bg;
    f.render_widget(Block::default().style(Style::default().bg(bg)), area);

    let tick = app.git_wb.loading_tick();
    let accent = app.theme.mode_normal;
    let label = app.git_wb.loading_label().unwrap_or("syncing");

    // Animate block density around the logo (same family as welcome / About).
    let shades = ['░', '▒', '▓', '█'];
    let phase = tick % 8;
    let mut left = String::new();
    for i in 0..4 {
        if i > 0 {
            left.push(' ');
        }
        left.push(shades[(phase + i) % 4]);
    }
    let mut right = String::new();
    for i in (0..4).rev() {
        if !right.is_empty() {
            right.push(' ');
        }
        right.push(shades[(phase + i) % 4]);
    }
    let logo_line = format!("{left}  晴  {right}");
    let status = format!("{}  {label}", app.git_wb.spinner_frame());
    let tip = app.git_wb.loading_tip();

    // (text, color, bold, italic)
    let rows: Vec<(String, Color, bool, bool)> = vec![
        (logo_line, accent, true, false),
        (String::new(), app.theme.line_no, false, false),
        ("x  e  i".into(), app.theme.fg, true, false),
        (status, app.theme.accent, false, false),
        (String::new(), app.theme.muted, false, false),
        (tip.into(), app.theme.muted, false, true),
    ];

    let y0 = area.y + (area.height / 2).saturating_sub(rows.len() as u16 / 2 + 1);
    for (i, (text, color, bold, italic)) in rows.iter().enumerate() {
        let y = y0 + i as u16;
        if y >= area.y + area.height {
            break;
        }
        let mut style = Style::default().fg(*color).bg(bg);
        if *bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if *italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text.clone(), style)))
                .alignment(Alignment::Center),
            Rect::new(area.x, y, area.width, 1),
        );
    }
}

/// Editor-like unified diff: dual line numbers + sign + content, tinted rows.
fn draw_git_diff_lines(lines: &mut Vec<Line>, app: &App, list_h: usize, w: usize) {
    use xei_core::git_ops::DiffLineKind;
    let th = app.theme;
    let path = app.git_wb.diff_path.clone().unwrap_or_default();
    let staged = if app.git_wb.diff_staged { "staged" } else { "worktree" };
    lines.push(Line::from(Span::styled(
        format!(" Diff · {path}  ·  {staged}  ·  Esc back  ·  j/k scroll"),
        Style::default().fg(th.muted),
    )));
    // Column header (old | new | ·)
    lines.push(Line::from(vec![
        Span::styled("  old ", Style::default().fg(th.muted)),
        Span::styled("  new ", Style::default().fg(th.muted)),
        Span::styled(" │ ", Style::default().fg(th.border)),
        Span::styled("code", Style::default().fg(th.muted)),
    ]));

    let gutter_w = 6usize; // " 1234 " style each side
    let start = app.git_wb.diff_scroll;
    let body_h = list_h.saturating_sub(2);
    let content_w = w.saturating_sub(gutter_w * 2 + 4).max(8);

    for dl in app
        .git_wb
        .diff_lines
        .iter()
        .skip(start)
        .take(body_h)
    {
        match dl.kind {
            DiffLineKind::Header | DiffLineKind::Meta => {
                lines.push(Line::from(Span::styled(
                    format!(" {}", truncate_path(&dl.text, w.saturating_sub(2))),
                    Style::default().fg(th.muted),
                )));
            }
            DiffLineKind::Hunk => {
                lines.push(Line::from(Span::styled(
                    format!(" {}", truncate_path(&dl.text, w.saturating_sub(2))),
                    Style::default()
                        .fg(th.git_hunk)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            DiffLineKind::Add | DiffLineKind::Del | DiffLineKind::Context => {
                let (sign, fg, row_bg) = match dl.kind {
                    DiffLineKind::Add => ('+', th.success, th.git_add_bg),
                    DiffLineKind::Del => ('-', th.error, th.git_del_bg),
                    _ => (' ', th.fg, th.editor_bg),
                };
                let old_s = dl
                    .old_no
                    .map(|n| format!("{n:>4} "))
                    .unwrap_or_else(|| "     ".into());
                let new_s = dl
                    .new_no
                    .map(|n| format!("{n:>4} "))
                    .unwrap_or_else(|| "     ".into());
                let content = truncate_path(dl.content(), content_w);
                lines.push(Line::from(vec![
                    Span::styled(old_s, Style::default().fg(th.muted).bg(row_bg)),
                    Span::styled(new_s, Style::default().fg(th.muted).bg(row_bg)),
                    Span::styled(
                        format!("│{sign}"),
                        Style::default().fg(fg).bg(row_bg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(content, Style::default().fg(fg).bg(row_bg)),
                ]));
            }
        }
    }
    if app.git_wb.diff_lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no diff)",
            Style::default().fg(th.muted),
        )));
    }
}

fn draw_editor_ctx_menu(f: &mut Frame, app: &App, area: Rect) {
    let Some(menu) = app.editor_ctx.as_ref() else {
        return;
    };
    let w = 32u16;
    let h = (menu.items.len() as u16).saturating_add(2).max(3);
    let x = menu.x.min(area.x + area.width.saturating_sub(w + 1));
    let y = menu.y.min(area.y + area.height.saturating_sub(h + 1));
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let th = app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th.panel_border))
        .style(Style::default().bg(th.panel_bg))
        .title(Span::styled(
            " Edit ",
            Style::default()
                .fg(th.accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);
    let mut lines: Vec<Line> = Vec::new();
    for (i, item) in menu.items.iter().enumerate() {
        let sel = i == menu.sel;
        let bgc = if sel { th.panel_sel_bg } else { th.panel_bg };
        let label = format!(" {:<18} {:>6} ", item.label(), item.key_hint());
        lines.push(Line::from(Span::styled(
            label,
            Style::default()
                .fg(if sel { th.panel_sel_fg } else { th.fg })
                .bg(bgc)
                .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_git_ctx_menu(f: &mut Frame, app: &App, area: Rect) {
    let Some(menu) = app.git_wb.ctx_menu.as_ref() else {
        return;
    };
    let w = 28u16;
    let h = (menu.items.len() as u16).saturating_add(2).max(3);
    let x = menu.x.min(area.x + area.width.saturating_sub(w + 1));
    let y = menu.y.min(area.y + area.height.saturating_sub(h + 1));
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let th = app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th.panel_border))
        .style(Style::default().bg(th.panel_bg))
        .title(Span::styled(
            " Commit ",
            Style::default()
                .fg(th.accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);
    let mut lines: Vec<Line> = Vec::new();
    for (i, item) in menu.items.iter().enumerate() {
        let sel = i == menu.sel;
        let bgc = if sel { th.panel_sel_bg } else { th.panel_bg };
        let label = format!(" {:<18} {:>5} ", item.label(), item.key_hint());
        lines.push(Line::from(Span::styled(
            label,
            Style::default()
                .fg(if sel { th.panel_sel_fg } else { th.fg })
                .bg(bgc)
                .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn git_pane_border(active: bool, theme: &xei_core::theme::Theme) -> Style {
    if active {
        Style::default().fg(theme.accent)
    } else {
        Style::default().fg(theme.border)
    }
}

/// Left: Changes + commit message (JetBrains Source Control).
fn draw_git_wb_changes(f: &mut Frame, app: &mut App, area: Rect) {
    use xei_core::git_workbench::GitPane;
    let bg = app.theme.editor_bg;
    let active = app.git_wb.pane == GitPane::Changes;
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(git_pane_border(active, app.theme))
        .style(Style::default().bg(bg))
        .title(Span::styled(
            if active { " ▸ Changes " } else { " Changes " },
            Style::default()
                .fg(if active {
                    app.theme.accent
                } else {
                    app.theme.line_no
                })
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(4),    // file lists
            Constraint::Length(5), // commit box
        ])
        .split(inner);

    let mut lines: Vec<Line> = Vec::new();
    let list_h = parts[0].height as usize;
    let mut flat = 0usize;
    let w = parts[0].width as usize;

    let n_ch = app.git_wb.changes.len() + app.git_wb.staged.len();
    lines.push(Line::from(Span::styled(
        format!(" ▾ Changes  {n_ch}"),
        Style::default()
            .fg(app.theme.line_no)
            .add_modifier(Modifier::BOLD),
    )));

    if !app.git_wb.staged.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  Staged ({})", app.git_wb.staged.len()),
            Style::default().fg(app.theme.success),
        )));
        for e in &app.git_wb.staged {
            let sel = active && app.git_wb.selected == flat;
            let bgc = if sel {
                app.theme.panel_sel_bg
            } else {
                bg
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", e.status.letter()),
                    Style::default().fg(status_color(e.status, app.theme)).bg(bgc),
                ),
                Span::styled(
                    truncate_path(&e.path, w.saturating_sub(6)),
                    Style::default()
                        .fg(if sel { app.theme.panel_sel_fg } else { app.theme.fg })
                        .bg(bgc),
                ),
            ]));
            flat += 1;
        }
    }

    lines.push(Line::from(Span::styled(
        format!("  Local Changes ({})", app.git_wb.changes.len()),
        Style::default().fg(app.theme.line_no),
    )));
    if app.git_wb.changes.is_empty() && app.git_wb.staged.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (clean)",
            Style::default().fg(app.theme.line_no),
        )));
    }
    for e in &app.git_wb.changes {
        let sel = active && app.git_wb.selected == flat;
        let bgc = if sel {
            app.theme.panel_sel_bg
        } else {
            bg
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", e.status.letter()),
                Style::default()
                    .fg(status_color(e.status, app.theme))
                    .bg(bgc)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                truncate_path(&e.path, w.saturating_sub(6)),
                Style::default()
                    .fg(if sel {
                        app.theme.panel_sel_fg
                    } else {
                        app.theme.fg
                    })
                    .bg(bgc),
            ),
        ]));
        flat += 1;
    }

    let start = app
        .git_wb
        .selected
        .saturating_sub(list_h.saturating_sub(4) / 2)
        .min(lines.len().saturating_sub(list_h));
    let vis: Vec<Line> = lines.into_iter().skip(start).take(list_h).collect();
    f.render_widget(Paragraph::new(vis), parts[0]);

    // Commit message box
    let cblock = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(app.theme.border))
        .style(Style::default().bg(bg))
        .title(Span::styled(
            " Commit ",
            Style::default().fg(app.theme.line_no),
        ));
    let cinner = cblock.inner(parts[1]);
    f.render_widget(cblock, parts[1]);
    let msg = if app.git_wb.commit_buf.is_empty() && !app.git_wb.commit_editing {
        "i = edit message · c = commit".to_string()
    } else if app.git_wb.commit_editing {
        format!("{}▌", app.git_wb.commit_buf)
    } else {
        app.git_wb.commit_buf.clone()
    };
    let msg_style = if app.git_wb.commit_buf.is_empty() && !app.git_wb.commit_editing {
        Style::default().fg(app.theme.muted)
    } else {
        Style::default().fg(app.theme.fg)
    };
    let box_bg = if app.git_wb.commit_editing {
        app.theme.panel_bg
    } else {
        bg
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(format!(" {msg}"), msg_style.bg(box_bg))),
            Line::from(Span::styled(
                " [c] commit  Space stage  a all  Tab panes",
                Style::default().fg(app.theme.muted),
            )),
        ]),
        cinner,
    );
}

/// Center: commit log + graph (JetBrains Git Log).
fn draw_git_wb_log(f: &mut Frame, app: &mut App, area: Rect) {
    use xei_core::git_workbench::{GitPane, HistoryView};
    let bg = app.theme.editor_bg;
    let active = app.git_wb.pane == GitPane::Log;
    let mode = match app.git_wb.history_view {
        HistoryView::List => "list",
        HistoryView::Graph => "graph",
    };
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(git_pane_border(active, app.theme))
        .style(Style::default().bg(bg))
        .title(Span::styled(
            format!(
                "{} Log · {mode} · {} ",
                if active { "▸" } else { " " },
                app.git_wb.commits.len()
            ),
            Style::default()
                .fg(if active {
                    app.theme.accent
                } else {
                    app.theme.line_no
                })
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let list_h = inner.height as usize;
    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    let avail = list_h;
    app.git_log_hits.clear();

    match app.git_wb.history_view {
        HistoryView::List => {
            let n = app.git_wb.commits.len();
            let start = app
                .git_wb
                .history_sel
                .saturating_sub(avail.saturating_sub(1) / 2)
                .min(n.saturating_sub(avail));
            if n == 0 {
                lines.push(Line::from(Span::styled(
                    "  No commits",
                    Style::default().fg(app.theme.line_no),
                )));
            }
            let mut row_y = inner.y;
            for (i, c) in app.git_wb.commits.iter().enumerate().skip(start).take(avail) {
                let sel = active && i == app.git_wb.history_sel;
                let bgc = if sel {
                    app.theme.panel_sel_bg
                } else {
                    bg
                };
                let sub = truncate_path(&c.subject, w.saturating_sub(32));
                lines.push(Line::from(vec![
                    Span::styled(
                        " ● ",
                        Style::default()
                            .fg(if sel {
                                app.theme.accent
                            } else {
                                app.theme.accent
                            })
                            .bg(bgc),
                    ),
                    Span::styled(
                        format!("{sub}  "),
                        Style::default()
                            .fg(if sel {
                                app.theme.panel_sel_fg
                            } else {
                                app.theme.fg
                            })
                            .bg(bgc)
                            .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                    ),
                    Span::styled(
                        format!("{} · {}", c.author, c.when),
                        Style::default()
                            .fg(if sel {
                                app.theme.muted
                            } else {
                                app.theme.line_no
                            })
                            .bg(bgc),
                    ),
                ]));
                app.git_log_hits
                    .push((inner.x, row_y, inner.width, 1, i));
                row_y = row_y.saturating_add(1);
            }
        }
        HistoryView::Graph => {
            let n = app.git_wb.history_graph.len();
            let start = app
                .git_wb
                .history_sel
                .saturating_sub(avail.saturating_sub(1) / 2)
                .min(n.saturating_sub(avail));
            if n == 0 {
                lines.push(Line::from(Span::styled(
                    "  No graph",
                    Style::default().fg(app.theme.line_no),
                )));
            }
            let mut row_y = inner.y;
            for (i, row) in app
                .git_wb
                .history_graph
                .iter()
                .enumerate()
                .skip(start)
                .take(avail)
            {
                let sel = active && i == app.git_wb.history_sel;
                let bgc = if sel {
                    app.theme.panel_sel_bg
                } else {
                    bg
                };
                let mut spans: Vec<Span> = vec![Span::styled(" ", Style::default().bg(bgc))];
                for g in &row.glyphs {
                    let ch = g.ch().to_string();
                    let fg = if let Some(id) = g.color_id() {
                        let (r, gr, b) = xei_core::git_graph::lane_rgb(id);
                        Color::Rgb(r, gr, b)
                    } else {
                        Color::Rgb(55, 60, 70)
                    };
                    let mut st = Style::default().fg(fg).bg(bgc);
                    if matches!(g, xei_core::git_graph::GraphGlyph::Node(_)) {
                        st = st.add_modifier(Modifier::BOLD);
                    }
                    spans.push(Span::styled(ch, st));
                }
                spans.push(Span::styled(" ", Style::default().bg(bgc)));
                let graph_w = row.glyphs.len() + 2;
                let remain = w.saturating_sub(graph_w + 2);
                let mut label = row.subject.clone();
                if !row.refs.is_empty() {
                    let first = row
                        .refs
                        .split(',')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_start_matches("HEAD -> ");
                    if !first.is_empty() {
                        label = format!("{label}  [{first}]");
                    }
                }
                let label = truncate_path(&label, remain.max(8));
                spans.push(Span::styled(
                    label,
                    Style::default()
                        .fg(if sel {
                            app.theme.panel_sel_fg
                        } else {
                            app.theme.fg
                        })
                        .bg(bgc)
                        .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                ));
                lines.push(Line::from(spans));
                app.git_log_hits
                    .push((inner.x, row_y, inner.width, 1, i));
                row_y = row_y.saturating_add(1);
            }
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Right: files for selected commit (JetBrains detail).
fn draw_git_wb_files(f: &mut Frame, app: &mut App, area: Rect) {
    use xei_core::git_workbench::GitPane;
    let bg = app.theme.editor_bg;
    let active = app.git_wb.pane == GitPane::Files;
    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(bg))
        .title(Span::styled(
            if active { " ▸ Files " } else { " Files " },
            Style::default()
                .fg(if active {
                    app.theme.accent
                } else {
                    app.theme.line_no
                })
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let list_h = inner.height as usize;
    let w = inner.width as usize;

    if let Some(ref d) = app.git_wb.commit_detail {
        lines.push(Line::from(Span::styled(
            format!(" {}", truncate_path(&d.subject, w.saturating_sub(2))),
            Style::default()
                .fg(Color::Rgb(220, 230, 245))
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!(" {} · {}", d.short, d.date),
            Style::default().fg(app.theme.accent),
        )));
        lines.push(Line::from(Span::styled(
            format!(" +{} −{} · {} files", d.insertions, d.deletions, d.files.len()),
            Style::default().fg(app.theme.line_no),
        )));
        lines.push(Line::from(Span::styled(
            " ─────────────",
            Style::default().fg(app.theme.border),
        )));
        let header = lines.len();
        let file_h = list_h.saturating_sub(header);
        let start = app
            .git_wb
            .commit_file_sel
            .saturating_sub(file_h.saturating_sub(1) / 2)
            .min(d.files.len().saturating_sub(file_h));
        for (i, file) in d.files.iter().enumerate().skip(start).take(file_h) {
            let sel = active && i == app.git_wb.commit_file_sel;
            let bgc = if sel {
                app.theme.panel_sel_bg
            } else {
                bg
            };
            lines.push(Line::from(Span::styled(
                format!(
                    " {} {}",
                    file.status,
                    truncate_path(&file.path, w.saturating_sub(4))
                ),
                Style::default()
                    .fg(if sel {
                        app.theme.panel_sel_fg
                    } else {
                        app.theme.fg
                    })
                    .bg(bgc),
            )));
        }
    } else {
        lines.push(Line::from(Span::styled(
            " Select a commit",
            Style::default().fg(app.theme.line_no),
        )));
        lines.push(Line::from(Span::styled(
            " in the Log pane",
            Style::default().fg(app.theme.line_no),
        )));
        lines.push(Line::from(Span::styled(
            " (Enter / j k)",
            Style::default().fg(Color::Rgb(70, 80, 90)),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Full-width modes: Diff / Branches / PRs / Issues / Auth.
fn draw_git_wb_special(f: &mut Frame, app: &mut App, area: Rect, tab: xei_core::git_workbench::GitTab) {
    use xei_core::git_workbench::GitTab;
    let bg = app.theme.editor_bg;
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(app.theme.border))
        .style(Style::default().bg(bg));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let list_h = inner.height as usize;
    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    match tab {
        GitTab::Branches => {
            lines.push(Line::from(Span::styled(
                " Branches  ·  Enter checkout  ·  c new  ·  d delete",
                Style::default().fg(app.theme.line_no),
            )));
            let avail = list_h.saturating_sub(1);
            let start = app
                .git_wb
                .branch_sel
                .saturating_sub(avail.saturating_sub(1) / 2)
                .min(app.git_wb.branches.len().saturating_sub(avail));
            for (i, b) in app
                .git_wb
                .branches
                .iter()
                .enumerate()
                .skip(start)
                .take(avail)
            {
                let sel = i == app.git_wb.branch_sel;
                let bgc = if sel {
                    app.theme.panel_sel_bg
                } else {
                    bg
                };
                let mark = if b.current {
                    "●"
                } else if b.remote {
                    "↪"
                } else {
                    " "
                };
                let fg = if sel {
                    app.theme.panel_sel_fg
                } else if b.current {
                    app.theme.success
                } else {
                    app.theme.fg
                };
                lines.push(Line::from(Span::styled(
                    format!(" {mark} {} ", b.name),
                    Style::default().fg(fg).bg(bgc),
                )));
            }
        }
        GitTab::Diff => {
            draw_git_diff_lines(&mut lines, app, list_h, w);
        }
        GitTab::PullRequests => {
            lines.push(Line::from(Span::styled(
                format!(
                    " PRs · {}  ·  Enter review  ·  c checkout  ·  [ / ] state  ·  / filter",
                    app.git_wb.pr_state.label()
                ),
                Style::default().fg(app.theme.line_no),
            )));
            let idxs = if app.git_wb.pr_filtered.is_empty() && app.git_wb.pr_filter.is_empty() {
                (0..app.git_wb.prs.len()).collect::<Vec<_>>()
            } else {
                app.git_wb.pr_filtered.clone()
            };
            let start = app
                .git_wb
                .pr_sel
                .saturating_sub(list_h.saturating_sub(2) / 2)
                .min(idxs.len().saturating_sub(list_h.saturating_sub(1)));
            for (vi, &pi) in idxs.iter().enumerate().skip(start).take(list_h.saturating_sub(1)) {
                let Some(pr) = app.git_wb.prs.get(pi) else {
                    continue;
                };
                // pr_sel is a visual index into the (possibly filtered) list.
                let sel = vi == app.git_wb.pr_sel;
                let bgc = if sel {
                    app.theme.panel_sel_bg
                } else {
                    bg
                };
                lines.push(Line::from(Span::styled(
                    format!(
                        " #{} {}  ·{}",
                        pr.number,
                        truncate_path(&pr.title, w.saturating_sub(16)),
                        pr.author
                    ),
                    Style::default()
                        .fg(if sel {
                            app.theme.panel_sel_fg
                        } else {
                            app.theme.fg
                        })
                        .bg(bgc),
                )));
            }
            if idxs.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  No pull requests",
                    Style::default().fg(app.theme.line_no),
                )));
            }
        }
        GitTab::Issues => {
            lines.push(Line::from(Span::styled(
                format!(" Issues · {}  ·  s state  ·  / filter", app.git_wb.issue_state.label()),
                Style::default().fg(app.theme.line_no),
            )));
            let idxs = if app.git_wb.issue_filtered.is_empty() && app.git_wb.issue_filter.is_empty()
            {
                (0..app.git_wb.issues.len()).collect::<Vec<_>>()
            } else {
                app.git_wb.issue_filtered.clone()
            };
            let start = app
                .git_wb
                .issue_sel
                .saturating_sub(list_h.saturating_sub(2) / 2)
                .min(idxs.len().saturating_sub(list_h.saturating_sub(1)));
            for (vi, &ii) in idxs.iter().enumerate().skip(start).take(list_h.saturating_sub(1)) {
                let Some(it) = app.git_wb.issues.get(ii) else {
                    continue;
                };
                // issue_sel is a visual index into the (possibly filtered) list.
                let sel = vi == app.git_wb.issue_sel;
                let bgc = if sel {
                    app.theme.panel_sel_bg
                } else {
                    bg
                };
                lines.push(Line::from(Span::styled(
                    format!(
                        " #{} {}",
                        it.number,
                        truncate_path(&it.title, w.saturating_sub(10))
                    ),
                    Style::default()
                        .fg(if sel {
                            app.theme.panel_sel_fg
                        } else {
                            app.theme.fg
                        })
                        .bg(bgc),
                )));
            }
            if idxs.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  No issues",
                    Style::default().fg(app.theme.line_no),
                )));
            }
        }
        GitTab::Auth => {
            draw_git_auth_tab(&mut lines, app, list_h, w);
        }
        GitTab::Stash => {
            draw_git_stash_tab(&mut lines, app, list_h);
        }
        _ => {}
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_git_stash_tab(lines: &mut Vec<Line>, app: &App, list_h: usize) {
    lines.push(Line::from(Span::styled(
        " Stash  ·  Enter apply  ·  d drop  ·  z push  ·  Z pop latest  ·  p preview",
        Style::default().fg(app.theme.line_no),
    )));
    lines.push(Line::from(""));
    if app.git_wb.stashes.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (empty — z to stash working tree)",
            Style::default().fg(app.theme.muted).add_modifier(Modifier::ITALIC),
        )));
        return;
    }
    let avail = list_h.saturating_sub(3);
    let start = app
        .git_wb
        .stash_sel
        .saturating_sub(avail.saturating_sub(1) / 2)
        .min(app.git_wb.stashes.len().saturating_sub(avail));
    for (i, s) in app
        .git_wb
        .stashes
        .iter()
        .enumerate()
        .skip(start)
        .take(avail)
    {
        let sel = i == app.git_wb.stash_sel;
        let bg = if sel {
            app.theme.panel_sel_bg
        } else {
            app.theme.editor_bg
        };
        let fg = if sel {
            app.theme.panel_sel_fg
        } else {
            app.theme.fg
        };
        let mark = if sel { "▸" } else { " " };
        lines.push(Line::from(Span::styled(
            format!(" {mark} {s}"),
            Style::default()
                .fg(fg)
                .bg(bg)
                .add_modifier(if sel {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        )));
    }
}

/// Auth tab — status card + device-code login + action list.
fn draw_git_auth_tab(lines: &mut Vec<Line>, app: &App, list_h: usize, w: usize) {
    use xei_core::GhAuthState;

    let auth = &app.git_wb.auth;
    let (badge, badge_fg, badge_bg) = match auth.state {
        GhAuthState::LoggedIn => (" SIGNED IN ", app.theme.accent_fg, app.theme.success),
        GhAuthState::LoggedOut => (" SIGNED OUT ", Color::Black, Color::Rgb(240, 180, 100)),
        GhAuthState::NotInstalled => (" NO GH CLI ", app.theme.accent_fg, app.theme.error),
    };

    lines.push(Line::from(Span::styled(
        " GitHub Authentication",
        Style::default()
            .fg(app.theme.fg)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Status badge + summary
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            badge,
            Style::default()
                .fg(badge_fg)
                .bg(badge_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", auth.detail),
            Style::default().fg(app.theme.fg),
        ),
    ]));

    match auth.state {
        GhAuthState::LoggedIn => {
            lines.push(Line::from(Span::styled(
                format!(
                    "  @{}  ·  {}  ·  git:{}",
                    auth.user,
                    auth.host,
                    if auth.protocol.is_empty() {
                        "—"
                    } else {
                        auth.protocol.as_str()
                    }
                ),
                Style::default().fg(app.theme.line_no),
            )));
            if !auth.scopes.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("  scopes  {}", auth.scopes),
                    Style::default().fg(app.theme.muted),
                )));
            }
            if !auth.token_source.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("  token   {}", auth.token_source),
                    Style::default().fg(app.theme.muted),
                )));
            }
        }
        GhAuthState::LoggedOut => {
            lines.push(Line::from(Span::styled(
                "  Sign in to use Pull Requests, Issues, and remotes.",
                Style::default().fg(app.theme.line_no),
            )));
        }
        GhAuthState::NotInstalled => {
            lines.push(Line::from(Span::styled(
                "  Install:  brew install gh   ·   https://cli.github.com",
                Style::default().fg(app.theme.line_no),
            )));
        }
    }

    // Live browser-login panel
    if let Some(ref session) = app.git_wb.auth_login {
        lines.push(Line::from(""));
        let spin = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let si = (session.started.elapsed().as_millis() / 80) as usize % spin.len();
        lines.push(Line::from(Span::styled(
            format!("  {}  Waiting for browser…", spin[si]),
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
        if let Some(ref code) = session.code {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  one-time code",
                Style::default().fg(app.theme.line_no),
            )));
            lines.push(Line::from(Span::styled(
                format!("    {code}"),
                Style::default()
                    .fg(app.theme.accent_fg)
                    .bg(app.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        let url = session
            .url
            .as_deref()
            .unwrap_or("https://github.com/login/device");
        lines.push(Line::from(Span::styled(
            format!("  open  {url}"),
            Style::default().fg(app.theme.success),
        )));
        if session.code_delivered {
            lines.push(Line::from(Span::styled(
                "  ✓ code copied to clipboard · browser opened",
                Style::default().fg(app.theme.success),
            )));
        }
        lines.push(Line::from(Span::styled(
            "  Paste the code in the browser, then authorize.",
            Style::default().fg(app.theme.muted),
        )));
        if !session.log.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("  · {}", session.log.last().map(|s| s.as_str()).unwrap_or("")),
                Style::default().fg(app.theme.line_no),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Actions  ·  j/k move  ·  Enter run  ·  r refresh",
        Style::default().fg(app.theme.line_no),
    )));

    let actions = app.git_wb.auth_actions();
    let sel = app.git_wb.auth_action_sel.min(actions.len().saturating_sub(1));
    for (i, a) in actions.iter().enumerate() {
        if lines.len() >= list_h.saturating_sub(1) {
            break;
        }
        let here = i == sel;
        let bg = if here {
            app.theme.panel_sel_bg
        } else {
            app.theme.editor_bg
        };
        let fg = if here {
            app.theme.panel_sel_fg
        } else {
            app.theme.fg
        };
        let mark = if here { "▸" } else { " " };
        let label = if w > 8 {
            format!("  {mark} {a}")
        } else {
            a.to_string()
        };
        lines.push(Line::from(Span::styled(
            label,
            Style::default().fg(fg).bg(bg).add_modifier(if here {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        )));
    }

    if let Some(ref msg) = app.git_wb.message {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {msg}"),
            Style::default().fg(app.theme.success),
        )));
    }
    if let Some(ref err) = app.git_wb.error {
        lines.push(Line::from(Span::styled(
            format!("  ! {err}"),
            Style::default().fg(app.theme.error),
        )));
    }
}
///
/// Renders **in place of the editor pane** — a flat mode switch, not a window
/// floating over the buffer. On entry the old source view is consumed top-down
/// by the pretty content behind a ░▒▓ wavefront; on exit the wave **reverses**
/// (pretty → source).
fn draw_preview_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let linear = app.preview.anim_progress();
    let t = if app.preview.closing {
        // Ease-in reverse so the source “grows back” cleanly.
        ease_in_cubic(linear)
    } else {
        ease_out_cubic(linear)
    };
    render_preview_pane(f, app, area, t);
}

/// Depth (rows) of the ░▒▓ transformation band between old and new content.
const PREVIEW_BAND_ROWS: f32 = 3.5;

/// Gentle easing — mild enough that dropped frames don't swallow the motion.
fn ease_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Display columns for terminal cells (CJK / emoji-aware).
fn ss_display_width(s: &str) -> usize {
    s.chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(1))
        .sum()
}

/// Pad `s` with spaces so its display width equals `width` (centered).
fn ss_center_in(s: &str, width: usize) -> String {
    let w = ss_display_width(s);
    if w >= width {
        return s.to_string();
    }
    let left = (width - w) / 2;
    let right = width - w - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}

fn short_adapter(name: &str) -> &str {
    if name.is_empty() {
        "dap"
    } else if name.contains("debugpy") || name == "python3" || name == "python" {
        "py"
    } else if name.contains("dlv") {
        "go"
    } else if name.contains("lldb") || name.contains("codelldb") {
        "lldb"
    } else {
        // last path component, truncated
        name.rsplit('/').next().unwrap_or(name)
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

fn render_preview_pane(f: &mut Frame, app: &App, area: Rect, t: f32) {
    use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};
    use xei_core::preview::preview_line_to_ratatui;

    if area.width < 6 || area.height < 2 {
        return;
    }
    let bg = app.theme.editor_bg;
    f.render_widget(Block::default().style(Style::default().bg(bg)), area);

    let rows_total = area.height as usize;
    let page = rows_total - 1;
    let lines = &app.preview.lines;
    let scroll = app.preview.scroll.min(lines.len().saturating_sub(1));
    let ext = app.file_extension();

    // Transform sweep over the WHOLE pane, header row included. Below the
    // wavefront the pane still shows the untouched editor view — same gutter,
    // git signs, and syntax colors — so the first frame is indistinguishable
    // from NORMAL mode and the mode switch *is* the animation. Above the
    // wavefront sits the settled pretty view; rows inside it materialize
    // through ░▒▓ shades.
    let wave = t * (rows_total as f32 + PREVIEW_BAND_ROWS);
    // Pretty content is inset by the gutter width so text doesn't jump left
    // when a source row transforms (the editor text also starts at column 5).
    let pad = " ".repeat(LINE_NO_WIDTH as usize);
    let pad_span = || Span::styled(pad.clone(), Style::default().bg(bg));

    let mut out: Vec<Line> = Vec::with_capacity(rows_total);
    for r in 0..rows_total {
        let a = ((wave - r as f32) / PREVIEW_BAND_ROWS).clamp(0.0, 1.0);
        if r == 0 {
            // The header materializes as the wave passes the first row.
            out.push(if a > 0.0 {
                preview_header_line(app, area.width, a)
            } else {
                source_line(app, 0, ext.as_deref())
            });
            continue;
        }
        if a <= 0.0 {
            out.push(source_line(app, r, ext.as_deref()));
            continue;
        }
        let pretty = lines.get(scroll + r - 1);
        if a >= 1.0 {
            out.push(match pretty {
                Some(pl) => {
                    let mut spans = vec![pad_span()];
                    spans.extend(preview_line_to_ratatui(pl, app.theme).spans);
                    Line::from(spans)
                }
                None => Line::from(Span::raw("")),
            });
        } else {
            match pretty {
                Some(pl) => {
                    let mut spans = vec![pad_span()];
                    spans.extend(shade_preview_line(pl, app.theme, bg, a, r).spans);
                    out.push(Line::from(spans));
                }
                None => {
                    // Nothing pretty arrives here — the old line de-materializes.
                    let old = xei_core::preview::PreviewLine {
                        spans: vec![(
                            source_row_plain(app, r),
                            xei_core::preview::PreviewStyle::Dim,
                        )],
                    };
                    out.push(shade_preview_line(&old, app.theme, bg, 1.0 - a, r));
                }
            }
        }
    }
    f.render_widget(
        Paragraph::new(out).style(Style::default().fg(app.theme.fg).bg(bg)),
        area,
    );

    // Flat scrollbar along the right edge once settled.
    if t >= 1.0 && lines.len() > page {
        let sb = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: area.y + 1,
            width: 1,
            height: area.height - 1,
        };
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            sb,
            &mut ScrollbarState::new(lines.len().saturating_sub(page).max(1)).position(scroll),
        );
    }
}

/// Flat preview header: label ── rule ── hints, fading in with the wavefront.
fn preview_header_line(app: &App, width: u16, a: f32) -> Line<'static> {
    let bg = app.theme.editor_bg;
    let accent = app.theme.accent;
    let header_fg = lerp_color(bg, accent, 0.35 + 0.65 * a);
    let kind = app.preview.kind.map(|k| k.label()).unwrap_or("Preview");
    let label = format!(" ▾ {} Preview ", kind);
    let hint = " Esc close · j/k scroll · r refresh ";
    let mut spans = vec![Span::styled(
        label.clone(),
        Style::default()
            .fg(header_fg)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )];
    let used = label.width() + hint.width();
    if (width as usize) > used + 2 {
        spans.push(Span::styled(
            "─".repeat(width as usize - used),
            Style::default().fg(lerp_color(bg, app.theme.border, a)).bg(bg),
        ));
        spans.push(Span::styled(
            hint,
            Style::default().fg(app.theme.line_no).bg(bg),
        ));
    }
    Line::from(spans)
}

/// The editor's own rendering of the buffer row at this pane row — gutter,
/// git sign, syntax colors, cursor-line strip — so rows the wavefront hasn't
/// reached yet are pixel-identical to how NORMAL mode drew them. (The editor
/// renders one buffer line per screen row, so display row = scroll + row.)
fn source_line(app: &App, area_row: usize, ext: Option<&str>) -> Line<'static> {
    let bg = app.theme.editor_bg;
    let row = app.scroll + area_row;
    if row >= app.buffer.line_count() {
        return Line::from(Span::styled(String::new(), Style::default().bg(bg)));
    }
    let cursor_row = app.buffer.cursor().row;
    let is_cursor_line = row == cursor_row;
    let git_sign = app.git.sign_at(row);
    let git_ch = match git_sign {
        Some(xei_core::git::GitSign::Added) => '+',
        Some(xei_core::git::GitSign::Modified) => '~',
        Some(xei_core::git::GitSign::Deleted) => '▁',
        None => ' ',
    };
    let git_color = match git_sign {
        Some(xei_core::git::GitSign::Added) => app.theme.success,
        Some(xei_core::git::GitSign::Modified) => app.theme.warning,
        Some(xei_core::git::GitSign::Deleted) => app.theme.error,
        None => {
            if is_cursor_line {
                app.theme.fg
            } else {
                app.theme.line_no
            }
        }
    };
    let num = if app.relative_number && !is_cursor_line {
        format!("{:>3}", (row as isize - cursor_row as isize).unsigned_abs())
    } else {
        format!("{:>3}", row + 1)
    };
    let mut gutter_style = Style::default().fg(git_color).bg(bg);
    if is_cursor_line {
        gutter_style = gutter_style.add_modifier(Modifier::BOLD);
    }
    let text = app.buffer.line(row).to_string();
    let mut spans = vec![Span::styled(format!("{}{} ", git_ch, num), gutter_style)];
    spans.extend(render_line_with_highlights(
        &row,
        &text,
        app,
        None,
        ext,
        is_cursor_line,
        None, // preview transition: full line (caller clips)
    ));
    Line::from(spans)
}

/// Gutter + text of the old buffer line as plain text (for de-materializing).
fn source_row_plain(app: &App, area_row: usize) -> String {
    let row = app.scroll + area_row;
    if row >= app.buffer.line_count() {
        return String::new();
    }
    format!(" {:>3} {}", row + 1, expand_tabs(app.buffer.line(row)))
}

fn expand_tabs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut vis = 0usize;
    for ch in line.chars() {
        if ch == '\t' {
            let n = 4 - (vis % 4);
            out.extend(std::iter::repeat(' ').take(n));
            vis += n;
        } else {
            out.push(ch);
            vis += UnicodeWidthChar::width(ch).unwrap_or(1);
        }
    }
    out
}

/// Render one preview line mid-animation: each glyph is replaced by a shade
/// block (`░▒▓`) picked from its local alpha, colored by blending the final
/// foreground toward the background. Per-cell jitter keeps the band organic.
fn shade_preview_line(
    line: &xei_core::preview::PreviewLine,
    theme: &xei_core::theme::Theme,
    bg: Color,
    alpha: f32,
    row: usize,
) -> Line<'static> {
    let base_fg = theme.fg;
    const SHADES: [char; 3] = ['░', '▒', '▓'];
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut col = 0usize;

    for (text, st) in &line.spans {
        let final_style = xei_core::preview::to_ratatui_style(*st, theme);
        let fg = final_style.fg.unwrap_or(base_fg);

        // Group consecutive cells of equal shade level into one span.
        let mut run = String::new();
        let mut run_level: i8 = i8::MIN;
        let flush = |run: &mut String, level: i8, spans: &mut Vec<Span<'static>>| {
            if run.is_empty() {
                return;
            }
            let style = match level {
                3 => final_style.bg(bg),
                l => {
                    let blend = 0.30 + 0.25 * l.max(0) as f32;
                    Style::default().fg(lerp_color(bg, fg, blend)).bg(bg)
                }
            };
            spans.push(Span::styled(std::mem::take(run), style));
        };

        for ch in text.chars() {
            let w = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
            // Deterministic per-cell jitter so the wavefront shimmers.
            let jitter = ((col * 7 + row * 13) % 4) as f32 * 0.06;
            let a = (alpha - jitter).clamp(0.0, 1.0);
            let level: i8 = if ch == ' ' || a >= 0.92 {
                3 // spaces and nearly-settled cells show as-is
            } else {
                ((a * 3.4) as i8).min(2)
            };
            if level != run_level {
                flush(&mut run, run_level, &mut spans);
                run_level = level;
            }
            if level == 3 {
                run.push(ch);
            } else {
                // Wide glyphs (CJK) become two shade cells to keep columns stable.
                for _ in 0..w {
                    run.push(SHADES[level.max(0) as usize]);
                }
            }
            col += w;
        }
        flush(&mut run, run_level, &mut spans);
    }

    if spans.is_empty() {
        spans.push(Span::raw(""));
    }
    Line::from(spans)
}

/// Linear RGB blend; non-RGB colors snap at the midpoint.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => Color::Rgb(
            (ar as f32 + (br as f32 - ar as f32) * t) as u8,
            (ag as f32 + (bg as f32 - ag as f32) * t) as u8,
            (ab as f32 + (bb as f32 - ab as f32) * t) as u8,
        ),
        _ => {
            if t < 0.5 {
                a
            } else {
                b
            }
        }
    }
}

fn status_color(s: xei_core::scm::ScmStatus, theme: &xei_core::theme::Theme) -> Color {
    use xei_core::scm::ScmStatus;
    match s {
        ScmStatus::Modified => theme.warning,
        ScmStatus::Added => theme.success,
        ScmStatus::Deleted => theme.error,
        ScmStatus::Renamed => theme.accent,
        ScmStatus::Untracked => theme.success,
        ScmStatus::Conflict => theme.error,
        ScmStatus::TypeChange => theme.mode_settings,
        ScmStatus::Unknown => theme.muted,
    }
}

fn draw_palette(f: &mut Frame, app: &mut App, area: Rect) {
    let width = area.width.saturating_sub(8).min(72).max(40);
    let height = 16u16.min(area.height.saturating_sub(4)).max(8);
    // Entrance: expand downward from the input row while the border fades in.
    let t = ease_out_quad(app.palette.anim_progress());
    let height = ((height as f32 * t).ceil() as u16).clamp(4, height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + 2;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };

    f.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        Rect {
            x: x + 1,
            y: y + 1,
            width,
            height,
        },
    );

    let border_fg = lerp_color(app.theme.completion_bg, app.theme.mode_normal, 0.4 + 0.6 * t);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_fg))
        .style(Style::default().bg(app.theme.completion_bg))
        .title(Span::styled(
            app.palette.title(),
            Style::default().fg(border_fg).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let prompt = match app.palette.kind {
        xei_core::PaletteKind::Files => ">",
        xei_core::PaletteKind::Commands => ":",
        xei_core::PaletteKind::Problems => "!",
        xei_core::PaletteKind::Symbols => "@",
        xei_core::PaletteKind::CodeActions => ".",
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", prompt),
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.mode_normal)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}█", app.palette.query),
                Style::default().fg(app.theme.fg),
            ),
        ])),
        chunks[0],
    );

    let list_h = chunks[1].height as usize;
    let start = app
        .palette
        .selected
        .saturating_sub(list_h.saturating_sub(1) / 2)
        .min(
            app.palette
                .filtered
                .len()
                .saturating_sub(list_h),
        );

    let mut lines: Vec<Line> = Vec::new();
    for (abs, &item_idx) in app.palette.filtered.iter().enumerate().skip(start).take(list_h)
    {
        let item = &app.palette.items[item_idx];
        let selected = abs == app.palette.selected;
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(app.theme.completion_selected)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.fg)
        };
        let detail_style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(app.theme.completion_selected)
        } else {
            Style::default().fg(app.theme.line_no)
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", item.label), style),
            Span::styled(item.detail.clone(), detail_style),
        ]));
    }

    f.render_widget(Paragraph::new(lines), chunks[1]);
}

fn draw_hover(f: &mut Frame, app: &App, area: Rect, hover: &str) {
    let lines: Vec<&str> = hover.lines().take(12).collect();
    let height = (lines.len() as u16 + 2).min(14);
    let width = lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(20)
        .min(60)
        .max(20) as u16
        + 4;

    let cursor = app.buffer.cursor();
    let screen_col = app
        .buffer
        .buffer_col_to_screen_col(cursor.row, cursor.col);
    let x = (app.viewport.x + LINE_NO_WIDTH + screen_col as u16)
        .min(area.x + area.width.saturating_sub(width));
    let y = (app.viewport.y + (cursor.row.saturating_sub(app.scroll)) as u16 + 1)
        .min(area.y + area.height.saturating_sub(height));

    let popup = Rect {
        x,
        y,
        width,
        height,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.border))
        .style(Style::default().bg(app.theme.completion_bg))
        .title(Span::styled(" hover ", Style::default().fg(app.theme.line_no)));

    let text: Vec<Line> = lines
        .iter()
        .map(|l| Line::from(Span::styled((*l).to_string(), Style::default().fg(app.theme.fg))))
        .collect();

    f.render_widget(Clear, popup);
    f.render_widget(Paragraph::new(text).block(block), popup);
}

/// Paint inverted cells at extra multi-cursor positions.
fn draw_pr_review(f: &mut Frame, app: &mut App, area: Rect) {
    use xei_core::pr_review::PrReviewFocus;

    app.pr_tab_hits.clear();
    app.pr_row_hits.clear();
    let bg = app.theme.panel_bg;
    // Clear symbols first — a style-only Block recolors cells but leaves the
    // previous surface's text visible underneath.
    f.render_widget(Clear, area);
    f.render_widget(Block::default().style(Style::default().bg(bg)), area);

    let title = if app.pr_review.loading {
        format!(" PR #{} · ⏳ loading… ", app.pr_review.number)
    } else {
        format!(
            " PR #{} · {} · {} → {} · @{} ",
            app.pr_review.number,
            app.pr_review.title,
            app.pr_review.head,
            app.pr_review.base,
            app.pr_review.author
        )
    };
    let mut header: Vec<Span> = vec![Span::styled(
        title.clone(),
        Style::default()
            .fg(app.theme.accent)
            .bg(app.theme.status_bg)
            .add_modifier(Modifier::BOLD),
    )];
    let mut tab_x = area.x.saturating_add(title.chars().count() as u16);
    header.push(Span::styled("  ", Style::default().bg(bg)));
    tab_x = tab_x.saturating_add(2);
    let focus = app.pr_review.focus;
    for (i, (name, f_)) in [
        ("1 Files", PrReviewFocus::Files),
        ("2 Comments", PrReviewFocus::Comments),
        ("3 Body", PrReviewFocus::Body),
    ]
    .iter()
    .enumerate()
    {
        let active = focus == *f_;
        let label = format!(" {name} ");
        let w = label.chars().count() as u16;
        app.pr_tab_hits.push((tab_x, area.y, w, 1, i as u8));
        tab_x = tab_x.saturating_add(w);
        header.push(Span::styled(
            label,
            if active {
                Style::default()
                    .fg(app.theme.accent_fg)
                    .bg(app.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.muted).bg(bg)
            },
        ));
    }
    header.push(Span::styled(
        "  · Tab · o open · c checkout · b browser · Esc",
        Style::default().fg(app.theme.muted).bg(bg),
    ));
    f.render_widget(
        Paragraph::new(Line::from(header)),
        Rect::new(area.x, area.y, area.width, 1),
    );

    if area.height < 4 {
        return;
    }

    let mut body = Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(1));
    // Fetch failure — surface it inline instead of an empty shell.
    if let Some(ref err) = app.pr_review.error {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" ⚠ {err} — c checkout may still work · Esc close "),
                Style::default()
                    .fg(app.theme.error)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect::new(body.x, body.y, body.width, 1),
        );
        body = Rect::new(body.x, body.y + 1, body.width, body.height.saturating_sub(1));
    }
    match app.pr_review.focus {
        PrReviewFocus::Files => {
            let left_w = (body.width / 3).clamp(18, 36);
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(left_w), Constraint::Min(20)])
                .split(body);
            // file list — keep the selection scrolled into view
            let list_h = (chunks[0].height as usize).max(1);
            let skip = app
                .pr_review
                .file_sel
                .saturating_sub(list_h.saturating_sub(1) / 2)
                .min(app.pr_review.files.len().saturating_sub(list_h));
            let mut flines: Vec<Line> = Vec::new();
            let mut file_hits: Vec<(u16, u16, u16, u16, usize)> = Vec::new();
            for (i, file) in app
                .pr_review
                .files
                .iter()
                .enumerate()
                .skip(skip)
                .take(list_h)
            {
                let sel = i == app.pr_review.file_sel;
                let mark = if sel { "▸" } else { " " };
                let text = format!(
                    "{mark} {}  +{} -{}",
                    file.path, file.additions, file.deletions
                );
                let y = chunks[0].y + 1 + flines.len() as u16;
                if y < chunks[0].y + chunks[0].height {
                    file_hits.push((chunks[0].x, y, chunks[0].width, 1, i));
                }
                flines.push(Line::from(Span::styled(
                    text,
                    if sel {
                        Style::default()
                            .fg(app.theme.panel_sel_fg)
                            .bg(app.theme.panel_sel_bg)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(app.theme.fg)
                    },
                )));
            }
            if flines.is_empty() {
                flines.push(Line::from(Span::styled(
                    if app.pr_review.loading {
                        "  ⏳ loading files…"
                    } else {
                        "  (no files)"
                    },
                    Style::default().fg(app.theme.muted),
                )));
            }
            f.render_widget(
                Paragraph::new(flines).block(
                    Block::default()
                        .borders(Borders::RIGHT)
                        .border_style(Style::default().fg(app.theme.panel_border))
                        .style(Style::default().bg(bg))
                        .title(" files "),
                ),
                chunks[0],
            );
            app.pr_row_hits = file_hits;
            // diff
            let start = app.pr_review.diff_scroll;
            let mut dlines: Vec<Line> = Vec::new();
            for line in app.pr_review.file_diff.iter().skip(start).take(chunks[1].height as usize) {
                let fg = if line.starts_with('+') && !line.starts_with("+++") {
                    app.theme.success
                } else if line.starts_with('-') && !line.starts_with("---") {
                    app.theme.error
                } else if line.starts_with("@@") {
                    app.theme.git_hunk
                } else {
                    app.theme.fg
                };
                dlines.push(Line::from(Span::styled(
                    line.clone(),
                    Style::default().fg(fg),
                )));
            }
            if dlines.is_empty() {
                dlines.push(Line::from(Span::styled(
                    "  (select a file)",
                    Style::default().fg(app.theme.muted),
                )));
            }
            // file comments badge
            let n_comments = app.pr_review.comments_for_selected_file().len();
            f.render_widget(
                Paragraph::new(dlines).block(
                    Block::default()
                        .style(Style::default().bg(bg))
                        .title(Span::styled(
                            format!(" diff · {n_comments} comment(s) on file · J/K scroll "),
                            Style::default().fg(app.theme.muted),
                        )),
                ),
                chunks[1],
            );
        }
        PrReviewFocus::Comments => {
            let mut lines: Vec<Line> = Vec::new();
            if app.pr_review.comments.is_empty() {
                lines.push(Line::from(Span::styled(
                    if app.pr_review.loading {
                        "  ⏳ loading comments…"
                    } else {
                        "  No review comments on this PR"
                    },
                    Style::default().fg(app.theme.muted),
                )));
            }
            // Keep the selection scrolled into view
            let list_h = (body.height as usize).max(1);
            let skip = app
                .pr_review
                .comment_sel
                .saturating_sub(list_h.saturating_sub(1) / 2)
                .min(app.pr_review.comments.len().saturating_sub(list_h));
            let mut comment_hits: Vec<(u16, u16, u16, u16, usize)> = Vec::new();
            for (i, c) in app
                .pr_review
                .comments
                .iter()
                .enumerate()
                .skip(skip)
                .take(list_h)
            {
                let sel = i == app.pr_review.comment_sel;
                let mark = if sel { "▸" } else { " " };
                let y = body.y + lines.len() as u16;
                if y < body.y + body.height {
                    comment_hits.push((body.x, y, body.width, 1, i));
                }
                let loc = c
                    .line
                    .map(|l| format!(":{}", l))
                    .unwrap_or_default();
                // One row per comment: flatten newlines/tabs from multi-line bodies
                let body: String = c
                    .body
                    .chars()
                    .map(|ch| if ch == '\n' || ch == '\r' || ch == '\t' { ' ' } else { ch })
                    .take(120)
                    .collect();
                let text = format!("{mark} @{}  {}{}  — {body}", c.author, c.path, loc);
                lines.push(Line::from(Span::styled(
                    text,
                    if sel {
                        Style::default()
                            .fg(app.theme.panel_sel_fg)
                            .bg(app.theme.panel_sel_bg)
                    } else {
                        Style::default().fg(app.theme.fg)
                    },
                )));
            }
            f.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), body);
            app.pr_row_hits = comment_hits;
        }
        PrReviewFocus::Body => {
            let start = app.pr_review.body_scroll;
            let mut lines: Vec<Line> = Vec::new();
            for (i, line) in app.pr_review.body.lines().enumerate().skip(start) {
                if i - start >= body.height as usize {
                    break;
                }
                lines.push(Line::from(Span::styled(
                    format!("  {line}"),
                    Style::default().fg(app.theme.fg),
                )));
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (empty description)",
                    Style::default().fg(app.theme.muted),
                )));
            }
            f.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), body);
        }
    }
}

fn draw_call_hierarchy(f: &mut Frame, app: &App, area: Rect) {
    let w = 56u16.min(area.width.saturating_sub(4)).max(32);
    let h = ((app.call_hierarchy.items.len() as u16) + 4)
        .min(area.height.saturating_sub(2))
        .max(8);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + 2;
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let shadow = Rect {
        x: x.saturating_add(1),
        y: y.saturating_add(1),
        width: w,
        height: h,
    };
    f.render_widget(Block::default().style(Style::default().bg(Color::Black)), shadow);

    let bg = app.theme.panel_bg;
    let title = format!(
        " calls · {} · {} ",
        app.call_hierarchy.root_name,
        app.call_hierarchy.direction.label()
    );
    let mut lines: Vec<Line> = Vec::new();
    if app.call_hierarchy.loading {
        lines.push(Line::from(Span::styled(
            "  loading…",
            Style::default().fg(app.theme.muted),
        )));
    } else if app.call_hierarchy.items.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no calls — server may not support hierarchy)",
            Style::default().fg(app.theme.muted),
        )));
    }
    for (i, item) in app.call_hierarchy.items.iter().enumerate() {
        let sel = i == app.call_hierarchy.selected;
        let mark = if sel { "▸" } else { " " };
        let file = std::path::Path::new(&item.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&item.path);
        let detail = if item.detail.is_empty() {
            String::new()
        } else {
            format!(" · {}", item.detail)
        };
        let text = format!(
            "{mark} {}  {}:{}{}  [{}]",
            item.name,
            file,
            item.row + 1,
            detail,
            item.kind
        );
        let style = if sel {
            Style::default()
                .fg(app.theme.panel_sel_fg)
                .bg(app.theme.panel_sel_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.fg)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    lines.push(Line::from(Span::styled(
        "  Tab direction · Enter jump · Esc close",
        Style::default().fg(app.theme.muted),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.panel_border))
        .style(Style::default().bg(bg))
        .title(Span::styled(
            title,
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Clear, popup);
    f.render_widget(Paragraph::new(lines).block(block), popup);
}

fn draw_rebase_panel(f: &mut Frame, app: &App, area: Rect) {
    let w = 64u16.min(area.width.saturating_sub(4)).max(36);
    let h = ((app.rebase.entries.len() as u16) + 5)
        .min(area.height.saturating_sub(2))
        .max(10);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + 2;
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let shadow = Rect {
        x: x.saturating_add(1),
        y: y.saturating_add(1),
        width: w,
        height: h,
    };
    f.render_widget(Block::default().style(Style::default().bg(Color::Black)), shadow);

    let bg = app.theme.panel_bg;
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "  action  hash     subject",
        Style::default().fg(app.theme.muted),
    )));
    for (i, e) in app.rebase.entries.iter().enumerate() {
        let sel = i == app.rebase.selected;
        let mark = if sel { "▸" } else { " " };
        let act_col = match e.action {
            xei_core::rebase::RebaseAction::Pick => app.theme.success,
            xei_core::rebase::RebaseAction::Drop => app.theme.error,
            xei_core::rebase::RebaseAction::Squash | xei_core::rebase::RebaseAction::Fixup => {
                app.theme.warning
            }
            xei_core::rebase::RebaseAction::Reword | xei_core::rebase::RebaseAction::Edit => {
                app.theme.accent
            }
        };
        let text = format!(
            "{mark} {:7}  {}  {}",
            e.action.label(),
            e.short,
            e.subject
        );
        let style = if sel {
            Style::default()
                .fg(act_col)
                .bg(app.theme.panel_sel_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(act_col)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    lines.push(Line::from(Span::styled(
        "  Tab/p/r/e/s/f/d · J/K reorder · Enter run · Esc cancel",
        Style::default().fg(app.theme.muted),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.warning))
        .style(Style::default().bg(bg))
        .title(Span::styled(
            " interactive rebase ",
            Style::default()
                .fg(app.theme.warning)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Clear, popup);
    f.render_widget(Paragraph::new(lines).block(block), popup);
}

/// Editor on top + DAP debug panel docked at the bottom.
fn draw_editor_with_debug(f: &mut Frame, app: &mut App, area: Rect) {
    app.dap_tab_hits.clear();
    app.dap_row_hits.clear();
    app.dap_panel_rect = None;
    let full_h = 12u16.min(area.height.saturating_sub(4)).max(6);
    // Slide-up entrance (lazy first-frame clock)
    let t = app.dap.anim_progress();
    let t = ease_out_cubic(t.clamp(0.0, 1.0));
    let panel_h = ((full_h as f32) * t).round() as u16;
    if panel_h == 0 {
        draw_editor_split_or_single(f, app, area);
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(panel_h)])
        .split(area);
    draw_editor_split_or_single(f, app, chunks[0]);
    draw_debug_panel(f, app, chunks[1]);
}

fn draw_debug_panel(f: &mut Frame, app: &mut App, area: Rect) {
    use xei_core::dap::{DapState, DebugPane};

    app.dap_panel_rect = Some((area.x, area.y, area.width, area.height));
    app.dap_tab_hits.clear();
    app.dap_row_hits.clear();

    let bg = app.theme.panel_bg;
    let border = app.theme.panel_border;
    let accent = app.theme.accent;
    let dim = app.theme.muted;
    let ok = app.theme.success;
    let stop = app.theme.warning;

    f.render_widget(Block::default().style(Style::default().bg(bg)), area);

    let state = app.dap.state;
    let state_col = match state {
        DapState::Stopped => stop,
        DapState::Running | DapState::Starting => ok,
        DapState::Idle | DapState::Ending => dim,
    };
    let adapter = if app.dap.adapter_name.is_empty() {
        "no adapter"
    } else {
        app.dap.adapter_name.as_str()
    };
    let reason = app
        .dap
        .build_message
        .as_deref()
        .or(app.dap.stopped_reason.as_deref())
        .unwrap_or(state.label());
    let title = format!(
        " DEBUG · {} · {} · {} ",
        state.label().to_uppercase(),
        adapter,
        reason
    );
    // Tab hit geometry (approximate widths for mouse)
    let tab_labels = ["1 Stack", "2 Vars", "3 BPs", "4 Console"];
    let mut tab_x = area.x.saturating_add(title.chars().count() as u16 + 1);
    let tabs: Vec<Span> = tab_labels
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let pane = match i {
                0 => DebugPane::Stack,
                1 => DebugPane::Variables,
                2 => DebugPane::Breakpoints,
                _ => DebugPane::Console,
            };
            let label = format!(" {t} ");
            let w = label.chars().count() as u16;
            app.dap_tab_hits
                .push((tab_x, area.y, w, 1, i as u8));
            tab_x = tab_x.saturating_add(w);
            let active = app.dap.pane == pane;
            Span::styled(
                label,
                if active {
                    Style::default()
                        .fg(app.theme.accent_fg)
                        .bg(accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(dim).bg(bg)
                },
            )
        })
        .collect();

    // Header row
    if area.height >= 1 {
        let mut header = vec![Span::styled(
            title,
            Style::default()
                .fg(state_col)
                .bg(app.theme.status_bg)
                .add_modifier(Modifier::BOLD),
        )];
        header.push(Span::styled(" ", Style::default().bg(bg)));
        header.extend(tabs);
        header.push(Span::styled(
            "  F5 cont · F6 pause · F9 bp · F10/F11 step · Esc unfocus · q close ",
            Style::default().fg(dim).bg(bg),
        ));
        f.render_widget(
            Paragraph::new(Line::from(header)),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }

    if area.height < 3 {
        return;
    }

    let body = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );
    // left border accent
    f.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(border))
            .style(Style::default().bg(bg)),
        body,
    );

    let inner = Rect::new(body.x, body.y.saturating_add(0), body.width, body.height);
    let mut lines: Vec<Line> = Vec::new();

    match app.dap.pane {
        DebugPane::Stack => {
            if app.dap.stack.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (no stack — start with F5 or SPC d s)",
                    Style::default().fg(dim),
                )));
            }
            for (i, fr) in app.dap.stack.iter().enumerate() {
                let sel = i == app.dap.focus_row;
                let mark = if sel { "▸" } else { " " };
                let file = std::path::Path::new(&fr.path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&fr.path);
                let text = format!(
                    "{mark} {}  {}:{}  {}",
                    fr.name,
                    file,
                    fr.line + 1,
                    fr.path
                );
                let style = if sel {
                    Style::default()
                        .fg(app.theme.panel_sel_fg)
                        .bg(app.theme.panel_sel_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.theme.fg)
                };
                lines.push(Line::from(Span::styled(text, style)));
                let y = inner.y.saturating_add(i as u16);
                if y < inner.y.saturating_add(inner.height) {
                    app.dap_row_hits
                        .push((inner.x, y, inner.width, 1, i));
                }
            }
        }
        DebugPane::Variables => {
            if app.dap.vars.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (no variables — stop at a BP first)",
                    Style::default().fg(dim),
                )));
            }
            for (i, v) in app.dap.vars.iter().enumerate() {
                let sel = i == app.dap.focus_row;
                let mark = if sel { "▸" } else { " " };
                let indent = "  ".repeat(v.depth);
                let twist = if v.var_ref > 0 {
                    if v.expanded {
                        "▾ "
                    } else {
                        "▸ "
                    }
                } else if v.is_scope {
                    "  "
                } else {
                    "  "
                };
                let typ = if v.typ.is_empty() {
                    String::new()
                } else {
                    format!(": {}", v.typ)
                };
                let text = if v.is_scope {
                    format!("{mark}{indent}{twist}{}", v.name)
                } else {
                    format!("{mark}{indent}{twist}{} = {}{}", v.name, v.value, typ)
                };
                let style = if sel {
                    Style::default()
                        .fg(app.theme.panel_sel_fg)
                        .bg(app.theme.panel_sel_bg)
                        .add_modifier(Modifier::BOLD)
                } else if v.is_scope {
                    Style::default().fg(accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.theme.fg)
                };
                lines.push(Line::from(Span::styled(text, style)));
                let y = inner.y.saturating_add(i as u16);
                if y < inner.y.saturating_add(inner.height) {
                    app.dap_row_hits
                        .push((inner.x, y, inner.width, 1, i));
                }
            }
            lines.push(Line::from(Span::styled(
                "  Enter/l expand · h collapse",
                Style::default().fg(dim),
            )));
        }
        DebugPane::Breakpoints => {
            let bps = app.dap.flat_bps();
            if bps.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  No breakpoints — F9 / :bp · :bp if expr",
                    Style::default().fg(dim),
                )));
            }
            for (i, (path, line, verified)) in bps.iter().enumerate() {
                let sel = i == app.dap.focus_row;
                let mark = if sel { "▸" } else { " " };
                let dot = if *verified { "●" } else { "○" };
                let file = std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path);
                // Show condition/log if set
                let extra = app
                    .dap
                    .breakpoints
                    .get(path)
                    .and_then(|list| list.iter().find(|b| b.line == *line))
                    .map(|b| {
                        if let Some(ref c) = b.condition {
                            format!(" if {c}")
                        } else if let Some(ref m) = b.log_message {
                            format!(" log {m}")
                        } else {
                            String::new()
                        }
                    })
                    .unwrap_or_default();
                let text = format!("{mark} {dot} {file}:{}{extra}", line + 1);
                let style = if sel {
                    Style::default()
                        .fg(app.theme.error)
                        .bg(app.theme.panel_sel_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.theme.error)
                };
                lines.push(Line::from(Span::styled(text, style)));
                let y = inner.y.saturating_add(i as u16);
                if y < inner.y.saturating_add(inner.height) {
                    app.dap_row_hits
                        .push((inner.x, y, inner.width, 1, i));
                }
            }
        }
        DebugPane::Console => {
            // Leave one row for the REPL prompt
            let body_h = inner.height.saturating_sub(1).max(1) as usize;
            let start = app.dap.console.len().saturating_sub(body_h);
            let mut row_i = 0u16;
            for (i, line) in app.dap.console.iter().enumerate().skip(start) {
                let sel = i == app.dap.focus_row;
                lines.push(Line::from(Span::styled(
                    format!("  {line}"),
                    if sel {
                        Style::default().fg(app.theme.panel_sel_fg)
                    } else {
                        Style::default().fg(app.theme.fg)
                    },
                )));
                row_i = row_i.saturating_add(1);
            }
            if app.dap.console.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (console empty — type expr + Enter when stopped)",
                    Style::default().fg(dim),
                )));
            }
            // REPL input line
            let prompt = format!("  > {}_", app.dap.eval_input);
            lines.push(Line::from(Span::styled(
                prompt,
                Style::default()
                    .fg(app.theme.success)
                    .add_modifier(Modifier::BOLD),
            )));
            let _ = row_i;
        }
    }

    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(bg)),
        inner,
    );
}

/// Editor with optional git-blame column sliding in from the left (Ctrl+B).
fn draw_editor_with_blame(f: &mut Frame, app: &mut App, area: Rect) {
    if !app.blame.visible() {
        draw_editor(f, app, area);
        return;
    }
    let linear = app.blame.anim_progress();
    let t = if app.blame.closing {
        // ease-in when leaving
        let u = linear.clamp(0.0, 1.0);
        u * u
    } else {
        ease_out_cubic(linear)
    };
    let bw = xei_core::git::blame_width_for_openness(t);
    if bw == 0 {
        draw_editor(f, app, area);
        return;
    }
    let bw = bw.min(area.width.saturating_sub(20).max(1));
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(bw), Constraint::Min(8)])
        .split(area);
    draw_blame_panel(f, app, chunks[0], t);
    draw_editor(f, app, chunks[1]);
}

/// Fixed flame-colored blame strip; content follows editor scroll/folds.
fn draw_blame_panel(f: &mut Frame, app: &App, area: Rect, openness: f32) {
    use xei_core::git::flame_color_for;

    // Dark ember background (theme-independent)
    let bg = Color::Rgb(28, 14, 10);
    f.render_widget(Block::default().style(Style::default().bg(bg)), area);

    if area.width < 4 || area.height < 1 {
        return;
    }

    // Header — plain BLAME (fades to a thin bar while sliding)
    let title = if openness > 0.35 { " BLAME " } else { " ▍ " };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("{title:<w$}", w = area.width as usize),
            Style::default()
                .fg(Color::Rgb(255, 200, 80))
                .bg(Color::Rgb(50, 20, 12))
                .add_modifier(Modifier::BOLD),
        )),
        Rect::new(area.x, area.y, area.width, 1),
    );

    if area.height < 2 {
        return;
    }

    // Mirror editor's visible buffer rows (scroll + folds)
    let scroll = app.scroll;
    let all_lines = app.buffer.lines();
    let visible = (area.height as usize).saturating_sub(1);
    let mut rows: Vec<usize> = Vec::new();
    for (idx, _) in all_lines.iter().enumerate().skip(scroll) {
        if app.folds.is_hidden(idx) {
            continue;
        }
        rows.push(idx);
        if rows.len() >= visible {
            break;
        }
    }

    let col_w = area.width as usize;
    let mut out: Vec<Line> = Vec::new();
    // Author label only on the first line of a consecutive same-author run
    let mut prev_author: Option<String> = None;
    for idx in rows {
        let (text, (r, g, b), bold) = if let Some(bl) = app.blame.lines.get(&idx) {
            let key = format!("{}:{}", bl.author, bl.hash);
            let flame = flame_color_for(&key);
            let author_key = bl.author.clone();
            let show_author = prev_author.as_ref() != Some(&author_key);
            prev_author = Some(author_key);
            if show_author {
                let author: String = bl.author.chars().take(10).collect();
                let label = format!("{author:<10} {}", bl.hash);
                let clipped: String = label.chars().take(col_w.saturating_sub(1)).collect();
                (format!(" {clipped}"), flame, true)
            } else {
                // Continuation: keep flame color, no repeated author text
                (" ".into(), flame, false)
            }
        } else {
            prev_author = None;
            (" ·".into(), (80, 40, 30), false)
        };
        // Pad to width
        let mut cell = text;
        while cell.chars().count() < col_w {
            cell.push(' ');
        }
        let cell: String = cell.chars().take(col_w).collect();
        let mut style = Style::default().fg(Color::Rgb(r, g, b)).bg(bg);
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        out.push(Line::from(Span::styled(cell, style)));
    }
    f.render_widget(
        Paragraph::new(out).style(Style::default().bg(bg)),
        Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(1)),
    );
}

fn draw_extra_cursors(f: &mut Frame, app: &App, area: Rect, text_width: usize) {
    let gutter_w = LINE_NO_WIDTH;
    for pos in &app.multi.extras {
        // Find first screen row mapping to this buffer row
        let Some(screen_i) = app
            .screen_row_to_buffer
            .iter()
            .position(|&r| r == pos.row)
        else {
            continue;
        };
        let vis = app.buffer.buffer_col_to_screen_col(pos.row, pos.col);
        let (seg, col_in_seg) = if !app.wrap_lines {
            // Horizontal-scroll mode: single segment panned by hscroll.
            if vis < app.hscroll || vis >= app.hscroll + text_width.max(1) {
                continue;
            }
            (0usize, vis - app.hscroll)
        } else if text_width == 0 {
            (0, 0)
        } else {
            (vis / text_width, vis % text_width)
        };
        // screen_row_to_buffer may have multiple segments — find matching base
        let seg_base = if app.wrap_lines {
            seg * text_width
        } else {
            app.hscroll
        };
        let mut row_i = screen_i;
        for (i, (&br, &base)) in app
            .screen_row_to_buffer
            .iter()
            .zip(app.screen_row_visual_base.iter())
            .enumerate()
        {
            if br == pos.row && base == seg_base {
                row_i = i;
                break;
            }
        }
        let x = area.x + gutter_w + col_in_seg as u16;
        let y = area.y + row_i as u16;
        if x >= area.x + area.width || y >= area.y + area.height {
            continue;
        }
        let ch = app
            .buffer
            .line(pos.row)
            .chars()
            .nth(pos.col)
            .unwrap_or(' ');
        let display = if ch == '\t' { ' ' } else { ch };
        f.render_widget(
            Paragraph::new(Span::styled(
                display.to_string(),
                Style::default()
                    .fg(app.theme.accent_fg)
                    .bg(app.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Rect::new(x, y, 1, 1),
        );
    }
}

fn draw_breadcrumbs(f: &mut Frame, app: &App, area: Rect) {
    let crumbs = app.breadcrumbs();
    let mut spans: Vec<Span> = vec![Span::styled(
        " ",
        Style::default().bg(app.theme.status_bg),
    )];
    for (i, part) in crumbs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                " › ",
                Style::default().fg(app.theme.line_no).bg(app.theme.status_bg),
            ));
        }
        let last = i + 1 == crumbs.len();
        spans.push(Span::styled(
            part.clone(),
            Style::default()
                .fg(if last {
                    app.theme.fg
                } else {
                    app.theme.line_no
                })
                .bg(app.theme.status_bg)
                .add_modifier(if last {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ));
    }
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    let fill = area.width.saturating_sub(used as u16) as usize;
    if fill > 0 {
        spans.push(Span::styled(
            " ".repeat(fill),
            Style::default().bg(app.theme.status_bg),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_tabbar(f: &mut Frame, app: &mut App, area: Rect) {
    app.tab_bar_y = area.y;
    app.tab_hit_regions.clear();

    let mut spans: Vec<Span> = Vec::new();
    let brand = " 晴 ";
    spans.push(Span::styled(
        brand,
        Style::default()
            .fg(app.theme.mode_normal)
            .bg(app.theme.status_bg)
            .add_modifier(Modifier::BOLD),
    ));
    let mut x = area.x + brand.width() as u16;

    for (i, tab) in app.buffers.iter().enumerate() {
        let is_current = i == app.current_buffer;
        // Current tab reflects live App state (tab copy is only synced on switch).
        let filename = if is_current {
            app.filename.as_ref()
        } else {
            tab.filename.as_ref()
        };
        let name = filename
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("[No Name]");
        let dirty = if is_current {
            app.modified
        } else {
            tab.modified
        };
        let mark = if dirty { " ●" } else { "" };
        let label = format!(" {}{} ", name, mark);
        let marker_w: u16 = if is_current { 1 } else { 0 };
        let w = label.width() as u16 + marker_w;
        app.tab_hit_regions.push((x, x + w, i));
        x += w + 1; // + separator

        if is_current {
            // Accent marker + editor-bg "lifted" look for the active tab.
            spans.push(Span::styled(
                "▎",
                Style::default()
                    .fg(app.theme.mode_normal)
                    .bg(app.theme.editor_bg),
            ));
            spans.push(Span::styled(
                label,
                Style::default()
                    .fg(app.theme.fg)
                    .bg(app.theme.editor_bg)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                label,
                Style::default().fg(app.theme.line_no).bg(app.theme.status_bg),
            ));
        }
        spans.push(Span::styled(
            "│",
            Style::default().fg(app.theme.border).bg(app.theme.status_bg),
        ));
    }

    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    let fill = area.width.saturating_sub(used as u16) as usize;
    if fill > 0 {
        spans.push(Span::styled(
            " ".repeat(fill),
            Style::default().bg(app.theme.status_bg),
        ));
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(app.theme.status_bg)),
        area,
    );
}

/// Draw single editor or split panes into `area`.
fn draw_editor_split_or_single(f: &mut Frame, app: &mut App, area: Rect) {
    app.pane_hit_regions.clear();
    app.split_sep_hit = None;
    let preview_active = app.preview.open && app.mode == Mode::Preview;
    if !app.split.is_split() {
        // Bound pane terminal with no split left → fill the whole area.
        if preview_active {
            draw_preview_pane(f, app, area);
        } else if app.terminal.open && app.terminal.full_panel {
            draw_terminal(f, app, area);
        } else {
            draw_editor_with_blame(f, app, area);
        }
        app.pane_hit_regions
            .push((area.x, area.y, area.width, area.height, 0));
        return;
    }

    use xei_core::split::SplitKind;
    let n = app.split.panes.len().max(2);
    let ratio = app.split.ratio;
    let vertical = app.split.kind == SplitKind::Vertical;
    if app.split.kind == SplitKind::None {
        draw_editor(f, app, area);
        return;
    }
    // 2 panes keep the draggable ratio; ≥3 panes split evenly.
    let constraints: Vec<Constraint> = if n == 2 {
        if vertical {
            let w = area.width;
            let left = ((w as f32) * ratio).round() as u16;
            let left = left.clamp(12, w.saturating_sub(12).max(12));
            vec![Constraint::Length(left), Constraint::Min(8)]
        } else {
            let h = area.height;
            let top = ((h as f32) * ratio).round() as u16;
            let top = top.clamp(4, h.saturating_sub(4).max(4));
            vec![Constraint::Length(top), Constraint::Min(3)]
        }
    } else {
        vec![Constraint::Ratio(1, n as u32); n]
    };
    let chunks = Layout::default()
        .direction(if vertical {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints(constraints)
        .split(area);

    // Record divider for mouse drag-resize (2-pane layout only).
    if n == 2 {
        app.split_sep_hit = Some(xei_core::SplitSepHit {
            vertical,
            pos: if vertical {
                chunks[0].x.saturating_add(chunks[0].width)
            } else {
                chunks[0].y.saturating_add(chunks[0].height)
            },
            area_x: area.x,
            area_y: area.y,
            area_w: area.width,
            area_h: area.height,
        });
    }

    // Draw unfocused panes from tab snapshots, the focused one with the live
    // buffer. Ctrl+Shift+T binds to one pane only — the rest stay editors.
    let focus = app.split.focus.min(n.saturating_sub(1));
    let term_pane = if app.terminal.open && app.terminal.full_panel {
        app.terminal.pane_bound
    } else {
        None
    };
    let pane_rects: Vec<(usize, Rect)> =
        (0..n).map(|i| (i, chunks[i.min(chunks.len() - 1)])).collect();
    for (idx, rect) in pane_rects {
        app.pane_hit_regions
            .push((rect.x, rect.y, rect.width, rect.height, idx));
        let focused = idx == focus;
        let as_term = term_pane == Some(idx);
        let term_black = Color::Rgb(0, 0, 0);
        if focused {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if as_term {
                    app.theme.success
                } else {
                    app.theme.mode_settings
                }))
                .border_type(BorderType::Plain)
                .style(Style::default().bg(if as_term {
                    term_black
                } else {
                    app.theme.editor_bg
                }));
            let inner = block.inner(rect);
            f.render_widget(block, rect);
            if as_term {
                draw_terminal(f, app, inner);
            } else if preview_active {
                // Preview transforms in-pane — only the focused pane, so the
                // split survives entering pretty mode.
                draw_preview_pane(f, app, inner);
            } else {
                draw_editor(f, app, inner);
            }
        } else {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if as_term {
                    Color::Rgb(60, 120, 100)
                } else {
                    app.theme.line_no
                }))
                .style(Style::default().bg(if as_term {
                    term_black
                } else {
                    app.theme.editor_bg
                }));
            let inner = block.inner(rect);
            f.render_widget(block, rect);
            if as_term {
                draw_terminal(f, app, inner);
            } else {
                draw_editor_inactive_pane(f, app, inner, idx);
            }
        }
    }
}

/// Render a non-focused split pane from its tab snapshot (no live cursor).
fn draw_editor_inactive_pane(f: &mut Frame, app: &App, area: Rect, pane_idx: usize) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(pane) = app.split.panes.get(pane_idx) else {
        return;
    };
    let tab = match app.buffers.get(pane.tab_index) {
        Some(t) => t,
        None => return,
    };
    let scroll = pane.scroll;
    let all_lines = tab.buffer.lines();
    let content_width = area.width.max(1) as usize;
    let gutter_w = LINE_NO_WIDTH as usize;
    let text_width = content_width.saturating_sub(gutter_w).max(1);
    let visible_height = area.height as usize;

    let mut lines: Vec<Line> = Vec::new();
    let mut used = 0usize;
    for (idx, text) in all_lines.iter().enumerate().skip(scroll) {
        let vis = inactive_visual_width(text);
        let rows = if !app.wrap_lines || vis == 0 {
            1
        } else {
            (vis + text_width - 1) / text_width
        };
        if used + rows > visible_height {
            break;
        }
        for seg in 0..rows {
            if used + seg >= visible_height {
                break;
            }
            let v0 = seg * text_width;
            let v1 = (v0 + text_width).min(vis.max(1));
            let num = if seg == 0 {
                format!("{:>3}", idx + 1)
            } else {
                "   ".into()
            };
            let gutter = Span::styled(
                format!(" {} ", num),
                Style::default().fg(app.theme.line_no).bg(app.theme.editor_bg),
            );
            let slice = visual_slice_plain(text, v0, v1);
            let body = Span::styled(
                if slice.is_empty() { " ".into() } else { slice },
                Style::default().fg(app.theme.fg).bg(app.theme.editor_bg),
            );
            lines.push(Line::from(vec![gutter, body]));
        }
        used += rows.max(1);
    }
    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.editor_bg)),
        area,
    );
}

fn inactive_visual_width(text: &str) -> usize {
    let mut vis = 0usize;
    for ch in text.chars() {
        vis += if ch == '\t' {
            4 - (vis % 4)
        } else {
            UnicodeWidthChar::width(ch).unwrap_or(1)
        };
    }
    vis
}

/// Plain text slice for visual columns [v0, v1) (tabs → spaces).
fn visual_slice_plain(text: &str, v0: usize, v1: usize) -> String {
    if v0 >= v1 {
        return String::new();
    }
    let mut out = String::new();
    let mut vis = 0usize;
    for ch in text.chars() {
        let w = if ch == '\t' {
            4 - (vis % 4)
        } else {
            UnicodeWidthChar::width(ch).unwrap_or(1)
        };
        let end = vis + w;
        if end <= v0 {
            vis = end;
            continue;
        }
        if vis >= v1 {
            break;
        }
        if ch == '\t' {
            let from = v0.saturating_sub(vis);
            let to = v1.min(end) - vis;
            for _ in from..to {
                out.push(' ');
            }
        } else if vis >= v0 && end <= v1 {
            out.push(ch);
        } else {
            // Wide char straddling boundary — pad with spaces
            let from = v0.saturating_sub(vis);
            let to = v1.min(end) - vis;
            for _ in from..to {
                out.push(' ');
            }
        }
        vis = end;
    }
    out
}

fn draw_editor(f: &mut Frame, app: &mut App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Soft left gutter + text, no heavy box border (modern flat look)
    f.render_widget(
        Block::default().style(Style::default().bg(app.theme.editor_bg)),
        area,
    );

    // Fresh start (no file, nothing typed) → shade-art welcome screen.
    let show_welcome = app.filename.is_none()
        && !app.modified
        && app.buffers.len() == 1
        && app.buffer.line_count() == 1
        && app.buffer.line(0).is_empty();
    if show_welcome {
        app.screen_row_to_buffer.clear();
        app.screen_row_visual_base.clear();
        app.screen_row_to_buffer.push(0);
        app.screen_row_visual_base.push(0);
        draw_welcome(f, app, area);
        f.set_cursor_position((area.x + LINE_NO_WIDTH, area.y));
        return;
    }

    let visible_height = area.height as usize;
    let scroll = app.scroll;
    let all_lines = app.buffer.lines();
    let selection = app.selected_range();
    let ext = app.file_extension();
    let cursor_row = app.buffer.cursor().row;

    // Gutter + text geometry — soft-wrap uses text-only width.
    let content_width = area.width.max(1) as usize;
    let gutter_w = LINE_NO_WIDTH as usize;
    let text_width = content_width.saturating_sub(gutter_w).max(1);

    // Horizontal-scroll mode: keep the cursor inside the pan window no matter
    // which code path moved it (h/l/$/search don't call update_scroll).
    if !app.wrap_lines {
        let sc = app
            .buffer
            .buffer_col_to_screen_col(cursor_row, app.buffer.cursor().col);
        if sc < app.hscroll {
            app.hscroll = sc;
        } else if sc >= app.hscroll + text_width {
            app.hscroll = sc + 1 - text_width;
        }
    } else if app.hscroll != 0 {
        app.hscroll = 0;
    }

    // Rebuild screen-row maps (must match rendered segments exactly).
    app.screen_row_to_buffer.clear();
    app.screen_row_visual_base.clear();

    let mut screen_lines: Vec<Line> = Vec::new();
    let mut wrap_height = 0usize;

    for (idx, text) in all_lines.iter().enumerate().skip(scroll) {
        // Skip lines hidden by closed folds
        if app.folds.is_hidden(idx) {
            continue;
        }
        let fold_closed = app.folds.is_closed(idx);
        let fold_count = app.folds.closed_count(idx);
        let vis = visual_line_width(app, idx);
        let rows = if !app.wrap_lines || vis == 0 {
            1
        } else {
            (vis + text_width - 1) / text_width
        };
        if wrap_height >= visible_height {
            break;
        }
        let is_cursor_line = idx == cursor_row;
        let path_s = app
            .filename
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let has_bp = !path_s.is_empty() && app.dap.has_breakpoint(&path_s, idx);
        let is_debug_line = !path_s.is_empty()
            && app.dap.current_line_for(&path_s) == Some(idx);
        let git_sign = app.git.sign_at(idx);
        // BP + stopped → ◉; else ● / ▶; else git/fold marks
        let git_ch = if has_bp && is_debug_line {
            '◉'
        } else if has_bp {
            '●'
        } else if is_debug_line {
            '▶'
        } else {
            match git_sign {
                Some(xei_core::git::GitSign::Added) => '+',
                Some(xei_core::git::GitSign::Modified) => '~',
                Some(xei_core::git::GitSign::Deleted) => '▁',
                None => {
                    if fold_closed {
                        '▸'
                    } else if app.folds.fold_at(idx).is_some() {
                        '▾'
                    } else {
                        ' '
                    }
                }
            }
        };
        let git_color = if has_bp && is_debug_line {
            app.theme.warning
        } else if has_bp {
            app.theme.error
        } else if is_debug_line {
            app.theme.warning
        } else {
            match git_sign {
                Some(xei_core::git::GitSign::Added) => app.theme.success,
                Some(xei_core::git::GitSign::Modified) => app.theme.warning,
                Some(xei_core::git::GitSign::Deleted) => app.theme.error,
                None => {
                    if fold_closed {
                        app.theme.accent
                    } else if is_cursor_line {
                        app.theme.fg
                    } else {
                        app.theme.line_no
                    }
                }
            }
        };
        let mut gutter_style = Style::default().fg(git_color).bg(if is_debug_line {
            app.theme.selection_bg
        } else {
            app.theme.editor_bg
        });
        if is_cursor_line || is_debug_line {
            gutter_style = gutter_style.add_modifier(Modifier::BOLD);
        }

        for seg in 0..rows {
            if wrap_height >= visible_height {
                break;
            }
            let v0 = if app.wrap_lines {
                seg * text_width
            } else {
                app.hscroll
            };
            let v1 = v0 + text_width;
            app.screen_row_to_buffer.push(idx);
            app.screen_row_visual_base.push(v0);

            let num = if seg == 0 {
                if app.relative_number && !is_cursor_line {
                    let dist = (idx as isize - cursor_row as isize).unsigned_abs();
                    format!("{:>3}", dist)
                } else {
                    format!("{:>3}", idx + 1)
                }
            } else {
                "   ".into()
            };
            let gutter = Span::styled(
                if seg == 0 {
                    format!("{git_ch}{num} ") // always 5 cells
                } else {
                    "  ·  ".to_string()
                },
                if seg == 0 {
                    gutter_style
                } else {
                    Style::default()
                        .fg(app.theme.line_no)
                        .bg(app.theme.editor_bg)
                },
            );

            let mut styled = render_line_with_highlights(
                &idx,
                text,
                app,
                selection,
                ext.as_deref(),
                is_cursor_line,
                Some((v0, v1)),
            );
            if styled.is_empty() {
                let bg = if is_cursor_line {
                    dim_bg(app.theme.editor_bg)
                } else {
                    app.theme.editor_bg
                };
                styled.push(Span::styled(" ", Style::default().bg(bg)));
            }
            // Closed fold annotation on first segment
            if seg == 0 && fold_closed && fold_count > 0 {
                styled.push(Span::styled(
                    format!("  ⋯ {fold_count} lines"),
                    Style::default()
                        .fg(app.theme.accent)
                        .bg(app.theme.editor_bg)
                        .add_modifier(Modifier::ITALIC),
                ));
            }
            // Inline blame suffix only when panel is closed (panel is Ctrl+B)
            if seg == 0 && app.blame.enabled && !app.blame.visible() {
                if let Some(b) = app.blame.at(idx) {
                    styled.push(Span::styled(
                        format!("  {} {}", b.author, b.hash),
                        Style::default()
                            .fg(app.theme.muted)
                            .bg(app.theme.editor_bg)
                            .add_modifier(Modifier::ITALIC),
                    ));
                }
            }
            let mut spans = vec![gutter];
            spans.extend(styled);
            if is_cursor_line {
                spans.push(Span::styled(
                    " ",
                    Style::default().bg(dim_bg(app.theme.editor_bg)),
                ));
            }
            screen_lines.push(Line::from(spans));
            wrap_height += 1;
        }
    }

    // Soft-wrap is explicit segment Lines — do not enable Paragraph wrap.
    let paragraph = Paragraph::new(screen_lines).style(Style::default().bg(app.theme.editor_bg));
    f.render_widget(paragraph, area);

    // Secondary multi-cursors as reverse-video cells
    if app.multi.is_active() {
        draw_extra_cursors(f, app, area, text_width);
    }

    let cursor = app.buffer.cursor();
    let screen_col = app.buffer.buffer_col_to_screen_col(cursor.row, cursor.col);

    let mut display_row = 0usize;
    let mut found = false;
    for row in scroll..app.buffer.line_count() {
        let visual_len = visual_line_width(app, row);
        let line_rows = if !app.wrap_lines || visual_len == 0 {
            1
        } else {
            (visual_len + text_width - 1) / text_width
        };
        if row == cursor.row {
            let wrap_row = if app.wrap_lines {
                screen_col / text_width
            } else {
                0
            };
            display_row += wrap_row;
            found = true;
            break;
        }
        display_row += line_rows;
    }

    // Don't move the terminal cursor while Search / XLC owns the input line.
    if matches!(
        app.mode,
        Mode::Search
            | Mode::XlcInput
            | Mode::Palette
            | Mode::SourceControl
            | Mode::GitWorkbench
            | Mode::Settings
            | Mode::WorkspaceSearch
    ) || app.peek.open
    {
        return;
    }

    if found && display_row < area.height as usize {
        let col_in_view = if app.wrap_lines {
            screen_col % text_width
        } else {
            screen_col.saturating_sub(app.hscroll)
        };
        let cursor_x = (area.x + LINE_NO_WIDTH + col_in_view as u16)
            .min(area.x + area.width.saturating_sub(1));
        let cursor_y = (area.y + display_row as u16).min(area.y + area.height.saturating_sub(1));
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn dim_bg(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_add(14),
            g.saturating_add(14),
            b.saturating_add(20),
        ),
        other => other,
    }
}

/// Diagnostic underline: colored undercurl-style when `gpu_acc` + terminal caps.
fn diag_underline_style(
    app: &App,
    base: Style,
    severity: &xei_core::lsp::DiagnosticSeverity,
) -> Style {
    let (fg, ul) = match severity {
        xei_core::lsp::DiagnosticSeverity::Error => {
            (Color::Rgb(255, 100, 100), Color::Rgb(255, 80, 80))
        }
        xei_core::lsp::DiagnosticSeverity::Warning => {
            (Color::Rgb(255, 200, 80), Color::Rgb(230, 180, 60))
        }
        xei_core::lsp::DiagnosticSeverity::Info => {
            (Color::Rgb(120, 180, 255), Color::Rgb(100, 160, 240))
        }
        xei_core::lsp::DiagnosticSeverity::Hint => {
            (Color::Rgb(140, 200, 160), Color::Rgb(120, 180, 140))
        }
    };
    let rich = app.gpu_acc && (app.term_underline_color || app.term_modern || app.term_undercurl);
    if rich {
        // Per-span colored underline only (never sticky CSI 4:3 — that waves
        // the whole screen on Ghostty/Kitty).
        base.fg(fg)
            .add_modifier(Modifier::UNDERLINED)
            .underline_color(ul)
    } else {
        base.fg(fg).add_modifier(Modifier::UNDERLINED)
    }
}

/// Centered shade-art logo + key hints for an empty, fileless session.
fn draw_welcome(f: &mut Frame, app: &App, area: Rect) {
    let accent = app.theme.mode_normal;
    let dim = app.theme.line_no;
    let mut rows: Vec<(String, Color, bool)> = vec![
        ("░ ▒ ▓ █  晴  █ ▓ ▒ ░".into(), accent, true),
        ("".into(), dim, false),
        ("x  e  i".into(), app.theme.fg, true),
        (concat!("v", env!("CARGO_PKG_VERSION")).into(), dim, false),
        ("".into(), dim, false),
        ("i insert · Ctrl+P files · :help commands".into(), dim, false),
        ("Ctrl+G source control · Ctrl+T terminal".into(), dim, false),
    ];
    // Newer release found by the async check → offer the in-place update.
    if app.update.installed {
        rows.push(("".into(), dim, false));
        rows.push((
            "✓ updated — restart xei to finish".into(),
            app.theme.success,
            true,
        ));
    } else if let Some(ref v) = app.update.latest {
        rows.push(("".into(), dim, false));
        rows.push((
            format!("⬆ v{v} available — :update to install · update_check=false to hide"),
            app.theme.warning,
            true,
        ));
    }
    let y0 = area.y + (area.height / 2).saturating_sub(rows.len() as u16 / 2 + 1);
    for (i, (text, color, bold)) in rows.iter().enumerate() {
        let y = y0 + i as u16;
        if y >= area.y + area.height {
            break;
        }
        let mut style = Style::default().fg(*color).bg(app.theme.editor_bg);
        if *bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text.clone(), style)))
                .alignment(Alignment::Center),
            Rect::new(area.x, y, area.width, 1),
        );
    }
}

/// VS Code-ish per-filetype tint for explorer file names.
fn file_type_color(name: &str, fallback: Color) -> Color {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "rs" => Color::Rgb(255, 160, 100),
        "py" | "pyi" => Color::Rgb(120, 180, 255),
        "js" | "jsx" | "mjs" | "cjs" => Color::Rgb(240, 220, 130),
        "ts" | "tsx" => Color::Rgb(100, 160, 250),
        "go" => Color::Rgb(110, 200, 230),
        "c" | "h" | "cpp" | "hpp" | "cc" | "hh" => Color::Rgb(170, 180, 255),
        "md" | "mdx" => Color::Rgb(150, 200, 255),
        "json" | "toml" | "yaml" | "yml" | "lock" => Color::Rgb(190, 185, 140),
        "sh" | "bash" | "zsh" => Color::Rgb(150, 220, 150),
        "html" | "htm" => Color::Rgb(235, 145, 110),
        "css" | "scss" | "less" => Color::Rgb(150, 160, 250),
        _ => fallback,
    }
}

fn visual_line_width(app: &App, row: usize) -> usize {
    let line = app.buffer.line(row);
    let mut vis = 0;
    for ch in line.chars() {
        vis += if ch == '\t' {
            4 - (vis % 4)
        } else {
            UnicodeWidthChar::width(ch).unwrap_or(1)
        };
    }
    vis
}

fn render_line_with_highlights(
    row: &usize,
    text: &str,
    app: &App,
    selection: Option<(xei_core::buffer::Position, xei_core::buffer::Position)>,
    ext: Option<&str>,
    is_cursor_line: bool,
    // Soft-wrap slice: only emit visual columns in [start, end).
    vis_range: Option<(usize, usize)>,
) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let line_bg = if is_cursor_line {
        dim_bg(app.theme.editor_bg)
    } else {
        app.theme.editor_bg
    };

    if len == 0 {
        return vec![Span::styled(" ", Style::default().bg(line_bg))];
    }

    // Quality stack: LSP semantic tokens > tree-sitter query > line fallback
    let semantic: Vec<&(highlight::TokenKind, usize, usize, usize)> = app
        .lsp
        .semantic_tokens
        .iter()
        .filter(|(_, _, _, r)| *r == *row)
        .collect();
    let hl_tokens: Vec<&(highlight::TokenKind, usize, usize, usize)> = app
        .syntax
        .tokens
        .iter()
        .filter(|(_, _, _, r)| *r == *row)
        .collect();
    let fallback = highlight::highlight_line(text, ext);
    let row_diags: Vec<&xei_core::lsp::Diagnostic> = app
        .lsp
        .diagnostics
        .iter()
        .filter(|d| d.row == *row)
        .collect();

    let visual_style = Style::default()
        .bg(app.theme.selection_bg)
        .add_modifier(Modifier::BOLD);
    let search_style = Style::default()
        .bg(app.theme.search_bg)
        .fg(app.theme.fg);
    let search_current_style = Style::default()
        .bg(Color::Rgb(220, 160, 60))
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let pattern_chars = app.search_pattern_len_chars();

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut vis_col = 0usize;
    let mut run_style = Style::default().fg(app.theme.fg).bg(line_bg);
    let mut run_text = String::new();

    // Inlays only on the first wrap segment (vis starts at 0) — they don't
    // participate in wrap geometry (`visual_line_width` ignores them).
    let show_inlays = matches!(vis_range, None | Some((0, _)));
    let inlays: Vec<&xei_core::lsp::InlayHint> = if app.inlay_hints_enabled && show_inlays {
        app.lsp
            .inlay_hints
            .iter()
            .filter(|h| h.row == *row)
            .collect()
    } else {
        Vec::new()
    };
    let inlay_style = Style::default()
        .fg(app.theme.muted)
        .bg(line_bg)
        .add_modifier(Modifier::ITALIC);

    for i in 0..len {
        // Virtual inlay text before this char (does not affect buffer columns)
        for h in inlays.iter().filter(|h| h.col == i) {
            if !run_text.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut run_text), run_style));
            }
            let label = if h.label.starts_with(':') || h.label.starts_with(' ') {
                h.label.clone()
            } else {
                format!(":{}", h.label)
            };
            spans.push(Span::styled(label, inlay_style));
        }

        let ch = chars[i];
        let full_tab = if ch == '\t' {
            4 - (vis_col % 4)
        } else {
            0
        };
        let char_width = if ch == '\t' {
            full_tab
        } else {
            UnicodeWidthChar::width(ch).unwrap_or(1)
        };
        let char_vis_end = vis_col + char_width;

        // Soft-wrap clip
        if let Some((rs, re)) = vis_range {
            if char_vis_end <= rs {
                vis_col = char_vis_end;
                continue;
            }
            if vis_col >= re {
                break;
            }
        }

        let char_str = if ch == '\t' {
            if let Some((rs, re)) = vis_range {
                let from = rs.saturating_sub(vis_col);
                let to = re.min(char_vis_end) - vis_col;
                " ".repeat(to.saturating_sub(from).max(0))
            } else {
                " ".repeat(full_tab)
            }
        } else if let Some((rs, re)) = vis_range {
            // Wide glyph straddling slice boundary → spaces of overlapping width
            if vis_col < rs || char_vis_end > re {
                let from = rs.saturating_sub(vis_col);
                let to = re.min(char_vis_end) - vis_col;
                " ".repeat(to.saturating_sub(from).max(1))
            } else {
                ch.to_string()
            }
        } else {
            ch.to_string()
        };

        if char_str.is_empty() {
            vis_col = char_vis_end;
            continue;
        }

        let mut s = Style::default().fg(app.theme.fg).bg(line_bg);

        // 1) Tightest semantic token wins (LSP-accurate kinds)
        let mut best_sem: Option<(usize, highlight::TokenKind)> = None;
        for (kind, st, ed, _) in &semantic {
            if i >= *st && i < *ed {
                let w = ed.saturating_sub(*st);
                if best_sem.map(|(bw, _)| w < bw).unwrap_or(true) {
                    best_sem = Some((w, *kind));
                }
            }
        }
        if let Some((_, kind)) = best_sem {
            s = highlight::style_for(app.theme, kind).bg(line_bg);
        } else {
            // 2) Tightest tree-sitter query capture
            let mut best: Option<(usize, highlight::TokenKind)> = None;
            for (kind, st, ed, _) in &hl_tokens {
                if i >= *st && i < *ed {
                    let w = ed.saturating_sub(*st);
                    if best.map(|(bw, _)| w < bw).unwrap_or(true) {
                        best = Some((w, *kind));
                    }
                }
            }
            if let Some((_, kind)) = best {
                s = highlight::style_for(app.theme, kind).bg(line_bg);
            } else {
                // 3) Line tokenizer fills gaps for unsupported grammars / anonymous nodes
                for &(kind, st, ed) in &fallback {
                    if i >= st && i < ed {
                        s = highlight::style_for(app.theme, kind).bg(line_bg);
                        break;
                    }
                }
            }
        }

        if app.mode == Mode::VisualBlock {
            if let Some((r0, r1, c0, c1)) = app.block_range() {
                if *row >= r0 && *row <= r1 && i >= c0 && i <= c1 {
                    s = visual_style;
                }
            }
        } else if let Some((start, end)) = selection {
            if app.mode == Mode::VisualLine {
                if *row >= start.row && *row <= end.row {
                    s = visual_style;
                }
            } else if *row >= start.row && *row <= end.row {
                let ls = if *row == start.row { start.col } else { 0 };
                // end.col is inclusive
                let le = if *row == end.row {
                    (end.col + 1).min(len)
                } else {
                    len
                };
                if i >= ls && i < le {
                    s = visual_style;
                }
            }
        }

        if pattern_chars > 0 && app.active_search_pattern().is_some() {
            for (mi, pos) in app.search_matches.iter().enumerate() {
                if pos.row == *row && i >= pos.col && i < pos.col + pattern_chars {
                    s = if mi == app.search_current {
                        search_current_style
                    } else {
                        search_style
                    };
                    break;
                }
            }
        }

        for diag in &row_diags {
            if i >= diag.col_start && i < diag.col_end {
                s = diag_underline_style(app, s, &diag.severity);
                break;
            }
        }

        if s != run_style && !run_text.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut run_text), run_style));
            run_style = s;
        } else if run_style != s {
            run_style = s;
        }
        run_text.push_str(&char_str);
        vis_col = char_vis_end;
    }

    if !run_text.is_empty() {
        spans.push(Span::styled(run_text, run_style));
    }
    // Trailing inlays (e.g. type annotations at EOL) — first segment only
    if show_inlays {
        for h in inlays.iter().filter(|h| h.col >= len) {
            let label = if h.label.starts_with(':') || h.label.starts_with(' ') {
                h.label.clone()
            } else {
                format!(":{}", h.label)
            };
            spans.push(Span::styled(label, inlay_style));
        }
    }

    // Code lens virtual text at EOL (first wrap segment)
    if app.code_lens_enabled && show_inlays {
        let lens_style = Style::default()
            .fg(Color::Rgb(130, 140, 180))
            .bg(line_bg)
            .add_modifier(Modifier::ITALIC);
        for lens in app.lsp.code_lenses.iter().filter(|l| l.row == *row) {
            spans.push(Span::styled(
                format!("  ○ {}", lens.title),
                lens_style,
            ));
        }
    }

    spans
}

fn draw_peek(f: &mut Frame, app: &App, area: Rect) {
    if !app.peek.open || app.peek.lines.is_empty() {
        return;
    }
    let width = area.width.saturating_sub(6).min(72).max(40);
    let height = (app.peek.lines.len() as u16 + 3).min(area.height.saturating_sub(4)).max(8);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + 2;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };
    // Soft cell shadow (always); Kitty graphics shadow is optional post-frame.
    if app.gpu_acc {
        f.render_widget(
            Block::default().style(Style::default().bg(Color::Black)),
            Rect {
                x: x.saturating_add(1),
                y: y.saturating_add(1),
                width,
                height: height.saturating_sub(1),
            },
        );
    }
    f.render_widget(Clear, popup);
    // Path shown in title; with hyperlinks caps, terminal may still parse
    // file:// from the message bar — title stays plain cells for layout safety.
    let path_disp = truncate_path(&app.peek.path.display().to_string(), 40);
    let title = format!(
        " Peek · {} · L{}{} ",
        path_disp,
        app.peek.target_row + 1,
        if app.gpu_acc && app.term_hyperlinks {
            " ↗"
        } else {
            ""
        }
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.mode_settings))
        .style(Style::default().bg(app.theme.completion_bg).fg(app.theme.fg))
        .title(Span::styled(
            title,
            Style::default()
                .fg(app.theme.mode_settings)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    let view_h = inner.height.saturating_sub(1) as usize;
    let start = app.peek.scroll;
    for (i, text) in app
        .peek
        .lines
        .iter()
        .enumerate()
        .skip(start)
        .take(view_h)
    {
        let abs_row = app.peek.base_row + i;
        let is_target = abs_row == app.peek.target_row;
        let num = format!("{:>4} ", abs_row + 1);
        let bg = if is_target {
            app.theme.completion_selected
        } else {
            app.theme.completion_bg
        };
        let fg = if is_target {
            Color::Black
        } else {
            app.theme.fg
        };
        lines.push(Line::from(vec![
            Span::styled(
                num,
                Style::default().fg(app.theme.line_no).bg(bg),
            ),
            Span::styled(text.clone(), Style::default().fg(fg).bg(bg)),
        ]));
    }
    lines.push(Line::from(Span::styled(
        " Enter open · Esc dismiss · j/k scroll",
        Style::default().fg(app.theme.line_no),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_workspace_search(f: &mut Frame, app: &mut App, area: Rect) {
    let ws = &app.workspace_search;
    let width = area.width.saturating_sub(4).min(90).max(50);
    let height = area.height.saturating_sub(2).min(24).max(12);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.accent))
        .style(Style::default().bg(app.theme.completion_bg).fg(app.theme.fg))
        .title(Span::styled(
            " Find in files ",
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // query
            Constraint::Length(1), // replace
            Constraint::Length(1), // status
            Constraint::Min(4),    // hits
            Constraint::Length(1), // hints
        ])
        .split(inner);

    let q_focus = !ws.replace_focus;
    let q_style = if q_focus {
        Style::default()
            .fg(Color::Black)
            .bg(app.theme.accent)
    } else {
        Style::default().fg(app.theme.fg)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Find: ", Style::default().fg(app.theme.line_no)),
            Span::styled(format!("{}▌", ws.query), q_style),
        ])),
        chunks[0],
    );
    let r_style = if ws.replace_focus {
        Style::default()
            .fg(Color::Black)
            .bg(app.theme.accent)
    } else {
        Style::default().fg(app.theme.fg)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Repl: ", Style::default().fg(app.theme.line_no)),
            Span::styled(format!("{}▌", ws.replace), r_style),
        ])),
        chunks[1],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" {}", ws.status),
            Style::default().fg(Color::Rgb(160, 170, 190)),
        )),
        chunks[2],
    );

    let list_h = chunks[3].height as usize;
    let mut list_lines = Vec::new();
    // Keep selection in view
    let sel = ws.selected;
    let scroll = if sel >= list_h {
        sel + 1 - list_h
    } else {
        0
    };
    // store scroll? we compute locally
    for (i, hit) in ws.hits.iter().enumerate().skip(scroll).take(list_h) {
        let sel_here = i == sel;
        let bg = if sel_here {
            app.theme.completion_selected
        } else {
            app.theme.completion_bg
        };
        let fg = if sel_here {
            Color::Black
        } else {
            app.theme.fg
        };
        let rel = hit
            .path
            .strip_prefix(&ws.root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| hit.path.display().to_string());
        let snippet: String = hit.line.chars().take(60).collect();
        let label = format!(" {}:{}  {}", rel, hit.row + 1, snippet);
        list_lines.push(Line::from(Span::styled(
            label,
            Style::default().fg(fg).bg(bg),
        )));
    }
    if list_lines.is_empty() {
        list_lines.push(Line::from(Span::styled(
            "  (no results)",
            Style::default().fg(app.theme.line_no),
        )));
    }
    f.render_widget(Paragraph::new(list_lines), chunks[3]);
    f.render_widget(
        Paragraph::new(Span::styled(
            " Tab field · ↑↓ · Enter open · r replace one · R all · Esc",
            Style::default().fg(app.theme.line_no),
        )),
        chunks[4],
    );
}

fn draw_explorer(f: &mut Frame, app: &mut App, area: Rect) {
    let cwd_display = app.explorer.cwd.display().to_string();
    let title = truncate_path(&cwd_display, area.width.saturating_sub(4) as usize);

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(app.theme.border))
        .border_type(BorderType::Plain)
        .style(Style::default().bg(app.theme.explorer_bg))
        .title(Span::styled(
            format!(" {} ", title),
            Style::default()
                .fg(app.theme.explorer_dir)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let visible_height = inner.height as usize;
    app.explorer.ensure_visible(visible_height);

    let entries = &app.explorer.entries;
    let scroll = app.explorer.scroll;
    let selected = app.explorer.selected;

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(idx, entry)| {
            let icon = if entry.name == ".." {
                "↩"
            } else if entry.is_dir {
                "▸"
            } else {
                "·"
            };
            let is_selected = idx == selected;
            let style = if is_selected {
                Style::default()
                    .fg(app.theme.explorer_bg)
                    .bg(app.theme.explorer_selected)
                    .add_modifier(Modifier::BOLD)
            } else if entry.is_dir {
                Style::default().fg(app.theme.explorer_dir)
            } else {
                Style::default().fg(file_type_color(&entry.name, app.theme.explorer_fg))
            };
            let max_name = inner.width.saturating_sub(4) as usize;
            let name = if entry.name.chars().count() > max_name {
                let truncated: String = entry.name.chars().take(max_name.saturating_sub(1)).collect();
                format!("{}…", truncated)
            } else {
                entry.name.clone()
            };
            let label = format!(" {} {} ", icon, name);
            ListItem::new(label).style(style)
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

fn truncate_path(path: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let w = path.width();
    if w <= max {
        return path.to_string();
    }
    let keep = max.saturating_sub(1);
    // keep rightmost segment
    let mut acc = String::new();
    for ch in path.chars().rev() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(1);
        if acc.width() + cw > keep {
            break;
        }
        acc.insert(0, ch);
    }
    format!("…{}", acc)
}

fn draw_pending_hints(f: &mut Frame, app: &App, area: Rect) {
    let hints = &app.pending_hints;
    if hints.is_empty() {
        return;
    }
    // Prefer bottom-right panel for Space leader (many entries); cursor-adjacent for short chords
    let is_leader = app.which_key.is_leader();
    let count = hints.len() as u16;
    let key_w = hints
        .iter()
        .map(|(k, _)| k.len())
        .max()
        .unwrap_or(4)
        .max(2);
    let desc_w = hints
        .iter()
        .map(|(_, d)| d.len())
        .max()
        .unwrap_or(12)
        .min(28);
    let popup_w = ((key_w + desc_w + 6) as u16)
        .min(area.width.saturating_sub(2))
        .max(28);
    let popup_h = (count + 2).min(area.height.saturating_sub(1)).max(3);

    let (cx, cy) = if is_leader {
        // Bottom-right which-key style
        (
            area.x + area.width.saturating_sub(popup_w + 2),
            area.y + area.height.saturating_sub(popup_h + 1),
        )
    } else {
        let vp = app.viewport;
        if vp.width > 0
            && matches!(
                app.mode,
                Mode::Normal | Mode::Insert | Mode::Visual | Mode::VisualLine | Mode::VisualBlock
            )
        {
            let cursor = app.buffer.cursor();
            let screen_col = app.buffer.buffer_col_to_screen_col(cursor.row, cursor.col);
            let cx = (vp.x + LINE_NO_WIDTH + screen_col as u16)
                .min(area.x + area.width.saturating_sub(popup_w));
            let cy = (vp.y + (cursor.row.saturating_sub(app.scroll)) as u16 + 1)
                .min(area.y + area.height.saturating_sub(popup_h));
            (cx, cy)
        } else {
            (
                area.x.saturating_add(2),
                area.y + area.height.saturating_sub(popup_h + 2),
            )
        }
    };

    let popup = Rect {
        x: cx,
        y: cy,
        width: popup_w,
        height: popup_h,
    };

    let shadow = Rect {
        x: cx.saturating_add(1),
        y: cy.saturating_add(1),
        width: popup_w,
        height: popup_h,
    };
    f.render_widget(Block::default().style(Style::default().bg(Color::Black)), shadow);

    let glass_bg = Color::Rgb(28, 30, 42);
    let items: Vec<Line> = hints
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!(" {key:<kw$} ", kw = key_w),
                    Style::default()
                        .fg(Color::Rgb(255, 200, 90))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    desc.to_string(),
                    Style::default().fg(Color::Rgb(180, 185, 200)),
                ),
            ])
        })
        .collect();

    let title = if app.which_key.title.is_empty() {
        " which-key ".to_string()
    } else {
        format!(" {} ", app.which_key.title)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(90, 100, 140)))
        .style(Style::default().bg(glass_bg))
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Rgb(255, 180, 80))
                .add_modifier(Modifier::BOLD),
        ));

    f.render_widget(Clear, popup);
    f.render_widget(Paragraph::new(items).block(block), popup);
}

fn draw_completions(f: &mut Frame, app: &App, area: Rect) {
    let suggestions = &app.completions.suggestions;
    if suggestions.is_empty() {
        return;
    }

    let max_height = (suggestions.len() as u16).min(12);
    let max_label = suggestions.iter().map(|s| s.label.len()).max().unwrap_or(0);
    let max_detail = suggestions.iter().map(|s| s.detail.len()).max().unwrap_or(0);
    let popup_width = (max_label + max_detail + 6).min(60).max(18) as u16;

    let vp = app.viewport;
    let cursor = app.buffer.cursor();
    let screen_col = app.buffer.buffer_col_to_screen_col(cursor.row, cursor.col);
    let cursor_x = vp.x + LINE_NO_WIDTH + screen_col as u16;
    let cursor_y = vp.y + (cursor.row.saturating_sub(app.scroll)) as u16;

    let popup_x = cursor_x.min(area.x + area.width.saturating_sub(popup_width + 2));
    let popup_y = if cursor_y + 1 + max_height < area.y + area.height {
        cursor_y + 1
    } else {
        cursor_y.saturating_sub(max_height + 1)
    };

    let popup_area = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height: max_height + 2,
    };

    let visible_start = if app.completions.selected >= suggestions.len().saturating_sub(1) {
        suggestions.len().saturating_sub(max_height as usize)
    } else {
        app.completions
            .selected
            .saturating_sub((max_height as usize) / 2)
            .min(suggestions.len().saturating_sub(max_height as usize))
    };

    let visible: Vec<(usize, &xei_core::completion::Suggestion)> = suggestions
        .iter()
        .enumerate()
        .skip(visible_start)
        .take(max_height as usize)
        .collect();

    let items: Vec<Line> = visible
        .iter()
        .map(|(idx, s)| {
            let is_selected = *idx == app.completions.selected;
            let label_style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.completion_selected)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            let detail_style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.completion_selected)
            } else {
                Style::default().fg(app.theme.line_no)
            };
            let pad = popup_width
                .saturating_sub((s.label.len() + s.detail.len() + 3) as u16) as usize;
            Line::from(vec![
                Span::styled(format!(" {} ", s.label), label_style),
                Span::styled(format!("{}{}", s.detail, " ".repeat(pad)), detail_style),
            ])
        })
        .collect();

    f.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        Rect {
            x: popup_x.saturating_add(1),
            y: popup_y.saturating_add(1),
            width: popup_width,
            height: max_height + 2,
        },
    );

    let glass_bg = app.theme.completion_bg;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.completion_border))
        .style(Style::default().bg(glass_bg))
        .title(Span::styled(
            format!(" complete · {} ", suggestions.len()),
            Style::default().fg(app.theme.line_no),
        ));

    let popup = Paragraph::new(items).block(block);

    f.render_widget(Clear, popup_area);
    f.render_widget(popup, popup_area);
}

fn draw_xlc(f: &mut Frame, app: &App, area: Rect) {
    let border_color = app.theme.xlc_border;

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(app.theme.xlc_bg))
        .title(Span::styled(
            " command ",
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let xlc_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let output_height = xlc_chunks[0].height as usize;
    let scroll = app.xlc.scroll_offset;
    let output_lines: Vec<&str> = app
        .xlc
        .output
        .iter()
        .rev()
        .skip(scroll)
        .take(output_height)
        .rev()
        .map(|s| s.as_str())
        .collect();

    let output_widget = Paragraph::new(
        output_lines
            .iter()
            .map(|s| {
                let style = if s.starts_with('>') {
                    Style::default().fg(app.theme.xlc_prompt)
                } else if s.starts_with("Error") || s.contains("error") {
                    Style::default().fg(Color::Rgb(255, 120, 120))
                } else {
                    Style::default().fg(app.theme.xlc_fg)
                };
                Line::from(Span::styled(format!("  {}", s), style))
            })
            .collect::<Vec<Line>>(),
    );

    f.render_widget(output_widget, xlc_chunks[0]);

    let input_text = &app.xlc.input;
    let prompt = format!(":{}", input_text);

    let prompt_style = Style::default()
        .fg(app.theme.xlc_prompt)
        .bg(app.theme.xlc_bg)
        .add_modifier(Modifier::BOLD);

    let prompt_widget =
        Paragraph::new(Line::from(Span::styled(format!("  {}", prompt), prompt_style)));

    f.render_widget(prompt_widget, xlc_chunks[1]);

    let cursor_offset = 2 + prompt.chars().count() as u16;
    let cursor_x = (xlc_chunks[1].x + cursor_offset)
        .min(xlc_chunks[1].x + xlc_chunks[1].width.saturating_sub(1));
    let cursor_y = xlc_chunks[1].y;
    f.set_cursor_position((cursor_x, cursor_y));
}

fn draw_terminal(f: &mut Frame, app: &mut App, area: Rect) {
    app.terminal_rect = Some((area.x, area.y, area.width, area.height));
    let cwd = if let Some(ref path) = app.filename {
        path.parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "?".to_string())
    } else {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "?".to_string())
    };

    let short_cwd = truncate_path(&cwd, area.width.saturating_sub(6) as usize);

    // Side terminal: left border. Pane terminal window: fill (split chrome
    // already provides the outer frame). Confirm banner when closing.
    let title = if app.terminal.close_confirm && app.terminal.full_panel {
        format!(" terminal · close? [y/n] · {} ", short_cwd)
    } else if app.terminal.full_panel {
        format!(" terminal · ^C to shell · ^⇧W close · {} ", short_cwd)
    } else {
        format!(" term · {} ", short_cwd)
    };
    let title_style = if app.terminal.close_confirm && app.terminal.full_panel {
        Style::default()
            .fg(Color::Rgb(255, 200, 100))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(app.theme.line_no)
    };
    // Pure black canvas — agent TUIs (opencode/claude) paint on black;
    // grey theme bg made them look like a floating card.
    const TERM_BG: Color = Color::Rgb(0, 0, 0);
    const TERM_FG: Color = Color::Rgb(200, 200, 200);

    let block = if app.terminal.full_panel {
        Block::default()
            .style(Style::default().bg(TERM_BG))
            .title(Span::styled(title, title_style))
            .title_alignment(Alignment::Left)
    } else {
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(app.theme.border))
            .style(Style::default().bg(TERM_BG))
            .title(Span::styled(title, title_style))
            .title_alignment(Alignment::Left)
    };

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 1 || inner.width < 1 {
        return;
    }

    // Size first (grid + real PTY TIOCSWINSZ), then spawn so COLUMNS/LINES match.
    app.terminal.resize(inner.width, inner.height);
    if app.terminal.open && !app.terminal.started {
        let path = app.filename.clone();
        app.terminal.start(path.as_ref());
    }
    app.terminal.poll();

    let vis_height = inner.height as usize;
    let alt = app.terminal.is_alt_screen();

    let row_to_line = |row: &Vec<(String, Option<Color>, Option<Color>)>| -> Line<'static> {
        let spans: Vec<Span> = row
            .iter()
            .map(|(ch, fg, bg)| {
                let mut style = Style::default().fg(TERM_FG).bg(TERM_BG);
                if let Some(c) = fg {
                    style = style.fg(*c);
                }
                if let Some(c) = bg {
                    style = style.bg(*c);
                }
                Span::styled(ch.clone(), style)
            })
            .collect();
        Line::from(spans)
    };

    let mut lines: Vec<Line> = Vec::new();
    let live_rows = app.terminal.visible_rows();
    // Bottom-anchored view over the virtual buffer [scrollback + live grid]:
    // scroll = 0 shows the tail (history above the live prompt); wheel-up
    // slides the window up into history. Alt-screen TUIs never mix scrollback.
    let mut view_start_virtual = 0usize;
    let mut shown_scroll = 0usize;
    if alt {
        for row in live_rows.iter().take(vis_height) {
            lines.push(row_to_line(row));
        }
    } else {
        let sb_rows = app.terminal.visible_scrollback();
        let total = sb_rows.len() + live_rows.len();
        let max_scroll = total.saturating_sub(vis_height);
        let scroll = app.terminal.scroll().min(max_scroll);
        let view_start = total.saturating_sub(vis_height + scroll);
        view_start_virtual = view_start;
        shown_scroll = scroll;
        for i in view_start..total.min(view_start + vis_height) {
            let row = if i < sb_rows.len() {
                &sb_rows[i]
            } else {
                &live_rows[i - sb_rows.len()]
            };
            lines.push(row_to_line(row));
        }
    }

    // Pad remaining rows with black so the pane is fully filled
    while lines.len() < vis_height {
        lines.push(Line::from(Span::styled(
            " ".repeat(inner.width as usize),
            Style::default().bg(TERM_BG),
        )));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);

    // Scrolled-back indicator (top-right): how far up + how to get back.
    if shown_scroll > 0 {
        let label = format!(" ↑{shown_scroll} · type/wheel↓ → live ");
        let w = (label.chars().count() as u16).min(inner.width);
        let x = inner.x + inner.width.saturating_sub(w);
        f.render_widget(
            Paragraph::new(Span::styled(
                label,
                Style::default()
                    .fg(app.theme.accent_fg)
                    .bg(app.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Rect::new(x, inner.y, w, 1),
        );
    }

    let (cx, cy) = app.terminal.cursor_position();
    let sb = app.terminal.scrollback_len();
    let row_offset = if alt {
        cy as usize
    } else {
        // Cursor lives in the live grid: virtual row = sb + cy.
        (sb + cy as usize).saturating_sub(view_start_virtual)
    };
    // When scrolled back the cursor is below the window — park it bottom-right.
    let cur_y = (inner.y + row_offset.min(u16::MAX as usize) as u16)
        .min(inner.y + inner.height.saturating_sub(1));
    let cur_x = (inner.x + cx).min(inner.x + inner.width.saturating_sub(1));
    f.set_cursor_position((cur_x, cur_y));
}

fn draw_search_bar(f: &mut Frame, app: &App, area: Rect) {
    let n = app.search_matches.len();
    let idx = if n == 0 {
        0
    } else {
        app.search_current + 1
    };
    let count = if app.search_input.is_empty() {
        String::from("  type to search")
    } else if n == 0 {
        String::from("  0 matches")
    } else {
        format!("  {}/{}", idx, n)
    };

    let prompt_bg = if app.search_forward {
        Color::Rgb(230, 200, 80)
    } else {
        Color::Rgb(180, 140, 255)
    };
    let prompt = if app.search_forward { " / " } else { " ? " };

    let spans = vec![
        Span::styled(
            prompt,
            Style::default()
                .fg(Color::Black)
                .bg(prompt_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.search_input.clone(),
            Style::default()
                .fg(app.theme.fg)
                .bg(app.theme.status_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "█",
            Style::default().fg(Color::Rgb(230, 200, 80)).bg(app.theme.status_bg),
        ),
        Span::styled(
            count,
            Style::default().fg(app.theme.line_no).bg(app.theme.status_bg),
        ),
        Span::styled(
            "  Enter accept · Esc cancel · ↑↓ cycle",
            Style::default().fg(app.theme.line_no).bg(app.theme.status_bg),
        ),
    ];

    // Fill remaining
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    let mut all = spans;
    let fill = area.width.saturating_sub(used as u16) as usize;
    if fill > 0 {
        all.push(Span::styled(
            " ".repeat(fill),
            Style::default().bg(app.theme.status_bg),
        ));
    }

    f.render_widget(
        Paragraph::new(Line::from(all)).style(Style::default().bg(app.theme.status_bg)),
        area,
    );

    // Place terminal cursor on the search input
    let cursor_x = area.x + 3 + app.search_input.chars().count() as u16;
    f.set_cursor_position((
        cursor_x.min(area.x + area.width.saturating_sub(1)),
        area.y,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    use xei_core::buffer::Buffer;
    use xei_core::App;

    const MD: &str = "# Title\n\nHello **world** with a longer line of text\n\n- one\n- two\n";

    fn frame_text(term: &Terminal<TestBackend>) -> String {
        term.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    /// Buffer with distinctive "old" source lines for transform-sweep tests.
    fn app_with_old_source() -> App {
        let mut app = App::new();
        let src: String = (0..12).map(|i| format!("old{}\n", i)).collect();
        app.buffer = Buffer::from_string(&src);
        app.preview.open_for(MD, Some("md"));
        app
    }

    #[test]
    fn preview_pane_settles_flat_with_content() {
        let app = app_with_old_source();
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render_preview_pane(f, &app, Rect::new(0, 0, 80, 24), 1.0))
            .unwrap();
        let text = frame_text(&term);
        assert!(text.contains("Markdown Preview"), "frame:\n{}", text);
        assert!(text.contains("Title"));
        assert!(text.contains("one"));
        assert!(
            !text.contains('╭'),
            "preview must be a flat pane, not a popup window"
        );
        assert!(!text.contains('░') && !text.contains('▒') && !text.contains('▓'));
        // The old source view is fully consumed once settled.
        assert!(!text.contains("old9"), "frame:\n{}", text);
        // Pretty content stays inset by the gutter width — no leftward jump.
        let buf = term.backend().buffer().clone();
        for x in 0..5u16 {
            assert_eq!(buf[(x, 1)].symbol(), " ", "col {} should be gutter pad", x);
        }
        assert_eq!(buf[(5u16, 1u16)].symbol(), "T", "content starts at column 5");
    }

    #[test]
    fn preview_first_frame_is_identical_to_source_view() {
        let app = app_with_old_source();
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render_preview_pane(f, &app, Rect::new(0, 0, 80, 24), 0.0))
            .unwrap();
        let text = frame_text(&term);
        // At t=0 nothing has transformed: the buffer view is still fully
        // visible (gutter + first line included) and no preview chrome shows.
        assert!(text.contains("old0"), "frame:\n{}", text);
        assert!(text.contains("old11"));
        assert!(!text.contains("Markdown Preview"));
        assert!(!text.contains("Title"));
        assert!(!text.contains('░') && !text.contains('▒') && !text.contains('▓'));
    }

    #[test]
    fn preview_pane_transform_sweep_mid_flight() {
        let app = app_with_old_source();
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render_preview_pane(f, &app, Rect::new(0, 0, 80, 24), 0.05))
            .unwrap();
        let text = frame_text(&term);
        // Top rows are still inside the ░▒▓ band — "Title" not yet legible…
        assert!(!text.contains("Title"), "frame:\n{}", text);
        assert!(text.contains('░') || text.contains('▒') || text.contains('▓'));
        // …while below the wavefront the dimmed old source is still visible.
        assert!(text.contains("old9"), "frame:\n{}", text);
    }

    #[test]
    fn preview_full_draw_replaces_editor_pane() {
        let mut app = App::new();
        app.filename = Some(std::path::PathBuf::from("/tmp/xei_ui_test.md"));
        app.buffer = Buffer::from_string(MD);
        app.toggle_preview();
        // Simulate the animation window having fully elapsed.
        app.preview.anim_pending = false;
        app.preview.opened_at =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(2));
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let text = frame_text(&term);
        assert!(text.contains("Markdown Preview"), "frame:\n{}", text);
        assert!(text.contains("Title"));
        assert!(
            !text.contains('░') && !text.contains('▒') && !text.contains('▓'),
            "no shade glyphs once settled, frame:\n{}",
            text
        );
    }

    #[test]
    fn welcome_screen_on_empty_session() {
        let mut app = App::new();
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let text = frame_text(&term);
        assert!(text.contains("x  e  i"), "frame:\n{}", text);
        assert!(text.contains('░'), "expected shade-art logo");
    }

    #[test]
    fn welcome_shows_update_notice() {
        let mut app = App::new();
        app.update.latest = Some("9.9.9".into());
        let backend = TestBackend::new(100, 28);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let text = frame_text(&term);
        assert!(
            text.contains("v9.9.9 available") && text.contains(":update"),
            "frame:\n{}",
            text
        );
        // After a successful install the notice flips to a restart hint.
        app.update.latest = None;
        app.update.installed = true;
        term.draw(|f| draw(f, &mut app)).unwrap();
        let text = frame_text(&term);
        assert!(text.contains("restart xei"), "frame:\n{}", text);
    }
}

fn draw_statusline(f: &mut Frame, app: &App, area: Rect) {
    // Pane terminal window stays Mode::Normal but should read as a term slot.
    let term_win = app.terminal_window_focused();
    let mode_text = if term_win && app.terminal.close_confirm {
        " TERM·? "
    } else if term_win {
        " TERM·WIN "
    } else {
        match app.mode {
            Mode::Normal => {
                if app.pending_key.is_some() || app.pending_ft.is_some() {
                    " PENDING "
                } else if app.count.is_some() {
                    " COUNT "
                } else {
                    " NORMAL "
                }
            }
            Mode::Insert => " INSERT ",
            Mode::Visual => " VISUAL ",
            Mode::VisualLine => " V-LINE ",
            Mode::VisualBlock => " V-BLOCK ",
            Mode::XlcInput => " CMD ",
            Mode::Search => " SEARCH ",
            Mode::Explorer => " FILES ",
            Mode::Terminal => {
                if app.terminal.full_panel {
                    " TERM·WIN "
                } else {
                    " TERM "
                }
            }
            Mode::Palette => " PALETTE ",
            Mode::SourceControl => " SCM ",
            Mode::GitWorkbench => " GIT ",
            Mode::Settings => " SETTINGS ",
            Mode::Preview => " PREVIEW ",
            Mode::WorkspaceSearch => " FIND ",
            Mode::Screensaver => " XEIFETCH ",
            Mode::Debug => " DEBUG ",
            Mode::CallHierarchy => " CALLS ",
            Mode::Rebase => " REBASE ",
            Mode::PrReview => " PR ",
        }
    };

    let mode_style = if term_win {
        Style::default()
            .fg(app.theme.accent_fg)
            .bg(if app.terminal.close_confirm {
                app.theme.warning
            } else {
                app.theme.mode_term
            })
            .add_modifier(Modifier::BOLD)
    } else {
        let (fg, bg) = match app.mode {
            Mode::Normal => (app.theme.accent_fg, app.theme.mode_normal),
            Mode::Insert => (app.theme.accent_fg, app.theme.mode_insert),
            Mode::Visual | Mode::VisualLine | Mode::VisualBlock => {
                (app.theme.accent_fg, app.theme.mode_visual)
            }
            Mode::XlcInput | Mode::Palette => (app.theme.accent_fg, app.theme.mode_xlc),
            Mode::Search => (app.theme.accent_fg, app.theme.mode_search),
            Mode::Explorer => (app.theme.accent_fg, app.theme.explorer_dir),
            Mode::SourceControl => (app.theme.accent_fg, app.theme.mode_git),
            Mode::GitWorkbench => (app.theme.accent_fg, app.theme.mode_git),
            Mode::Settings => (app.theme.accent_fg, app.theme.mode_settings),
            Mode::Terminal => (app.theme.accent_fg, app.theme.mode_term),
            Mode::Preview => (app.theme.accent_fg, app.theme.mode_preview),
            Mode::WorkspaceSearch => (app.theme.accent_fg, app.theme.mode_find),
            Mode::Screensaver => (app.theme.accent_fg, app.theme.mode_normal),
            Mode::Debug => (app.theme.accent_fg, app.theme.accent),
            Mode::CallHierarchy => (app.theme.accent_fg, app.theme.accent),
            Mode::Rebase => (app.theme.accent_fg, app.theme.warning),
            Mode::PrReview => (app.theme.accent_fg, app.theme.accent),
        };
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
    };

    let cursor = app.buffer.cursor();
    let basename = app
        .filename
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("[No Name]");
    let dirty = if app.modified { " ●" } else { "" };

    let lang = app
        .file_extension()
        .map(|e| e.to_uppercase())
        .unwrap_or_else(|| "—".into());

    let total = app.buffer.line_count().max(1);
    let pct = ((cursor.row + 1) * 100) / total;

    let mut left = vec![
        Span::styled(mode_text, mode_style),
        Span::styled(
            format!(" {}{} ", basename, dirty),
            Style::default().fg(app.theme.status_fg).bg(app.theme.status_bg),
        ),
    ];
    // Branch badge (VS Code status-bar style)
    if !app.scm.branch.is_empty() {
        let n = app.scm.total_files();
        let branch_txt = if n > 0 {
            format!("  {} · {} ", app.scm.branch, n)
        } else {
            format!("  {} ", app.scm.branch)
        };
        left.push(Span::styled(
            branch_txt,
            Style::default()
                .fg(app.theme.accent)
                .bg(app.theme.status_bg),
        ));
    }

    // GPU-acc badge when progressive features are live
    if app.gpu_active() {
        left.push(Span::styled(
            " GPU ",
            Style::default()
                .fg(Color::Black)
                .bg(app.theme.success)
                .add_modifier(Modifier::BOLD),
        ));
    } else if app.gpu_acc && !app.term_modern {
        // User wants it but host is basic — quiet dim mark
        left.push(Span::styled(
            " gpu? ",
            Style::default()
                .fg(app.theme.line_no)
                .bg(app.theme.status_bg),
        ));
    }

    // DAP session badge (visible whenever a session or docked panel is live)
    if app.dap.is_session() || app.dap.panel_open {
        use xei_core::dap::DapState;
        let (label, fg, bg) = match app.dap.state {
            DapState::Stopped => {
                let why = app
                    .dap
                    .stopped_reason
                    .as_deref()
                    .unwrap_or("stopped");
                (
                    format!(" ● {} {why} ", short_adapter(&app.dap.adapter_name)),
                    app.theme.accent_fg,
                    app.theme.warning,
                )
            }
            DapState::Running => (
                format!(" ▶ {} run ", short_adapter(&app.dap.adapter_name)),
                app.theme.accent_fg,
                app.theme.success,
            ),
            DapState::Starting => (
                format!(" … {} ", short_adapter(&app.dap.adapter_name)),
                app.theme.accent_fg,
                app.theme.accent,
            ),
            DapState::Ending => (
                " ■ ending ".into(),
                app.theme.accent_fg,
                app.theme.muted,
            ),
            DapState::Idle => (
                " DAP ".into(),
                app.theme.muted,
                app.theme.status_bg,
            ),
        };
        left.push(Span::styled(
            label,
            Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
        ));
    }

    if !app.message.is_empty()
        && !matches!(
            app.mode,
            Mode::Insert | Mode::Visual | Mode::VisualLine
        )
    {
        let msg = if app.message.chars().count() > 40 {
            let t: String = app.message.chars().take(39).collect();
            format!("{}…", t)
        } else {
            app.message.clone()
        };
        left.push(Span::styled(
            format!(" {}", msg),
            Style::default().fg(app.theme.line_no).bg(app.theme.status_bg),
        ));
    }

    // Diagnostics under cursor
    if let Some(diag) = app.lsp.diagnostics.iter().find(|d| d.row == cursor.row) {
        let tag = if diag.severity == xei_core::lsp::DiagnosticSeverity::Error {
            "E"
        } else {
            "W"
        };
        let dmsg = if diag.message.chars().count() > 36 {
            let t: String = diag.message.chars().take(35).collect();
            format!("{}…", t)
        } else {
            diag.message.clone()
        };
        left.push(Span::styled(
            format!("  {} {}", tag, dmsg),
            Style::default()
                .fg(if tag == "E" {
                    app.theme.error
                } else {
                    app.theme.warning
                })
                .bg(app.theme.status_bg),
        ));
    }

    let mut right_parts: Vec<Span> = Vec::new();

    // Horizontal pan indicator (wrap off + panned right)
    if !app.wrap_lines && app.hscroll > 0 {
        right_parts.push(Span::styled(
            format!(" ↔{} ", app.hscroll),
            Style::default()
                .fg(app.theme.accent)
                .bg(app.theme.status_bg),
        ));
    }

    match app.lsp.status_label() {
        xei_core::lsp::LspStatus::Running { name, diags } => {
            right_parts.push(Span::styled(
                format!(" LSP:{name} ({diags}) "),
                Style::default()
                    .fg(app.theme.mode_insert)
                    .bg(app.theme.status_bg),
            ));
        }
        xei_core::lsp::LspStatus::HardError => {
            // Truncate error for the badge; full text is in app.lsp.error
            let tip = app
                .lsp
                .error
                .as_deref()
                .unwrap_or("error")
                .chars()
                .take(24)
                .collect::<String>();
            right_parts.push(Span::styled(
                format!(" LSP:err "),
                Style::default()
                    .fg(app.theme.error)
                    .bg(app.theme.status_bg),
            ));
            // Keep a short hint visible next to the badge when space allows
            if !tip.is_empty() && tip != "error" {
                right_parts.push(Span::styled(
                    format!(" {tip} "),
                    Style::default()
                        .fg(Color::Rgb(255, 160, 140))
                        .bg(app.theme.status_bg),
                ));
            }
        }
        xei_core::lsp::LspStatus::Soft { .. } => {
            // Missing binary / optional method — amber, not red
            right_parts.push(Span::styled(
                " LSP:— ",
                Style::default()
                    .fg(app.theme.warning)
                    .bg(app.theme.status_bg),
            ));
        }
        xei_core::lsp::LspStatus::Idle => {}
    }

    right_parts.push(Span::styled(
        format!(" {} ", lang),
        Style::default().fg(app.theme.line_no).bg(app.theme.status_bg),
    ));
    right_parts.push(Span::styled(
        format!(" {}:{} ", cursor.row + 1, cursor.col + 1),
        Style::default().fg(app.theme.status_fg).bg(app.theme.status_bg),
    ));
    right_parts.push(Span::styled(
        format!(" {}% ", pct),
        Style::default()
            .fg(Color::Black)
            .bg(app.theme.mode_normal)
            .add_modifier(Modifier::BOLD),
    ));

    let right_width: u16 = right_parts
        .iter()
        .map(|s| s.content.width() as u16)
        .sum();

    f.render_widget(
        Paragraph::new(Line::from(left)).style(Style::default().bg(app.theme.status_bg)),
        area,
    );

    if area.width > right_width {
        let right_area = Rect {
            x: area.x + area.width - right_width,
            y: area.y,
            width: right_width,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Line::from(right_parts)).alignment(Alignment::Right),
            right_area,
        );
    }
}
