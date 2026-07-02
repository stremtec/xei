use std::collections::HashMap;
use gpui::*;
use crate::syntax::TokenKind;

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub bg: Hsla,
    pub fg: Hsla,
    pub gutter_bg: Hsla,
    pub gutter_fg: Hsla,
    pub selection_bg: Hsla,
    pub cursor: Hsla,
    pub line_highlight: Hsla,
    pub active_tab: Hsla,
    pub inactive_tab: Hsla,
    pub status_bg: Hsla,
    pub status_fg: Hsla,
    pub colors: HashMap<TokenKind, Hsla>,
}

impl Theme {
    pub fn color_for(&self, kind: &TokenKind) -> Hsla {
        self.colors.get(kind).copied().unwrap_or(self.fg)
    }
}

impl Default for Theme {
    fn default() -> Self {
        let mut colors = HashMap::new();
        colors.insert(TokenKind::Keyword, hsla(0.72, 0.77, 0.58, 1.0));
        colors.insert(TokenKind::Type, hsla(0.28, 0.65, 0.60, 1.0));
        colors.insert(TokenKind::Function, hsla(0.58, 0.70, 0.65, 1.0));
        colors.insert(TokenKind::Variable, hsla(0.65, 0.63, 0.68, 1.0));
        colors.insert(TokenKind::String, hsla(0.11, 0.57, 0.60, 1.0));
        colors.insert(TokenKind::Number, hsla(0.17, 0.86, 0.60, 1.0));
        colors.insert(TokenKind::Comment, hsla(0.55, 0.30, 0.45, 1.0));
        colors.insert(TokenKind::Operator, hsla(0.55, 0.85, 0.70, 1.0));
        colors.insert(TokenKind::Punctuation, hsla(0.65, 0.43, 0.58, 1.0));
        colors.insert(TokenKind::Tag, hsla(0.78, 0.70, 0.60, 1.0));
        colors.insert(TokenKind::Attribute, hsla(0.57, 0.70, 0.60, 1.0));
        colors.insert(TokenKind::Constant, hsla(0.97, 0.60, 0.55, 1.0));

        Self {
            name: "seisui dark".into(),
            bg: hsla(0.65, 0.15, 0.12, 1.0),
            fg: hsla(0.60, 0.20, 0.85, 1.0),
            gutter_bg: hsla(0.65, 0.12, 0.10, 1.0),
            gutter_fg: hsla(0.60, 0.10, 0.50, 1.0),
            selection_bg: hsla(0.67, 0.35, 0.30, 1.0),
            cursor: hsla(0.60, 0.80, 0.70, 1.0),
            line_highlight: hsla(0.65, 0.15, 0.18, 1.0),
            active_tab: hsla(0.65, 0.15, 0.16, 1.0),
            inactive_tab: hsla(0.65, 0.12, 0.10, 1.0),
            status_bg: hsla(0.65, 0.25, 0.20, 1.0),
            status_fg: hsla(0.60, 0.15, 0.70, 1.0),
            colors,
        }
    }
}
