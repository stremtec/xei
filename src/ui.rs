use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, EditorViewport, Mode};
use crate::highlight;

const XLC_HEIGHT: u16 = 8;
const LINE_NO_WIDTH: u16 = 5;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
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
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(XLC_HEIGHT),
                Constraint::Length(XLC_HEIGHT + 3),
                Constraint::Length(1),
            ])
            .split(main_rect);

        draw_editor(f, app, chunks[0]);
        draw_xlc(f, app, chunks[1]);
        draw_statusline(f, app, chunks[2]);
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
}

fn draw_editor(f: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

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

    let visible_height = (area.height as usize).saturating_sub(2);
    let scroll = app.scroll;
    let all_lines = app.buffer.lines();
    let selection = app.selected_range();
    let ext = app.file_extension();

    let visible_lines: Vec<(usize, &String)> = all_lines
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .collect();

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

    f.render_widget(paragraph, area);

    let cursor = app.buffer.cursor();
    let cursor_x = area.x + 1 + LINE_NO_WIDTH + cursor.col as u16;
    let cursor_y = area.y + 1 + (cursor.row.saturating_sub(scroll)) as u16;

    if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_line_with_highlights(
    row: &usize,
    text: &str,
    app: &App,
    selection: Option<(crate::buffer::Position, crate::buffer::Position)>,
    ext: Option<&str>,
) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    if len == 0 {
        return vec![Span::raw("")];
    }

    let hl_tokens = highlight::highlight_line(text, ext);

    let visual_style = Style::default()
        .bg(app.theme.selection_bg)
        .add_modifier(Modifier::BOLD);
    let search_style = Style::default()
        .bg(app.theme.search_bg)
        .add_modifier(Modifier::BOLD);

    let mut spans: Vec<Span<'static>> = Vec::new();

    for i in 0..len {
        let ch = chars[i];
        let char_str = if ch == '\t' {
            "    ".to_string()
        } else {
            ch.to_string()
        };

        let mut char_style = Style::default().fg(app.theme.fg);

        for &(kind, s, e) in &hl_tokens {
            if i >= s && i < e {
                char_style = highlight::style_for(app.theme, kind);
                break;
            }
        }

        if let Some((start, end)) = selection {
            if app.mode == Mode::VisualLine {
                if *row >= start.row && *row <= end.row {
                    char_style = visual_style;
                }
            } else if *row >= start.row && *row <= end.row {
                let line_start = if *row == start.row { start.col } else { 0 };
                let line_end = if *row == end.row { end.col } else { len };
                if i >= line_start && i < line_end {
                    char_style = visual_style;
                }
            }
        }

        if char_style == Style::default() {
            if let Some(ref pattern) = app.search_pattern {
                if !pattern.is_empty() {
                    for pos in &app.search_matches {
                        if pos.row == *row && i >= pos.col && i < pos.col + pattern.len() {
                            char_style = search_style;
                            break;
                        }
                    }
                }
            }
        }

        spans.push(Span::styled(char_str, char_style));
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
    let cursor_x = area.x + 1 + LINE_NO_WIDTH + cursor.col as u16;
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

    let visible: Vec<(usize, &crate::completion::Suggestion)> = suggestions
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

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.completion_border))
        .style(Style::default().bg(app.theme.completion_bg))
        .title(Span::styled(
            format!(" {} ({}) ", app.file_name(), suggestions.len()),
            Style::default().fg(app.theme.completion_border),
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
    f.set_cursor_position((
        xlc_chunks[1].x + input_text.len() as u16 + cursor_offset,
        xlc_chunks[1].y,
    ));
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
    let cur_y = inner.y + ((offset + cy as usize).saturating_sub(scroll)).min(inner.height.saturating_sub(1) as usize) as u16;
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
        " Ln {}  Col {}  {}",
        cursor.row + 1,
        cursor.col + 1,
        app.message
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
