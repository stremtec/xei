use crate::buffer::Language;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Keyword,
    Type,
    Function,
    Variable,
    String,
    Number,
    Comment,
    Operator,
    Punctuation,
    Tag,
    Attribute,
    Constant,
}

pub struct SyntaxEngine {
    parser: tree_sitter::Parser,
    tree: Option<tree_sitter::Tree>,
    cached_source: Vec<u8>,
    language: Option<Language>,
    rust_lang: tree_sitter::Language,
}

impl SyntaxEngine {
    pub fn new() -> Self {
        Self {
            parser: tree_sitter::Parser::new(),
            tree: None,
            cached_source: Vec::new(),
            language: None,
            rust_lang: tree_sitter_rust::LANGUAGE.into(),
        }
    }

    pub fn set_language(&mut self, lang: Language) {
        if self.language != Some(lang.clone()) {
            self.language = Some(lang);
            self.tree = None;
            self.cached_source.clear()
        }
    }

    fn ts_language(&self, lang: &Language) -> Option<tree_sitter::Language> {
        match lang {
            Language::Rust => Some(self.rust_lang.clone()),
            _ => None,
        }
    }

    pub fn parse(&mut self, source: &str, lang: &Language) {
        self.set_language(lang.clone());
        let Some(ts_lang) = self.ts_language(lang) else { return };
        let bytes = source.as_bytes();

        if bytes == self.cached_source && self.tree.is_some() {
            return;
        }

        self.parser.set_language(&ts_lang).ok();
        self.tree = self.parser.parse(source, self.tree.as_ref());
        self.cached_source = bytes.to_vec();
    }

    pub fn tokens_for_range(
        &self,
        source: &str,
        range_start: usize,
        range_end: usize,
    ) -> Vec<(usize, usize, TokenKind)> {
        let mut tokens = Vec::new();
        let Some(tree) = &self.tree else { return tokens };
        let root = tree.root_node();
        collect_node_tokens(root, source, range_start, range_end, &mut tokens);
        tokens
    }
}

fn collect_node_tokens(
    node: tree_sitter::Node,
    source: &str,
    range_start: usize,
    range_end: usize,
    tokens: &mut Vec<(usize, usize, TokenKind)>,
) {
    if node.end_byte() < range_start || node.start_byte() > range_end {
        return;
    }

    if node.child_count() == 0 {
        let start = node.start_byte();
        let end = node.end_byte();
        if end > range_start && start < range_end {
            let kind = classify_node_kind(node.kind());
            if kind != TokenKind::Variable {
                tokens.push((start, end, kind));
            }
        }
        return;
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_node_tokens(child, source, range_start, range_end, tokens);
        }
    }
}

fn classify_node_kind(kind: &str) -> TokenKind {
    match kind {
        "fn" | "let" | "mut" | "if" | "else" | "match" | "for" | "while" | "loop"
        | "return" | "break" | "continue" | "in" | "where" | "as" | "use" | "mod"
        | "pub" | "crate" | "super" | "self" | "Self" | "struct" | "enum" | "trait"
        | "impl" | "type" | "const" | "static" | "ref" | "move" | "async" | "await"
        | "unsafe" | "extern" | "true" | "false" | "macro_rules!" => TokenKind::Keyword,
        "identifier" | "field_identifier" | "type_identifier" | "constant_identifier" => TokenKind::Variable,
        "string_literal" | "raw_string_literal" | "character_literal" => TokenKind::String,
        "integer_literal" | "float_literal" | "boolean_literal" => TokenKind::Number,
        "line_comment" | "block_comment" | "doc_comment" => TokenKind::Comment,
        "primitive_type" => TokenKind::Type,
        _ => {
            if kind.contains("string") || kind.contains("char") {
                TokenKind::String
            } else if kind.contains("comment") {
                TokenKind::Comment
            } else if kind.contains("number") || kind.contains("integer") || kind.contains("float") {
                TokenKind::Number
            } else {
                TokenKind::Variable
            }
        }
    }
}
