//! Crystal's lexical pieces. Symbols, regular expressions, `%` literals and heredocs are
//! not recognized.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static CRYSTAL: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::character("'", SyntaxClass::String),
        Pair::escaped_spanning("`", "`", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "abstract",
                "alias",
                "annotation",
                "as",
                "asm",
                "begin",
                "break",
                "case",
                "class",
                "def",
                "do",
                "else",
                "elsif",
                "end",
                "ensure",
                "enum",
                "extend",
                "for",
                "fun",
                "if",
                "in",
                "include",
                "lib",
                "macro",
                "module",
                "next",
                "of",
                "pointerof",
                "private",
                "protected",
                "require",
                "rescue",
                "return",
                "sizeof",
                "struct",
                "super",
                "then",
                "typeof",
                "uninitialized",
                "union",
                "unless",
                "until",
                "verbatim",
                "when",
                "while",
                "with",
                "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "__DIR__",
                "__END_LINE__",
                "__FILE__",
                "__LINE__",
                "false",
                "nil",
                "true",
            ],
            SyntaxClass::Constant,
        ),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("def", SyntaxClass::Function),
        (".", SyntaxClass::Function),
        ("@", SyntaxClass::Property),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"?!",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::CRYSTAL;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &CRYSTAL,
                "def initialize(@name : String, @tags = [] of String); @tags.empty? # new"
            ),
            [
                ("def", SyntaxClass::Keyword),
                ("initialize", SyntaxClass::Function),
                ("name", SyntaxClass::Property),
                ("String", SyntaxClass::Type),
                ("tags", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("of", SyntaxClass::Keyword),
                ("String", SyntaxClass::Type),
                ("tags", SyntaxClass::Property),
                ("empty?", SyntaxClass::Function),
                ("# new", SyntaxClass::Comment),
            ]
        );
    }
}
