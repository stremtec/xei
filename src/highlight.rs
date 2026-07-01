use ratatui::style::Style;

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    String,
    Comment,
    Number,
    TypeName,
}

pub fn style_for(theme: &Theme, kind: TokenKind) -> Style {
    let fg = match kind {
        TokenKind::Keyword => theme.keyword,
        TokenKind::String => theme.string,
        TokenKind::Comment => theme.comment,
        TokenKind::Number => theme.number,
        TokenKind::TypeName => theme.type_name,
    };
    Style::default().fg(fg)
}

pub fn highlight_line(line: &str, ext: Option<&str>) -> Vec<(TokenKind, usize, usize)> {
    let rules = match ext {
        Some("rs") => rust_rules(),
        Some("py") => py_rules(),
        Some("ts" | "tsx" | "js" | "jsx") => js_rules(),
        Some("go") => go_rules(),
        Some("c" | "h") => c_rules(),
        Some("cpp" | "hpp" | "cc" | "cxx") => cpp_rules(),
        Some("sh" | "bash" | "zsh") => shell_rules(),
        Some("html" | "htm") => html_rules(),
        Some("css") => css_rules(),
        Some("toml") => toml_rules(),
        Some("yaml" | "yml") => yaml_rules(),
        Some("sql") => sql_rules(),
        Some("md" | "mdx") => md_rules(),
        _ => return vec![],
    };

    tokenize(line, &rules)
}

struct LangRules {
    line_comment: Option<&'static str>,
    keywords: &'static [&'static str],
    types: &'static [&'static str],
}

fn rust_rules() -> LangRules {
    LangRules {
        line_comment: Some("//"),
        keywords: &[
            "fn", "let", "mut", "struct", "impl", "enum", "trait", "pub",
            "use", "mod", "match", "if", "else", "while", "for", "loop",
            "return", "self", "Self", "const", "static", "type", "where",
            "unsafe", "async", "await", "move", "ref", "true", "false",
            "Some", "None", "Ok", "Err", "as", "in", "break", "continue",
            "extern", "crate", "super", "dyn", "macro_rules!",
        ],
        types: &[
            "i8", "i16", "i32", "i64", "i128", "isize",
            "u8", "u16", "u32", "u64", "u128", "usize",
            "f32", "f64", "bool", "char", "str",
            "String", "Vec", "HashMap", "Option", "Result",
            "Box", "Rc", "Arc", "Cell", "RefCell", "Mutex", "RwLock",
            "Clone", "Copy", "Debug", "Default", "Drop", "From", "Into",
            "Iterator", "Display", "PartialEq", "Eq", "PartialOrd", "Ord",
        ],
    }
}

fn py_rules() -> LangRules {
    LangRules {
        line_comment: Some("#"),
        keywords: &[
            "def", "class", "import", "from", "if", "elif", "else", "for",
            "while", "try", "except", "finally", "raise", "with", "as",
            "return", "yield", "lambda", "async", "await", "pass", "break",
            "continue", "and", "or", "not", "in", "is", "True", "False", "None",
            "global", "nonlocal", "del",
        ],
        types: &[
            "int", "float", "str", "bool", "list", "dict", "set", "tuple",
            "object", "type", "Exception", "ValueError", "TypeError",
        ],
    }
}

fn js_rules() -> LangRules {
    LangRules {
        line_comment: Some("//"),
        keywords: &[
            "function", "const", "let", "var", "class", "interface", "type",
            "enum", "import", "export", "default", "async", "await", "return",
            "if", "else", "for", "while", "switch", "case", "try", "catch",
            "throw", "new", "extends", "implements", "private", "protected",
            "public", "readonly", "static", "abstract", "typeof", "keyof",
            "as", "in", "null", "undefined", "true", "false", "this", "super",
        ],
        types: &[
            "number", "string", "boolean", "void", "any", "unknown", "never",
            "Array", "Promise", "Map", "Set", "Date", "Error", "RegExp",
        ],
    }
}

fn go_rules() -> LangRules {
    LangRules {
        line_comment: Some("//"),
        keywords: &[
            "func", "var", "const", "type", "struct", "interface", "package",
            "import", "if", "else", "for", "range", "switch", "case", "default",
            "defer", "go", "chan", "select", "return", "break", "continue",
            "map", "nil", "true", "false",
        ],
        types: &[
            "int", "int8", "int16", "int32", "int64",
            "uint", "uint8", "uint16", "uint32", "uint64",
            "float32", "float64", "string", "bool", "byte", "rune",
            "error", "interface",
        ],
    }
}

fn c_rules() -> LangRules {
    LangRules {
        line_comment: Some("//"),
        keywords: &[
            "if", "else", "for", "while", "do", "switch", "case", "break",
            "continue", "return", "struct", "union", "enum", "typedef",
            "sizeof", "static", "extern", "const", "volatile", "register",
            "auto", "unsigned", "signed", "short", "long", "void", "NULL",
            "true", "false", "goto",
        ],
        types: &[
            "int", "char", "float", "double", "size_t", "ssize_t",
            "int8_t", "int16_t", "int32_t", "int64_t",
            "uint8_t", "uint16_t", "uint32_t", "uint64_t",
        ],
    }
}

fn cpp_rules() -> LangRules {
    LangRules {
        line_comment: Some("//"),
        keywords: &[
            "class", "namespace", "public", "private", "protected", "virtual",
            "override", "template", "typename", "new", "delete", "this",
            "nullptr", "constexpr", "noexcept", "friend", "operator",
            "explicit", "mutable", "using", "auto", "decltype", "try",
            "catch", "throw", "if", "else", "for", "while", "do", "switch",
            "case", "break", "continue", "return", "struct", "union", "enum",
            "typedef", "sizeof", "static", "extern", "const", "volatile",
            "unsigned", "signed", "short", "long", "void", "true", "false",
            "goto",
        ],
        types: &[
            "int", "char", "float", "double", "bool", "size_t",
            "std::string", "std::vector", "std::map", "std::set",
            "std::unique_ptr", "std::shared_ptr", "std::weak_ptr",
        ],
    }
}

fn shell_rules() -> LangRules {
    LangRules {
        line_comment: Some("#"),
        keywords: &[
            "if", "then", "else", "elif", "fi", "for", "while", "do",
            "done", "case", "esac", "function", "local", "export", "source",
            "exit", "return", "echo", "read", "test", "shift", "unset",
            "alias", "trap",
        ],
        types: &[],
    }
}

fn html_rules() -> LangRules {
    LangRules {
        line_comment: None,
        keywords: &[],
        types: &[],
    }
}

fn css_rules() -> LangRules {
    LangRules {
        line_comment: None,
        keywords: &[
            "!important",
        ],
        types: &[],
    }
}

fn toml_rules() -> LangRules {
    LangRules {
        line_comment: Some("#"),
        keywords: &["true", "false"],
        types: &[],
    }
}

fn yaml_rules() -> LangRules {
    LangRules {
        line_comment: Some("#"),
        keywords: &["true", "false", "null", "yes", "no"],
        types: &[],
    }
}

fn sql_rules() -> LangRules {
    LangRules {
        line_comment: Some("--"),
        keywords: &[
            "SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE",
            "SET", "DELETE", "CREATE", "TABLE", "ALTER", "DROP", "JOIN",
            "LEFT", "INNER", "ON", "GROUP", "BY", "ORDER", "HAVING",
            "LIMIT", "OFFSET", "INDEX", "PRIMARY", "KEY", "FOREIGN",
            "NOT", "NULL", "DEFAULT", "UNIQUE", "AS", "DISTINCT",
            "COUNT", "SUM", "AVG", "MAX", "MIN", "AND", "OR",
        ],
        types: &[],
    }
}

fn md_rules() -> LangRules {
    LangRules {
        line_comment: None,
        keywords: &[],
        types: &[],
    }
}

fn tokenize(line: &str, rules: &LangRules) -> Vec<(TokenKind, usize, usize)> {
    let mut tokens: Vec<(TokenKind, usize, usize)> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    if let Some(comment_prefix) = rules.line_comment {
        if let Some(pos) = find_not_in_string(&chars, comment_prefix) {
            if pos < len {
                tokens.push((TokenKind::Comment, pos, len));
                if pos == 0 {
                    return tokens;
                }
            }
        }
    }

    while i < len {
        if chars[i] == '"' || chars[i] == '\'' {
            let quote = chars[i];
            let start = i;
            i += 1;
            while i < len && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < len {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < len {
                i += 1;
            }
            tokens.push((TokenKind::String, start, i));
            continue;
        }

        if chars[i].is_ascii_digit() {
            let start = i;
            while i < len && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '_' || chars[i] == 'x' || chars[i] == 'X' || chars[i] == 'e' || chars[i] == 'E') {
                i += 1;
            }
            tokens.push((TokenKind::Number, start, i));
            continue;
        }

        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '!') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();

            if rules.keywords.contains(&word.as_str()) {
                tokens.push((TokenKind::Keyword, start, i));
            } else if rules.types.contains(&word.as_str()) {
                tokens.push((TokenKind::TypeName, start, i));
            }
            continue;
        }

        i += 1;
    }

    tokens
}

fn find_not_in_string(chars: &[char], needle: &str) -> Option<usize> {
    let n: Vec<char> = needle.chars().collect();
    let n_len = n.len();
    let mut i = 0;
    let mut in_string = false;
    let mut string_char = '"';

    while i + n_len <= chars.len() {
        let c = chars[i];

        if (c == '"' || c == '\'') && (i == 0 || chars[i - 1] != '\\') {
            if !in_string {
                in_string = true;
                string_char = c;
            } else if c == string_char {
                in_string = false;
            }
        }

        if !in_string && chars[i..i + n_len] == n[..] {
            return Some(i);
        }

        i += 1;
    }

    None
}
