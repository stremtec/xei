use tree_sitter::{Parser, Tree};

use crate::highlight::TokenKind;

pub struct SyntaxEngine {
    rust: Parser,
    python: Parser,
    javascript: Parser,
    c: Parser,
    tree: Option<Tree>,
    pub tokens: Vec<(TokenKind, usize, usize, usize)>,
}

impl Default for SyntaxEngine {
    fn default() -> Self {
        let mut rust = Parser::new();
        rust.set_language(&tree_sitter_rust::LANGUAGE.into()).ok();

        let mut python = Parser::new();
        python.set_language(&tree_sitter_python::LANGUAGE.into()).ok();

        let mut javascript = Parser::new();
        javascript.set_language(&tree_sitter_javascript::LANGUAGE.into()).ok();

        let mut c = Parser::new();
        c.set_language(&tree_sitter_c::LANGUAGE.into()).ok();

            Self { rust, python, javascript, c, tree: None, tokens: Vec::new() }
    }
}

impl SyntaxEngine {
    pub fn new() -> Self { Self::default() }

    pub fn parse(&mut self, text: &str, ext: Option<&str>) {
        let parser = match ext {
            Some("rs") => &mut self.rust,
            Some("py") => &mut self.python,
            Some("js" | "jsx" | "ts" | "tsx" | "mjs") => &mut self.javascript,
            Some("c" | "h" | "cpp" | "hpp" | "cc") => &mut self.c,
            _ => { self.tokens.clear(); return; }
        };

        self.tree = parser.parse(text, self.tree.as_ref());
        self.tokens.clear();

        if let Some(ref tree) = self.tree {
            collect_tokens(tree.root_node(), &mut self.tokens);
        }
    }
}

fn collect_tokens(node: tree_sitter::Node, tokens: &mut Vec<(TokenKind, usize, usize, usize)>) {
    let kind = map_kind(node.kind());
    if let Some(tk) = kind {
        let start = node.start_position();
        let end = node.end_position();
        let scol = start.column;
        let ecol = if start.row == end.row { end.column } else { usize::MAX };
        if start.row == end.row {
            if scol < ecol {
                tokens.push((tk, scol, ecol, start.row));
            }
        } else {
            tokens.push((tk, scol, usize::MAX, start.row));
            for row in start.row + 1..end.row {
                tokens.push((tk, 0, usize::MAX, row));
            }
            tokens.push((tk, 0, end.column, end.row));
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_tokens(child, tokens);
        }
    }
}

fn map_kind(kind: &str) -> Option<TokenKind> {
    match kind {
        "comment" | "line_comment" | "block_comment" => Some(TokenKind::Comment),
        "string" | "string_content" | "string_literal" | "raw_string_literal"
        | "char_literal" | "template_string" | "template_substitution" => Some(TokenKind::String),
        "number_literal" | "float_literal" | "integer_literal" => Some(TokenKind::Number),
        "type_identifier" | "primitive_type" | "type" | "predefined_type" => Some(TokenKind::TypeName),

        "fn" | "function" | "function_item" | "function_declaration" | "function_definition"
        | "method_definition" | "arrow_function" | "lambda" | "closure_expression"
        | "let" | "let_declaration" | "variable_declaration" | "const" | "static"
        | "struct_item" | "enum_item" | "trait_item" | "impl_item" | "mod_item"
        | "use_declaration" | "import" | "import_statement" | "export_statement"
        | "match" | "if" | "if_statement" | "if_expression" | "else" | "else_clause"
        | "while" | "while_statement" | "for" | "for_statement" | "for_in_statement"
        | "loop" | "break" | "continue" | "return" | "return_statement"
        | "class" | "class_declaration" | "class_item" | "new" | "new_expression"
        | "try" | "try_statement" | "catch" | "catch_clause" | "finally" | "throw"
        | "async" | "await" | "yield" | "defer" | "go"
        | "pub" | "public" | "private" | "protected" | "unsafe" | "extern"
        | "self" | "this" | "super" | "Self" | "superclass"
        | "where" | "where_clause" | "as" | "in"
        | "switch" | "switch_statement" | "case" | "case_clause" | "default"
        | "raise" | "raise_statement" | "with" | "with_statement" | "pass"
        | "global" | "nonlocal" | "del"
        | "typedef" | "sizeof" | "goto" | "register" | "volatile"
        | "namespace" | "namespace_definition" | "template" | "typename"
        | "operator" | "explicit" | "mutable" | "friend"
        | "override" | "virtual" | "constexpr" | "noexcept" | "nullptr"
        | "macro_invocation" | "macro_definition"
        => Some(TokenKind::Keyword),

        _ => None,
    }
}
