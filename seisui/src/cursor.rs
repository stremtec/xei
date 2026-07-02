#[derive(Clone, Debug, PartialEq, Default)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub start: Position,
    pub end: Position,
    pub active: bool,
}

impl Default for Selection {
    fn default() -> Self {
        Self::from_positions(Position::default(), Position::default())
    }
}

impl Selection {
    pub fn from_positions(start: Position, end: Position) -> Self {
        Self { start, end, active: true }
    }
}

pub struct Cursor {
    pub position: Position,
    pub selections: Vec<Selection>,
}

impl Default for Cursor {
    fn default() -> Self {
        Self::new()
    }
}

impl Cursor {
    pub fn new() -> Self {
        Self {
            position: Position::default(),
            selections: vec![Selection::default()],
        }
    }
}
