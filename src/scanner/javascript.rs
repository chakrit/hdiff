//! JavaScript's lexical pieces. Regular-expression literals are not recognized.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static JAVASCRIPT: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
        Pair::escaped_spanning("`", "`", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"$"),
    words: &[
        (
            &[
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "debugger",
                "default",
                "delete",
                "do",
                "else",
                "export",
                "extends",
                "finally",
                "for",
                "function",
                "if",
                "import",
                "in",
                "instanceof",
                "let",
                "new",
                "return",
                "static",
                "switch",
                "throw",
                "try",
                "typeof",
                "var",
                "void",
                "while",
                "with",
                "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &["false", "null", "true", "undefined"],
            SyntaxClass::Constant,
        ),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("function", SyntaxClass::Function),
        (".", SyntaxClass::Property),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: None,
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::JAVASCRIPT;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_javascript() {
        assert_eq!(
            ground(
                &JAVASCRIPT,
                "export function $get(el) { return el?.value ?? `none`; } // x"
            ),
            [
                ("export", SyntaxClass::Keyword),
                ("function", SyntaxClass::Keyword),
                ("$get", SyntaxClass::Function),
                ("el", SyntaxClass::Variable),
                ("return", SyntaxClass::Keyword),
                ("el", SyntaxClass::Variable),
                ("?", SyntaxClass::Operator),
                ("value", SyntaxClass::Property),
                ("??", SyntaxClass::Operator),
                ("`none`", SyntaxClass::String),
                ("// x", SyntaxClass::Comment),
            ]
        );
    }
}
