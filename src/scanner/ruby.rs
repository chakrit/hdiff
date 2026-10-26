//! Ruby's lexical pieces. Symbols, regular expressions, `%` literals and heredocs are not
//! recognized; `=begin` opens a comment anywhere on a line.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static RUBY: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("=begin", "=end", SyntaxClass::Comment),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::escaped_spanning("'", "'", SyntaxClass::String),
        Pair::escaped_spanning("`", "`", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"$"),
    words: &[
        (
            &[
                "BEGIN",
                "END",
                "alias",
                "and",
                "begin",
                "break",
                "case",
                "class",
                "def",
                "defined?",
                "do",
                "else",
                "elsif",
                "end",
                "ensure",
                "for",
                "if",
                "in",
                "module",
                "next",
                "not",
                "or",
                "private",
                "protected",
                "public",
                "raise",
                "redo",
                "rescue",
                "retry",
                "return",
                "then",
                "undef",
                "unless",
                "until",
                "when",
                "while",
                "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "attr_accessor",
                "attr_reader",
                "attr_writer",
                "extend",
                "include",
                "prepend",
                "require",
                "require_relative",
            ],
            SyntaxClass::Function,
        ),
        (
            &[
                "__ENCODING__",
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
    capitalized: None,
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"?!",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::RUBY;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &RUBY,
                r#"def admin?(user) = user.roles.include?("admin") && !@locked # check"#
            ),
            [
                ("def", SyntaxClass::Keyword),
                ("admin?", SyntaxClass::Function),
                ("user", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("user", SyntaxClass::Variable),
                ("roles", SyntaxClass::Function),
                ("include?", SyntaxClass::Function),
                (r#""admin""#, SyntaxClass::String),
                ("&&", SyntaxClass::Operator),
                ("!", SyntaxClass::Operator),
                ("locked", SyntaxClass::Property),
                ("# check", SyntaxClass::Comment),
            ]
        );
    }
}
