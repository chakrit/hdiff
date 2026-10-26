//! Gleam's lexical pieces. Labelled arguments such as `with: x` and attributes such as
//! `@external` are not recognized.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static GLEAM: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[Pair::escaped_spanning("\"", "\"", SyntaxClass::String)],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "as", "assert", "case", "const", "echo", "else", "fn", "if", "import", "let",
                "opaque", "panic", "pub", "todo", "type", "use",
            ],
            SyntaxClass::Keyword,
        ),
        (&["False", "Nil", "True"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("fn", SyntaxClass::Function),
        ("type", SyntaxClass::Type),
        (".", SyntaxClass::Property),
    ],
    upper_case: Some(SyntaxClass::Type),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::GLEAM;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &GLEAM,
                r#"pub fn greet(name: String) -> String { case name { "" -> "Hi" _ -> string.append("Hi ", name) } } // greet"#
            ),
            [
                ("pub", SyntaxClass::Keyword),
                ("fn", SyntaxClass::Keyword),
                ("greet", SyntaxClass::Function),
                ("name", SyntaxClass::Variable),
                ("String", SyntaxClass::Type),
                ("->", SyntaxClass::Operator),
                ("String", SyntaxClass::Type),
                ("case", SyntaxClass::Keyword),
                ("name", SyntaxClass::Variable),
                (r#""""#, SyntaxClass::String),
                ("->", SyntaxClass::Operator),
                (r#""Hi""#, SyntaxClass::String),
                ("_", SyntaxClass::Variable),
                ("->", SyntaxClass::Operator),
                ("string", SyntaxClass::Variable),
                ("append", SyntaxClass::Function),
                (r#""Hi ""#, SyntaxClass::String),
                ("name", SyntaxClass::Variable),
                ("// greet", SyntaxClass::Comment),
            ]
        );
    }
}
