use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

pub struct Terminal {
    pub open: bool,
    rows: Vec<Vec<Cell>>,
    cursor_row: usize,
    cursor_col: usize,
    cols: u16,
    rows_count: u16,
    scroll_offset: usize,
    child: Option<Child>,
    stdin: Option<Box<dyn Write + Send>>,
    rx: Option<Receiver<Vec<u8>>>,
    scrollback: Vec<Vec<Cell>>,
    fg: Color,
    bg: Color,
    bold: bool,
    pub started: bool,
}

#[derive(Clone)]
struct Cell {
    ch: char,
    fg: Option<Color>,
    bg: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Default, Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
    BrightBlack, BrightRed, BrightGreen, BrightYellow,
    BrightBlue, BrightMagenta, BrightCyan, BrightWhite,
}

impl Color {
    fn to_ratatui(self) -> ratatui::style::Color {
        match self {
            Color::Default => ratatui::style::Color::Reset,
            Color::Black => ratatui::style::Color::Black,
            Color::Red => ratatui::style::Color::Red,
            Color::Green => ratatui::style::Color::Green,
            Color::Yellow => ratatui::style::Color::Yellow,
            Color::Blue => ratatui::style::Color::Blue,
            Color::Magenta => ratatui::style::Color::Magenta,
            Color::Cyan => ratatui::style::Color::Cyan,
            Color::White => ratatui::style::Color::White,
            Color::BrightBlack => ratatui::style::Color::Gray,
            Color::BrightRed => ratatui::style::Color::LightRed,
            Color::BrightGreen => ratatui::style::Color::LightGreen,
            Color::BrightYellow => ratatui::style::Color::LightYellow,
            Color::BrightBlue => ratatui::style::Color::LightBlue,
            Color::BrightMagenta => ratatui::style::Color::LightMagenta,
            Color::BrightCyan => ratatui::style::Color::LightCyan,
            Color::BrightWhite => ratatui::style::Color::White,
        }
    }
}

impl Default for Terminal {
    fn default() -> Self {
        let (cols, rows) = (80, 24);
        Self {
            open: false,
            rows: vec![vec![Cell { ch: ' ', fg: None, bg: None }; cols as usize]; rows as usize],
            cursor_row: 0, cursor_col: 0, cols, rows_count: rows, scroll_offset: 0,
            child: None, stdin: None, rx: None, scrollback: Vec::new(),
            fg: Color::Default, bg: Color::Default, bold: false, started: false,
        }
    }
}

impl Terminal {
    pub fn new() -> Self { Self::default() }

    pub fn start(&mut self, anchor: Option<&PathBuf>) {
        if self.started { return; }

        let cwd = anchor
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

        let mut child = match Command::new("/usr/bin/script")
            .args(["-q", "/dev/null", &shell])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&cwd)
            .env("TERM", "xterm-256color")
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return,
        };

        let stdin_writer = child.stdin.take().unwrap();
        let mut stdout_reader = child.stdout.take().unwrap();

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match stdout_reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() { break; }
                    }
                }
            }
        });

        self.stdin = Some(Box::new(stdin_writer));
        self.rx = Some(rx);
        self.child = Some(child);
        self.open = true;
        self.started = true;
    }

    pub fn shutdown(&mut self) {
        if let Some(ref mut stdin) = self.stdin {
            let _ = stdin.write_all(b"exit\n");
            let _ = stdin.flush();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stdin = None;
        self.rx = None;
        self.started = false;
        self.open = false;
        self.rows = vec![vec![Cell { ch: ' ', fg: None, bg: None }; self.cols as usize]; self.rows_count as usize];
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.scrollback.clear();
        self.fg = Color::Default;
        self.bg = Color::Default;
        self.bold = false;
    }

    pub fn write_input(&mut self, bytes: &[u8]) {
        if let Some(ref mut stdin) = self.stdin {
            let _ = stdin.write_all(bytes);
            let _ = stdin.flush();
        }
    }

    pub fn poll(&mut self) {
        let data = if let Some(ref rx) = self.rx {
            let mut all = Vec::new();
            loop {
                match rx.try_recv() {
                    Ok(part) => all.extend_from_slice(&part),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                }
            }
            if all.is_empty() { return; }
            all
        } else { return; };
        self.process_output(&data);
    }

    fn process_output(&mut self, data: &[u8]) {
        let mut i = 0;
        while i < data.len() {
            if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'[' {
                i = self.parse_csi(data, i + 2);
            } else if data[i] == b'\n' { self.newline(); i += 1; }
            else if data[i] == b'\r' { self.cursor_col = 0; i += 1; }
            else if data[i] == 0x08 { if self.cursor_col > 0 { self.cursor_col -= 1; } i += 1; }
            else if data[i] == b'\t' {
                self.write_char(' ');
                while self.cursor_col % 4 != 0 && self.cursor_col < self.cols as usize { self.write_char(' '); }
                i += 1;
            }
            else if data[i] == 0x07 { i += 1; }
            else if data[i] >= 0x20 { self.write_char(data[i] as char); i += 1; }
            else { i += 1; }
        }
    }

    fn parse_csi(&mut self, data: &[u8], start: usize) -> usize {
        let mut i = start;
        let mut nums = Vec::new();
        let mut cur = String::new();
        while i < data.len() {
            let b = data[i];
            if b == b'?' || b == b'>' {
                i += 1;
                while i < data.len() && !data[i].is_ascii_alphabetic() && data[i] != b'@' { i += 1; }
                if i < data.len() { i += 1; }
                return i;
            }
            if b.is_ascii_digit() { cur.push(b as char); }
            else if b == b';' { nums.push(cur.parse::<i32>().unwrap_or(0)); cur.clear(); }
            else if b.is_ascii_alphabetic() || b == b'@' || b == b'm' {
                nums.push(cur.parse::<i32>().unwrap_or(0));
                self.apply_csi(b as char, &nums);
                return i + 1;
            } else { return i; }
            i += 1;
        }
        i
    }

    fn apply_csi(&mut self, cmd: char, nums: &[i32]) {
        let n = |i: usize, d: i32| -> i32 { nums.get(i).copied().filter(|&v| v != 0).unwrap_or(d) };
        match cmd {
            'A' => self.cursor_row = self.cursor_row.saturating_sub(n(0,1).max(1) as usize),
            'B' => self.cursor_row = (self.cursor_row + n(0,1).max(1) as usize).min(self.rows_count as usize - 1),
            'C' => self.cursor_col = (self.cursor_col + n(0,1).max(1) as usize).min(self.cols as usize - 1),
            'D' => self.cursor_col = self.cursor_col.saturating_sub(n(0,1).max(1) as usize),
            'H'|'f' => {
                self.cursor_row = (n(0,1).max(1) as usize - 1).min(self.rows_count as usize - 1);
                self.cursor_col = (n(1,1).max(1) as usize - 1).min(self.cols as usize - 1);
            }
            'J' => {
                let m = n(0,0);
                if m == 2 || m == 3 {
                    for row in &mut self.rows { for c in row.iter_mut() { *c = Cell { ch: ' ', fg: None, bg: None }; } }
                    self.cursor_row = 0; self.cursor_col = 0;
                } else if m == 0 {
                    for c in self.cursor_col..self.cols as usize { self.rows[self.cursor_row][c] = Cell { ch: ' ', fg: None, bg: None }; }
                    for r in self.cursor_row+1..self.rows_count as usize { for c in 0..self.cols as usize { self.rows[r][c] = Cell { ch: ' ', fg: None, bg: None }; } }
                } else if m == 1 {
                    for r in 0..self.cursor_row { for c in 0..self.cols as usize { self.rows[r][c] = Cell { ch: ' ', fg: None, bg: None }; } }
                    for c in 0..=self.cursor_col { self.rows[self.cursor_row][c] = Cell { ch: ' ', fg: None, bg: None }; }
                }
            }
            'K' => {
                let m = n(0,0); let r = self.cursor_row;
                let range: Box<dyn Iterator<Item = usize>> = if m == 0 { Box::new(self.cursor_col..self.cols as usize) } else if m == 1 { Box::new(0..=self.cursor_col) } else { Box::new(0..self.cols as usize) };
                for c in range { self.rows[r][c] = Cell { ch: ' ', fg: None, bg: None }; }
            }
            'm' => {
                let modes: Vec<i32> = if nums.is_empty() { vec![0] } else { nums.to_vec() };
                let mut idx = 0;
                while idx < modes.len() {
                    match modes[idx] {
                        0 => { self.fg = Color::Default; self.bg = Color::Default; self.bold = false; }
                        1 => { self.bold = true; }
                        22 => { self.bold = false; }
                        30..=37 => { self.fg = ansi_to_color(modes[idx] - 30); }
                        38 => { if idx+2 < modes.len() && modes[idx+1] == 5 { self.fg = index_to_color(modes[idx+2]); idx += 2; } }
                        39 => { self.fg = Color::Default; }
                        40..=47 => { self.bg = ansi_to_color(modes[idx] - 40); }
                        48 => { if idx+2 < modes.len() && modes[idx+1] == 5 { self.bg = index_to_color(modes[idx+2]); idx += 2; } }
                        49 => { self.bg = Color::Default; }
                        90..=97 => { self.fg = bright_to_color(modes[idx] - 90); }
                        100..=107 => { self.bg = bright_to_color(modes[idx] - 100); }
                        _ => {}
                    }
                    idx += 1;
                }
            }
            _ => {}
        }
    }

    fn newline(&mut self) {
        if self.cursor_row + 1 >= self.rows_count as usize {
            self.scrollback.push(self.rows[0].clone());
            self.rows.remove(0);
            self.rows.push(vec![Cell { ch: ' ', fg: None, bg: None }; self.cols as usize]);
        } else { self.cursor_row += 1; }
        self.cursor_col = 0;
    }

    fn write_char(&mut self, ch: char) {
        if self.cursor_col >= self.cols as usize { self.newline(); }
        let fg = if self.fg != Color::Default { Some(self.fg) } else { None };
        let bg = if self.bg != Color::Default { Some(self.bg) } else { None };
        self.rows[self.cursor_row][self.cursor_col] = Cell { ch, fg, bg };
        self.cursor_col += 1;
    }

    pub fn visible_rows(&self) -> Vec<Vec<(String, Option<ratatui::style::Color>, Option<ratatui::style::Color>)>> {
        self.rows.iter().map(|row| row.iter().map(|cell| (cell.ch.to_string(), cell.fg.map(|c| c.to_ratatui()), cell.bg.map(|c| c.to_ratatui()))).collect()).collect()
    }

    pub fn scroll(&self) -> usize { self.scroll_offset }
    pub fn scroll_up(&mut self, a: usize) { self.scroll_offset = self.scroll_offset.saturating_add(a).min(self.scrollback.len()); }
    pub fn scroll_down(&mut self, a: usize) { self.scroll_offset = self.scroll_offset.saturating_sub(a); }
    pub fn scrollback_len(&self) -> usize { self.scrollback.len() }

    pub fn visible_scrollback(&self) -> Vec<Vec<(String, Option<ratatui::style::Color>, Option<ratatui::style::Color>)>> {
        self.scrollback.iter().map(|row| row.iter().map(|cell| (cell.ch.to_string(), cell.fg.map(|c| c.to_ratatui()), None)).collect()).collect()
    }

    pub fn cursor_position(&self) -> (u16, u16) { (self.cursor_col as u16, self.cursor_row as u16) }
}

fn ansi_to_color(i: i32) -> Color { match i { 0=>Color::Black,1=>Color::Red,2=>Color::Green,3=>Color::Yellow,4=>Color::Blue,5=>Color::Magenta,6=>Color::Cyan,7=>Color::White, _=>Color::Default } }
fn bright_to_color(i: i32) -> Color { match i { 0=>Color::BrightBlack,1=>Color::BrightRed,2=>Color::BrightGreen,3=>Color::BrightYellow,4=>Color::BrightBlue,5=>Color::BrightMagenta,6=>Color::BrightCyan,7=>Color::BrightWhite, _=>Color::Default } }
fn index_to_color(i: i32) -> Color { match i { 0..=7=>ansi_to_color(i), 8..=15=>bright_to_color(i-8), _=>Color::Default } }
