use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::buffer::{Buffer, Position};
use crate::completion::Completions;
use crate::config;
use crate::explorer::Explorer;
use crate::lsp::LspClient;
use crate::syntax::SyntaxEngine;
use crate::term::Terminal;
use crate::theme::{self, Theme, OCEAN};
use crate::xlc::{UndoStack, Xlc, XlcCmd};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    VisualLine,
    XlcInput,
    Search,
    Explorer,
    Terminal,
}

pub struct App {
    pub running: bool,
    pub mode: Mode,
    pub buffer: Buffer,
    pub message: String,
    pub filename: Option<PathBuf>,
    pub scroll: usize,
    pub xlc: Xlc,
    pub undo_stack: UndoStack,
    pub yank_buffer: Option<String>,
    pub pending_key: Option<char>,
    pub visual_anchor: Option<Position>,
    pub search_pattern: Option<String>,
    pub search_matches: Vec<Position>,
    pub search_current: usize,
    pub completions: Completions,
    pub modified: bool,
    pub mouse: MouseState,
    pub viewport: EditorViewport,
    pub explorer: Explorer,
    pub terminal: Terminal,
    pub explorer_width: u16,
    pub terminal_width: u16,
    pub resize_target: Option<ResizeTarget>,
    pub explorer_separator_x: u16,
    pub terminal_separator_x: u16,
    pub screen_width: u16,
    pub screen_height: u16,
    pub theme: &'static Theme,
    pub xlc_height: u16,
    pub xlc_separator_y: u16,
    pub file_mtime: Option<std::time::SystemTime>,
    pub buffers: Vec<BufferTab>,
    pub current_buffer: usize,
    pub syntax: SyntaxEngine,
    pub lsp: LspClient,
}

#[derive(Clone)]
pub struct BufferTab {
    pub buffer: Buffer,
    pub filename: Option<PathBuf>,
    pub scroll: usize,
    pub modified: bool,
    pub undo_stack: UndoStack,
    pub file_mtime: Option<std::time::SystemTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeTarget {
    Explorer,
    Terminal,
    Xlc,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MouseState {
    pub dragging: bool,
    pub drag_anchor: Option<Position>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EditorViewport {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            mode: Mode::Normal,
            buffer: Buffer::new(),
            message: String::from("Welcome to xei! i=insert :=XLC h/j/k/l=move"),
            filename: None,
            scroll: 0,
            xlc: Xlc::new(),
            undo_stack: UndoStack::new(),
            yank_buffer: None,
            pending_key: None,
            visual_anchor: None,
            search_pattern: None,
            search_matches: Vec::new(),
            search_current: 0,
            completions: Completions::new(),
            modified: false,
            mouse: MouseState::default(),
            viewport: EditorViewport::default(),
            explorer: Explorer::new(),
            terminal: Terminal::new(),
            explorer_width: 22,
            terminal_width: 30,
            resize_target: None,
            explorer_separator_x: 0,
            terminal_separator_x: 0,
            screen_width: 80,
            screen_height: 24,
            theme: &OCEAN,
            xlc_height: 11,
            xlc_separator_y: 0,
            file_mtime: None,
            buffers: vec![BufferTab {
                buffer: Buffer::new(),
                filename: None,
                scroll: 0,
                modified: false,
                undo_stack: UndoStack::new(),
                file_mtime: None,
            }],
            current_buffer: 0,
            syntax: SyntaxEngine::new(),
            lsp: LspClient::new(),
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_file(path: &str) -> Self {
        let pathbuf = PathBuf::from(path);
        let abs_path = if pathbuf.is_absolute() {
            pathbuf
        } else {
            env::current_dir()
                .unwrap_or_default()
                .join(&pathbuf)
        };
        let content = fs::read_to_string(&abs_path).unwrap_or_default();
        let message = format!("Opened: {}", abs_path.display());
        let buffer = Buffer::from_string(&content);
        let mut app = Self {
            buffer,
            filename: Some(abs_path.clone()),
            message,
            modified: false,
            ..Self::default()
        };
        app.undo_stack.push(app.buffer.snapshot());
        app.record_mtime();
        app.lsp.auto_start(&abs_path.display().to_string());
        app
    }

    pub fn file_extension(&self) -> Option<String> {
        self.filename
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
    }

    pub fn file_name(&self) -> &str {
        self.filename
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
    }

    pub fn push_undo(&mut self) {
        self.undo_stack.push(self.buffer.snapshot());
        self.modified = true;
        if let Some(ref path) = self.filename {
            self.lsp.notify_change(&path.display().to_string(), &self.buffer.text());
        }
    }

    pub fn undo(&mut self) {
        if let Some(snap) = self.undo_stack.undo() {
            self.buffer.restore(snap);
            self.message = String::from("UNDO");
        } else {
            self.message = String::from("Already at oldest change");
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn enter_insert(&mut self) {
        self.push_undo();
        self.visual_anchor = None;
        self.mode = Mode::Insert;
        self.message = String::from("-- INSERT --");
    }

    pub fn enter_normal(&mut self) {
        self.mode = Mode::Normal;
        self.visual_anchor = None;
        self.message = String::new();
    }

    pub fn enter_visual(&mut self) {
        self.mode = Mode::Visual;
        self.visual_anchor = Some(self.buffer.cursor());
        self.message = String::from("-- VISUAL --");
    }

    pub fn enter_visual_line(&mut self) {
        self.mode = Mode::VisualLine;
        self.visual_anchor = Some(self.buffer.cursor());
        self.message = String::from("-- VISUAL LINE --");
    }

    pub fn enter_xlc(&mut self, prompt: Option<&str>) {
        self.mode = Mode::XlcInput;
        self.xlc.open_panel(prompt);
    }

    pub fn close_xlc(&mut self) {
        self.xlc.close();
        self.mode = Mode::Normal;
    }

    pub fn enter_search(&mut self) {
        self.mode = Mode::Search;
        self.xlc.open_panel(Some("/"));
    }

    pub fn selected_range(&self) -> Option<(Position, Position)> {
        let anchor = self.visual_anchor?;
        let cursor = self.buffer.cursor();
        if self.mode == Mode::VisualLine {
            let (start_row, end_row) = if anchor.row <= cursor.row {
                (anchor.row, cursor.row)
            } else {
                (cursor.row, anchor.row)
            };
            Some((
                Position::new(start_row, 0),
                Position::new(end_row, self.buffer.line(end_row).chars().count()),
            ))
        } else {
            if anchor.row < cursor.row || (anchor.row == cursor.row && anchor.col <= cursor.col) {
                Some((anchor, cursor))
            } else {
                Some((cursor, anchor))
            }
        }
    }

    pub fn execute_xlc(&mut self) {
        let cmd = self.xlc.execute();
        match cmd {
            XlcCmd::Save => self.save_file(),
            XlcCmd::SaveAs(path) => {
                self.filename = Some(PathBuf::from(&path));
                self.save_file();
            }
            XlcCmd::Quit => {
                if self.modified {
                    self.message = String::from("Unsaved changes. Use :w first or :q! to force quit.");
                    self.xlc.add_output("Unsaved changes. Use w to save first, or q! to force quit.");
                } else {
                    self.quit();
                }
            }
            XlcCmd::ForceQuit => self.quit(),
            XlcCmd::Open(path) => self.open_in_place(&path),
            XlcCmd::Move(dest) => self.move_file(&dest),
            XlcCmd::Rename(name) => {
                if let Some(ref path) = self.filename {
                    let parent = path.parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| {
                            env::current_dir().unwrap_or_default()
                        });
                    let new_path = parent.join(name);
                    self.move_file(&new_path.display().to_string());
                } else {
                    self.xlc.add_output("No file to rename.");
                }
            }
            XlcCmd::DeleteFile => {
                if let Some(ref path) = self.filename {
                    match fs::remove_file(path) {
                        Ok(_) => self.xlc.add_output(&format!("Deleted: {}", path.display())),
                        Err(e) => self.xlc.add_output(&format!("Error: {}", e)),
                    }
                } else {
                    self.xlc.add_output("No file to delete.");
                }
            }
            XlcCmd::Pwd => {
                let cwd = env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "?".to_string());
                self.xlc.add_output(&cwd);
            }
            XlcCmd::Ls => {
                if let Ok(entries) = std::fs::read_dir(".") {
                    for entry in entries.flatten() {
                        let meta = entry.file_type().ok();
                        let name = entry.file_name();
                        let prefix = if meta.map(|m| m.is_dir()).unwrap_or(false) { "/" } else { "" };
                        self.xlc.add_output(&format!("  {}{}", name.to_string_lossy(), prefix));
                    }
                } else {
                    self.xlc.add_output("Could not list directory.");
                }
            }
            XlcCmd::Help => {
                self.xlc.add_output("=== xei Commands ===");
                self.xlc.add_output("  w, save         Save current file");
                self.xlc.add_output("  w <path>        Save to a new path");
                self.xlc.add_output("  e, open <file>  Open a file");
                self.xlc.add_output("  mv, move <dest> Move/rename current file");
                self.xlc.add_output("  rename <name>   Rename in same directory");
                self.xlc.add_output("  rm              Delete current file");
                self.xlc.add_output("  pwd             Show working directory");
                self.xlc.add_output("  ls              List files");
                self.xlc.add_output("  q               Quit (with unsaved warning)");
                self.xlc.add_output("  q!              Force quit");
                self.xlc.add_output("  wq, x           Save and quit");
                self.xlc.add_output("  find, / <pat>   Search in buffer");
                self.xlc.add_output("  help, h, ?      Show this help");
            }
            XlcCmd::Search(pattern) => {
                self.search_pattern = Some(pattern.clone());
                self.perform_search();
                self.message = format!("Search: /{}/  {} matches", pattern, self.search_matches.len());
                self.xlc.add_output(&format!("Search for /{}/ found {} matches", pattern, self.search_matches.len()));
            }
            XlcCmd::Theme(name) => {
                if name.is_empty() {
                    self.xlc.add_output("Available themes:");
                    for t in theme::all_themes() {
                        let marker = if self.theme.name == t.name { " *" } else { "  " };
                        self.xlc.add_output(&format!("{}{}", marker, t.name));
                    }
                } else if let Some(t) = theme::find(&name) {
                    self.theme = t;
                    config::save_theme(t.name);
                    set_cursor_esc(t.cursor);
                    self.message = format!("Theme: {}", t.name);
                    self.xlc.add_output(&format!("Switched to theme: {}", t.name));
                } else {
                    self.xlc.add_output(&format!("Unknown theme: {}. Use :theme to list.", name));
                }
            }
            XlcCmd::BufDelete => {
                self.close_current_tab();
                self.xlc.add_output("Buffer closed");
            }
            XlcCmd::LspStart(cmd) => {
                if let Some(ref path) = self.filename {
                    let root = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
                    self.lsp.start(&cmd, &root, &path.display().to_string());
                    self.xlc.add_output(&format!("LSP started: {}", cmd));
                }
            }
            XlcCmd::None => {
                self.message = String::from("Unknown command. Try :help");
                self.xlc.add_output("Try :help for available commands.");
            }
        }
    }

    fn open_in_place(&mut self, path: &str) {
        self.open_new_tab(path);
    }

    fn move_file(&mut self, dest: &str) {
        if let Some(ref path) = self.filename {
            let dest_path = PathBuf::from(dest);
            match fs::rename(path, &dest_path) {
                Ok(_) => {
                    self.filename = Some(dest_path);
                    self.message = format!("Moved to: {}", dest);
                    self.xlc.add_output(&format!("Moved to: {}", dest));
                }
                Err(e) => {
                    self.xlc.add_output(&format!("Error moving: {}", e));
                }
            }
        } else {
            self.xlc.add_output("No file to move.");
        }
    }

    pub fn perform_search(&mut self) {
        self.search_matches.clear();
        let pattern = match &self.search_pattern {
            Some(p) => p.clone(),
            None => return,
        };
        if pattern.is_empty() {
            return;
        }

        for (row, line) in self.buffer.lines().iter().enumerate() {
            let mut start = 0;
            while let Some(found) = line[start..].find(&pattern) {
                let col = start + found;
                self.search_matches.push(Position::new(row, col));
                start = col + 1;
            }
        }
        self.search_current = 0;
        if !self.search_matches.is_empty() {
            let pos = self.search_matches[0];
            self.buffer.cursor = pos;
            self.scroll = self.buffer.cursor.row.saturating_sub(5);
        }
    }

    pub fn search_next(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_current = (self.search_current + 1) % self.search_matches.len();
        let pos = self.search_matches[self.search_current];
        self.buffer.cursor = Position::new(pos.row, pos.col);
        self.update_scroll();
    }

    pub fn search_prev(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_current = if self.search_current == 0 {
            self.search_matches.len() - 1
        } else {
            self.search_current - 1
        };
        let pos = self.search_matches[self.search_current];
        self.buffer.cursor = Position::new(pos.row, pos.col);
        self.update_scroll();
    }

    pub fn save_file(&mut self) {
        if let Some(path) = self.filename.clone() {
            match fs::write(&path, self.buffer.text()) {
                Ok(_) => {
                    self.modified = false;
                    self.record_mtime();
                    self.message = format!("Saved: {}", path.display());
                    self.xlc.add_output(&format!("Saved: {}", path.display()));
                }
                Err(e) => {
                    self.message = format!("Error: {}", e);
                    self.xlc.add_output(&format!("Error: {}", e));
                }
            }
        } else {
            self.message = String::from("No filename. Use :w <filename>");
            self.xlc.add_output("No filename. Use: w <path>");
        }
    }

    pub fn move_left(&mut self) {
        self.buffer.move_left();
    }

    pub fn move_right(&mut self) {
        self.buffer.move_right();
    }

    pub fn move_up(&mut self) {
        self.buffer.move_up();
        self.update_scroll();
    }

    pub fn move_down(&mut self) {
        self.buffer.move_down();
        self.update_scroll();
    }

    pub fn update_scroll(&mut self) {
        let cursor_row = self.buffer.cursor.row;
        let visible_height = self.viewport.height.max(1) as usize;
        if cursor_row < self.scroll {
            self.scroll = cursor_row;
        } else if cursor_row >= self.scroll + visible_height {
            self.scroll = cursor_row.saturating_sub(visible_height) + 1;
        }
    }

    pub fn delete_line(&mut self) {
        self.push_undo();
        let deleted = self.buffer.delete_line();
        self.yank_buffer = Some(deleted);
    }

    pub fn delete_word(&mut self) {
        self.push_undo();
        let deleted = self.buffer.delete_word();
        self.yank_buffer = Some(deleted);
    }

    pub fn paste(&mut self) {
        if let Some(ref text) = self.yank_buffer.clone() {
            if text.contains('\n') {
                self.push_undo();
                for line in text.split('\n') {
                    self.buffer.paste_line_after(line);
                }
            } else {
                self.push_undo();
                for c in text.chars() {
                    self.buffer.insert_char(c);
                }
            }
        }
    }

    pub fn yank_selection(&mut self) {
        if let Some((start, end)) = self.selected_range() {
            let mut lines: Vec<String> = Vec::new();
            for row in start.row..=end.row {
                let line = self.buffer.line(row);
                let s = if row == start.row && row == end.row {
                    line[start.col..end.col].to_string()
                } else if row == start.row {
                    line[start.col..].to_string()
                } else if row == end.row {
                    line[..end.col].to_string()
                } else {
                    line.to_string()
                };
                lines.push(s);
            }
            self.yank_buffer = Some(lines.join("\n"));
            self.enter_normal();
            self.message = String::from("Yanked");
        }
    }

    pub fn delete_selection(&mut self) {
        if let Some((start, end)) = self.selected_range() {
            self.push_undo();
            let mut deleted_text = String::new();

            if self.mode == Mode::VisualLine {
                self.buffer.cursor.row = start.row;
                let count = end.row - start.row + 1;
                for _ in 0..count {
                    let line = self.buffer.delete_line();
                    if !deleted_text.is_empty() { deleted_text.push('\n'); }
                    deleted_text.push_str(&line);
                }
                self.yank_buffer = Some(deleted_text);
                self.enter_normal();
                self.message = String::from("Deleted");
                return;
            }

            if start.row == end.row {
                let line = self.buffer.line(start.row);
                let deleted: String = line.chars().skip(start.col).take(end.col - start.col).collect();
                let prefix: String = line.chars().take(start.col).collect();
                let suffix: String = line.chars().skip(end.col).collect();
                self.buffer.set_line(start.row, prefix + &suffix);
                deleted_text = deleted;
            } else {
                let first_line = self.buffer.line(start.row).to_string();
                let last_line = self.buffer.line(end.row).to_string();

                deleted_text.push_str(&first_line[start.col..]);
                for row in (start.row + 1)..end.row {
                    deleted_text.push('\n');
                    deleted_text.push_str(self.buffer.line(row));
                }
                deleted_text.push('\n');
                deleted_text.push_str(&last_line[..end.col.min(last_line.chars().count())]);

                let first_prefix: String = first_line.chars().take(start.col).collect();
                let last_suffix: String = last_line.chars().skip(end.col).collect();

                self.buffer.cursor.row = end.row;
                for _row in (start.row + 1..=end.row).rev() {
                    self.buffer.cursor.row = _row;
                    self.buffer.delete_line();
                }
                self.buffer.cursor.row = start.row;
                self.buffer.set_line(start.row, first_prefix + &last_suffix);
            }

            self.yank_buffer = Some(deleted_text);
            self.buffer.cursor = Position::new(start.row, start.col);
            self.buffer.clamp_col();
            self.enter_normal();
            self.message = String::from("Deleted");
        }
    }

    pub fn record_mtime(&mut self) {
        if let Some(ref path) = self.filename {
            self.file_mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        }
    }

    pub fn check_external_change(&mut self) {
        if let Some(ref path) = self.filename {
            if let Ok(meta) = std::fs::metadata(path) {
                if let Ok(mtime) = meta.modified() {
                    if self.file_mtime.map_or(false, |t| t != mtime) {
                        if let Ok(content) = std::fs::read_to_string(path) {
                            self.buffer = Buffer::from_string(&content);
                            self.scroll = 0;
                            self.undo_stack.push(self.buffer.snapshot());
                            self.modified = false;
                            self.file_mtime = Some(mtime);
                            self.message = String::from("Reloaded (external change)");
                        }
                    }
                }
            }
        }
    }

    pub fn save_state_to_tab(&mut self) {
        if self.current_buffer < self.buffers.len() {
            let tab = &mut self.buffers[self.current_buffer];
            tab.buffer = self.buffer.clone();
            tab.filename = self.filename.clone();
            tab.scroll = self.scroll;
            tab.modified = self.modified;
            tab.undo_stack = self.undo_stack.clone();
            tab.file_mtime = self.file_mtime;
        }
    }

    pub fn restore_state_from_tab(&mut self) {
        if let Some(tab) = self.buffers.get(self.current_buffer).cloned() {
            self.buffer = tab.buffer;
            self.filename = tab.filename;
            self.scroll = tab.scroll;
            self.modified = tab.modified;
            self.undo_stack = tab.undo_stack;
            self.file_mtime = tab.file_mtime;
        }
    }

    pub fn open_new_tab(&mut self, path: &str) {
        self.save_state_to_tab();

        let pathbuf = PathBuf::from(path);
        let abs_path = if pathbuf.is_absolute() {
            pathbuf
        } else {
            env::current_dir().unwrap_or_default().join(&pathbuf)
        };

        for (i, tab) in self.buffers.iter().enumerate() {
            if tab.filename.as_ref() == Some(&abs_path) {
                self.current_buffer = i;
                self.restore_state_from_tab();
                self.message = format!("Switched to: {}", abs_path.display());
                return;
            }
        }

        let content = fs::read_to_string(&abs_path).unwrap_or_default();
        let buffer = Buffer::from_string(&content);
        let mtime = std::fs::metadata(&abs_path).ok().and_then(|m| m.modified().ok());
        let mut undo = UndoStack::new();
        undo.push(buffer.snapshot());

        self.buffers.push(BufferTab {
            buffer,
            filename: Some(abs_path.clone()),
            scroll: 0,
            modified: false,
            undo_stack: undo,
            file_mtime: mtime,
        });
        self.current_buffer = self.buffers.len() - 1;
        self.restore_state_from_tab();
        self.lsp.auto_start(&abs_path.display().to_string());
        self.message = format!("Opened: {}", abs_path.display());
    }

    pub fn next_tab(&mut self) {
        if self.buffers.len() < 2 {
            return;
        }
        self.save_state_to_tab();
        self.current_buffer = (self.current_buffer + 1) % self.buffers.len();
        self.restore_state_from_tab();
    }

    pub fn prev_tab(&mut self) {
        if self.buffers.len() < 2 {
            return;
        }
        self.save_state_to_tab();
        if self.current_buffer == 0 {
            self.current_buffer = self.buffers.len() - 1;
        } else {
            self.current_buffer -= 1;
        }
        self.restore_state_from_tab();
    }

    pub fn close_current_tab(&mut self) {
        if self.buffers.len() <= 1 {
            self.buffer = Buffer::new();
            self.filename = None;
            self.scroll = 0;
            self.modified = false;
            self.undo_stack = UndoStack::new();
            self.undo_stack.push(self.buffer.snapshot());
            self.file_mtime = None;
            self.buffers[0] = BufferTab {
                buffer: self.buffer.clone(),
                filename: None,
                scroll: 0,
                modified: false,
                undo_stack: self.undo_stack.clone(),
                file_mtime: None,
            };
            return;
        }

        self.buffers.remove(self.current_buffer);
        if self.current_buffer >= self.buffers.len() {
            self.current_buffer = self.buffers.len() - 1;
        }
        self.restore_state_from_tab();
        self.message = String::from("Buffer closed");
    }
}

pub fn set_cursor_esc(color: ratatui::style::Color) {
    use ratatui::style::Color;
    if let Color::Rgb(r, g, b) = color {
        print!("\x1b]12;rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}\x1b\\", r, r, g, g, b, b);
        let _ = std::io::stdout().flush();
    }
}
