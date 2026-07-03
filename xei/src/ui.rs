use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, BorderType, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthChar;

use xei_core::app::{App, EditorViewport, Mode};
use xei_core::highlight;

const LINE_NO_WIDTH: u16 = 5;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let ext = app.file_extension();
    let text = app.buffer.text();
    app.syntax.parse(&text, ext.as_deref());

    let explorer_open = app.explorer.open;
    let terminal_open = app.terminal.open;
    let xlc_open = app.xlc.open;

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
            .split(area);

        let mut idx = 0;
        let expl = if has_explorer { idx += 1; chunks[idx - 1] } else { Rect::default() };
        let main = chunks[idx];
        idx += 1;
        let term = if has_terminal { chunks[idx] } else { Rect::default() };

        (expl, main, term)
    };

    let editor_area = if xlc_open {
        let xlc_total = app.xlc_height;
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(xlc_total),
                Constraint::Length(xlc_total),
                Constraint::Length(1),
            ])
            .split(main_rect);

        draw_editor(f, app, chunks[0]);
        draw_xlc(f, app, chunks[1]);
        draw_statusline(f, app, chunks[2]);
        app.xlc_separator_y = chunks[1].y;
        chunks[0]
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(main_rect);

        draw_editor(f, app, chunks[0]);
        draw_statusline(f, app, chunks[1]);
        chunks[0]
    };

    if explorer_open {
        draw_explorer(f, app, explorer_rect);
    }

    if terminal_open {
        app.terminal.poll();
        draw_terminal(f, app, term_rect);
    }

    app.screen_width = area.width;
    app.screen_height = area.height;
    app.explorer_separator_x = if explorer_open {
        explorer_rect.x + explorer_rect.width
    } else {
        0
    };
    app.terminal_separator_x = if terminal_open {
        term_rect.x
    } else {
        0
    };

    app.viewport = EditorViewport {
        x: editor_area.x + 1,
        y: editor_area.y + 1,
        width: editor_area.width.saturating_sub(2),
        height: editor_area.height.saturating_sub(2),
    };

    if app.completions.active {
        draw_completions(f, app, area);
    }

    if app.pending_key.is_some() && !app.pending_hints.is_empty() {
        draw_pending_hints(f, app, area);
    }
}

fn draw_tabbar(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    for (i, tab) in app.buffers.iter().enumerate() {
        let name = tab.filename.as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("[No Name]");
        let is_current = i == app.current_buffer;
        let style = if is_current {
            Style::default().fg(Color::Black).bg(app.theme.border)
        } else {
            Style::default().fg(app.theme.line_no)
        };
        let modified = if tab.modified { " +" } else { "" };
        spans.push(Span::styled(format!(" {} ", name), style));
        spans.push(Span::styled(modified, Style::default().fg(app.theme.border)));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_editor(f: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let (_tab_area, main_area) = if app.buffers.len() > 1 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);
        draw_tabbar(f, app, chunks[0]);
        (chunks[0], chunks[1])
    } else {
        (Rect::default(), area)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(app.theme.editor_bg))
        .title(Span::styled(
            format!(
                " xei {}",
                app.filename
                    .as_ref()
                    .map(|s| format!("- {}", s.display()))
                    .unwrap_or_default()
            ),
            Style::default().fg(app.theme.border),
        ))
        .title_alignment(Alignment::Left);

    let visible_height = (main_area.height as usize).saturating_sub(2);
    let scroll = app.scroll;
    let all_lines = app.buffer.lines();
    let selection = app.selected_range();
    let ext = app.file_extension();

    let mut visible_lines: Vec<(usize, &String)> = Vec::new();
    let mut wrap_height = 0;
    let text_width = (main_area.width.saturating_sub(2 + LINE_NO_WIDTH)).max(1) as usize;

    for (idx, line) in all_lines.iter().enumerate().skip(scroll) {
        let vis = visual_line_width(app, idx);
        let rows = if vis == 0 { 1 } else { (vis + text_width - 1) / text_width };
        if wrap_height + rows > visible_height {
            break;
        }
        visible_lines.push((idx, line));
        wrap_height += rows;
    }

    let lines: Vec<Line> = visible_lines
        .iter()
        .map(|(row, text)| {
            let line_no = Span::styled(
                format!("{:>4} ", row + 1),
                Style::default().fg(app.theme.line_no),
            );
            let styled_text = render_line_with_highlights(row, text, app, selection, ext.as_deref());
            let mut spans = vec![line_no];
            spans.extend(styled_text);
            Line::from(spans)
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, main_area);

    let cursor = app.buffer.cursor();
    let screen_col = app.buffer.buffer_col_to_screen_col(cursor.row, cursor.col);

    let text_width = (main_area.width.saturating_sub(2 + LINE_NO_WIDTH)).max(1) as usize;
    let mut display_row = 0usize;
    let mut found = false;

    for row in scroll..app.buffer.line_count() {
        let visual_len = visual_line_width(app, row);
        let line_rows = if visual_len == 0 { 1 } else { (visual_len + text_width - 1) / text_width };
        if row == cursor.row {
            let wrap_row = screen_col / text_width;
            display_row += wrap_row;
            found = true;
            break;
        }
        display_row += line_rows;
    }

    if found && display_row < (main_area.height as usize).saturating_sub(2) {
        let cursor_x = (main_area.x + 1 + LINE_NO_WIDTH + (screen_col % text_width) as u16)
            .min(main_area.x + main_area.width.saturating_sub(1));
        let cursor_y = (main_area.y + 1 + display_row as u16)
            .min(main_area.y + main_area.height.saturating_sub(1));
        f.set_cursor_position((cursor_x, cursor_y));
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
) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if len == 0 { return vec![Span::raw("")]; }

    let hl_tokens: Vec<&(highlight::TokenKind, usize, usize, usize)> = app.syntax.tokens.iter()
        .filter(|(_, _, _, r)| *r == *row).collect();
    let fallback = highlight::highlight_line(text, ext);
    let row_diags: Vec<&xei_core::lsp::Diagnostic> = app.lsp.diagnostics.iter()
        .filter(|d| d.row == *row).collect();

    let visual_style = Style::default().bg(app.theme.selection_bg).add_modifier(Modifier::BOLD);
    let search_style = Style::default().bg(app.theme.search_bg).add_modifier(Modifier::BOLD);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut vis_col = 0usize;
    let mut run_style = Style::default().fg(app.theme.fg);
    let mut run_text = String::new();

    for i in 0..len {
        let ch = chars[i];
        let char_str = if ch == '\t' { " ".repeat(4 - (vis_col % 4)) } else { ch.to_string() };
        let char_width = if ch == '\t' { char_str.chars().count() } else { UnicodeWidthChar::width(ch).unwrap_or(1) };

        let mut s = Style::default().fg(app.theme.fg);

        for (kind, st, ed, _) in &hl_tokens { if i >= *st && i < *ed { s = highlight::style_for(app.theme, *kind); break; } }
        if s == Style::default().fg(app.theme.fg) {
            for &(kind, st, ed) in &fallback { if i >= st && i < ed { s = highlight::style_for(app.theme, kind); break; } }
        }

        if let Some((start, end)) = selection {
            if app.mode == Mode::VisualLine { if *row >= start.row && *row <= end.row { s = visual_style; } }
            else if *row >= start.row && *row <= end.row {
                let ls = if *row == start.row { start.col } else { 0 };
                let le = if *row == end.row { end.col } else { len };
                if i >= ls && i < le { s = visual_style; }
            }
        }

        if s == Style::default().fg(app.theme.fg) {
            if let Some(ref pat) = app.search_pattern {
                if !pat.is_empty() {
                    for pos in &app.search_matches {
                        if pos.row == *row && i >= pos.col && i < pos.col + pat.len() { s = search_style; break; }
                    }
                }
            }
        }

        for diag in &row_diags {
            if i >= diag.col_start && i < diag.col_end {
                s = match diag.severity {
                    xei_core::lsp::DiagnosticSeverity::Error => s.fg(Color::Red).add_modifier(Modifier::UNDERLINED),
                    xei_core::lsp::DiagnosticSeverity::Warning => s.fg(Color::Yellow).add_modifier(Modifier::UNDERLINED),
                    _ => s,
                };
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
        vis_col += char_width;
    }

    if !run_text.is_empty() {
        spans.push(Span::styled(run_text, run_style));
    }

    spans
}

fn draw_explorer(f: &mut Frame, app: &App, area: Rect) {
    let cwd_display = app.explorer.cwd.display().to_string();
    let short_cwd = if cwd_display.len() > area.width as usize {
        format!("...{}", &cwd_display[cwd_display.len().saturating_sub(area.width as usize - 4)..])
    } else {
        cwd_display
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.border))
        .style(Style::default().bg(app.theme.explorer_bg))
        .title(Span::styled(short_cwd, Style::default().fg(app.theme.border)))
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let visible_height = inner.height as usize;
    let entries = &app.explorer.entries;
    let scroll = app.explorer.scroll;
    let selected = app.explorer.selected;

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(idx, entry)| {
            let prefix = if entry.is_dir { " /" } else { "  " };
            let is_selected = idx == selected;
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(app.theme.explorer_selected)
            } else if entry.is_dir {
                Style::default().fg(app.theme.explorer_dir)
            } else {
                Style::default().fg(app.theme.explorer_fg)
            };
            let name = if entry.name.len() > inner.width as usize - 3 {
                format!("{}{}", prefix, &entry.name[..inner.width as usize - 3])
            } else {
                format!("{}{}", prefix, entry.name)
            };
            ListItem::new(name).style(style)
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

fn draw_pending_hints(f: &mut Frame, app: &App, area: Rect) {
    let hints = &app.pending_hints;
    let count = hints.len() as u16;
    let popup_w = 30u16;
    let popup_h = count + 2;

    let cursor = app.buffer.cursor();
    let screen_col = app.buffer.buffer_col_to_screen_col(cursor.row, cursor.col);
    let cx = (area.x + 1 + LINE_NO_WIDTH + screen_col as u16).min(area.x + area.width.saturating_sub(popup_w));
    let cy = (area.y + 1 + (cursor.row.saturating_sub(app.scroll)) as u16 + 1)
        .min(area.y + area.height.saturating_sub(popup_h));

    let popup = Rect { x: cx, y: cy, width: popup_w, height: popup_h };

    // Frosted glass: shadow + rounded border + translucent bg
    let shadow = Rect { x: cx + 1, y: cy + 1, width: popup_w, height: popup_h };
    f.render_widget(Block::default().style(Style::default().bg(Color::Black)), shadow);

    let glass_bg = Color::Rgb(40, 43, 60);
    let items: Vec<Line> = hints.iter().map(|(key, desc)| {
        Line::from(vec![
            Span::styled(format!(" {} ", key), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(desc.to_string(), Style::default().fg(Color::Gray)),
        ])
    }).collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(90, 95, 120)))
        .style(Style::default().bg(glass_bg))
        .title(Span::styled(" hints ", Style::default().fg(Color::Rgb(130, 140, 170))));

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
    let popup_width = (max_label + max_detail + 6).min(60).max(16) as u16;

    let cursor = app.buffer.cursor();
    let screen_col = app.buffer.buffer_col_to_screen_col(cursor.row, cursor.col);
    let cursor_x = area.x + 1 + LINE_NO_WIDTH + screen_col as u16;
    let cursor_y = area.y + 1 + (cursor.row.saturating_sub(app.scroll)) as u16;

    let popup_x = cursor_x.min(
        area.x + area.width.saturating_sub(popup_width + 2)
    );
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
        app.completions.selected.saturating_sub((max_height as usize) / 2).min(
            suggestions.len().saturating_sub(max_height as usize)
        )
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
                Style::default().fg(Color::Black).bg(app.theme.completion_selected)
            } else {
                Style::default().fg(app.theme.line_no)
            };
            Line::from(vec![
                Span::styled(format!(" {} ", s.label), label_style),
                Span::styled(
                    format!(
                        "{}{}",
                        s.detail,
                        " ".repeat(
                            popup_width
                                .saturating_sub((s.label.len() + s.detail.len() + 3) as u16)
                                as usize
                        )
                    ),
                    detail_style,
                ),
            ])
        })
        .collect();

    // Frosted glass shadow
    f.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        Rect { x: popup_x + 1, y: popup_y + 1, width: popup_width, height: max_height + 2 },
    );

    let glass_bg = Color::Rgb(35, 38, 55);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(90, 95, 120)))
        .style(Style::default().bg(glass_bg))
        .title(Span::styled(
            format!(" {} ({}) ", app.file_name(), suggestions.len()),
            Style::default().fg(Color::Rgb(130, 140, 170)),
        ));

    let popup = Paragraph::new(items).block(block);

    f.render_widget(Clear, popup_area);
    f.render_widget(popup, popup_area);
}

fn draw_xlc(f: &mut Frame, app: &App, area: Rect) {
    let is_search = app.mode == Mode::Search;
    let title = if is_search { " Search " } else { " XLC " };
    let border_color = if is_search { Color::Yellow } else { app.theme.xlc_border };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(app.theme.xlc_bg))
        .title(Span::styled(title, Style::default().fg(border_color)))
        .title_alignment(Alignment::Center);

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
                Line::from(Span::styled(
                    format!("  {}", s),
                    Style::default().fg(app.theme.xlc_fg),
                ))
            })
            .collect::<Vec<Line>>(),
    );

    f.render_widget(output_widget, xlc_chunks[0]);

    let input_text = &app.xlc.input;
    let prompt = if is_search {
        if input_text.starts_with('/') {
            input_text.clone()
        } else {
            format!("/{}", input_text)
        }
    } else {
        format!("> {}", input_text)
    };

    let prompt_style = if is_search {
        Style::default().fg(Color::Yellow).bg(app.theme.xlc_bg)
    } else {
        Style::default().fg(app.theme.xlc_prompt).bg(app.theme.xlc_bg)
    };

    let prompt_widget = Paragraph::new(Line::from(Span::styled(&prompt, prompt_style)));

    f.render_widget(prompt_widget, xlc_chunks[1]);

    let cursor_offset = if is_search { 1 } else { 2 };
    let cursor_x = (xlc_chunks[1].x + input_text.len() as u16 + cursor_offset)
        .min(xlc_chunks[1].x + xlc_chunks[1].width.saturating_sub(1));
    let cursor_y = xlc_chunks[1].y;
    f.set_cursor_position((cursor_x, cursor_y));
}

fn draw_terminal(f: &mut Frame, app: &App, area: Rect) {
    let cwd = if let Some(ref path) = app.filename {
        path.parent().map(|p| p.display().to_string()).unwrap_or_else(|| "?".to_string())
    } else {
        std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| "?".to_string())
    };

    let short_cwd = if cwd.len() > area.width as usize - 10 {
        format!("...{}", &cwd[cwd.len().saturating_sub(area.width as usize - 10)..])
    } else {
        cwd
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.border))
        .style(Style::default().bg(app.theme.terminal_bg))
        .title(Span::styled(short_cwd, Style::default().fg(app.theme.border)))
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 1 {
        return;
    }

    let vis_height = inner.height as usize;
    let sb = app.terminal.scrollback_len();
    let scroll = app.terminal.scroll();

    let mut lines: Vec<Line> = Vec::new();

    if sb > 0 {
        let mut sb_rows = app.terminal.visible_scrollback();
        let start = scroll.min(sb_rows.len());
        let end = (start + vis_height).min(sb_rows.len());
        for row in sb_rows.drain(start..end) {
            let spans: Vec<Span> = row.iter().map(|(ch, fg, _bg)| {
                let mut style = Style::default().fg(app.theme.terminal_fg);
                if let Some(c) = fg { style = style.fg(*c); }
                Span::styled(ch.clone(), style)
            }).collect();
            lines.push(Line::from(spans));
        }
    }

    let remaining = vis_height.saturating_sub(lines.len());
    if remaining > 0 {
        let vt_rows = app.terminal.visible_rows();
        let end = remaining.min(vt_rows.len());
        for row in vt_rows.iter().take(end) {
            let spans: Vec<Span> = row.iter().map(|(ch, fg, bg)| {
                let mut style = Style::default().fg(app.theme.terminal_fg).bg(app.theme.terminal_bg);
                if let Some(c) = fg { style = style.fg(*c); }
                if let Some(c) = bg { style = style.bg(*c); }
                Span::styled(ch.clone(), style)
            }).collect();
            lines.push(Line::from(spans));
        }
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);

    let (cx, cy) = app.terminal.cursor_position();
    let offset = sb.saturating_sub(scroll);
    let row_offset = ((offset + cy as usize).saturating_sub(scroll)) as u16;
    let cur_y = (inner.y + row_offset).min(inner.y + inner.height.saturating_sub(1));
    let cur_x = (inner.x + cx).min(inner.x + inner.width.saturating_sub(1));
    f.set_cursor_position((cur_x, cur_y));
}

fn draw_statusline(f: &mut Frame, app: &App, area: Rect) {
    let style = Style::default().bg(app.theme.status_bg);

    let mode_text = match app.mode {
        Mode::Normal => {
            if app.pending_key.is_some() {
                " --PENDING-- "
            } else {
                " NORMAL "
            }
        }
        Mode::Insert => " INSERT ",
        Mode::Visual => " VISUAL ",
        Mode::VisualLine => " V-LINE ",
        Mode::XlcInput => " XLC ",
        Mode::Search => " SEARCH ",
        Mode::Explorer => " EXPLORER ",
        Mode::Terminal => " TERMINAL ",
    };

    let mode_style = match app.mode {
        Mode::Normal => Style::default().fg(Color::Black).bg(app.theme.mode_normal),
        Mode::Insert => Style::default().fg(Color::Black).bg(app.theme.mode_insert),
        Mode::Visual | Mode::VisualLine => Style::default().fg(Color::Black).bg(app.theme.mode_visual),
        Mode::XlcInput => Style::default().fg(Color::Black).bg(app.theme.mode_xlc),
        Mode::Search => Style::default().fg(Color::Black).bg(Color::Yellow),
        Mode::Explorer => Style::default().fg(Color::Black).bg(Color::Rgb(100, 150, 255)),
        Mode::Terminal => Style::default().fg(Color::Black).bg(Color::Rgb(100, 255, 100)),
    };

    let cursor = app.buffer.cursor();

    let left = Line::from(vec![
        Span::styled(mode_text, mode_style),
        Span::raw(" "),
        Span::styled(
            app.filename
                .as_ref()
                .map(|s| s.display().to_string())
                .unwrap_or_else(|| "[No Name]".to_string()),
            Style::default().fg(app.theme.status_fg),
        ),
    ]);

    let right_text = format!(
        " Ln {}  Col {}  {}{}",
        cursor.row + 1,
        cursor.col + 1,
        if let Some(diag) = app.lsp.diagnostics.iter().find(|d| d.row == cursor.row) {
            format!("  [{}] {}", if diag.severity == xei_core::lsp::DiagnosticSeverity::Error { "E" } else { "W" }, diag.message)
        } else {
            app.message.clone()
        },
        if app.lsp.server_running { format!("  LSP: {} ({})", app.lsp.server_name, app.lsp.diagnostics.len()) } else if app.lsp.error.is_some() { "  LSP: error".to_string() } else { String::new() }
    );

    let right_len = right_text.len() as u16;

    f.render_widget(Paragraph::new(left).style(style), area);

    if area.width > right_len {
        let right_area = Rect {
            x: area.x + area.width - right_len,
            y: area.y,
            width: right_len,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                right_text,
                Style::default().fg(app.theme.line_no),
            )))
            .alignment(Alignment::Right),
            right_area,
        );
    }
}
