pub struct ExtState {
    pub editor_text: String,
    pub cursor_line: usize,
    pub cursor_column: usize,
}

impl ExtState {
    pub fn new() -> Self {
        Self {
            editor_text: String::new(),
            cursor_line: 0,
            cursor_column: 0,
        }
    }
}
