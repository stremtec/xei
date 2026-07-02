use crate::theme::Theme;

pub struct App {
    pub theme: Theme,
}

impl App {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
        }
    }
}
