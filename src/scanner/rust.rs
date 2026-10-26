//! Rust's lexical pieces.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static RUST: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::nesting("/*", "*/", SyntaxClass::Comment),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::spanning("r\"", "\"", SyntaxClass::String),
        Pair::spanning("r#\"", "\"#", SyntaxClass::String),
        Pair::spanning("r##\"", "\"##", SyntaxClass::String),
        Pair::character("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "as", "async", "await", "break", "const", "continue", "crate", "default", "dyn",
                "else", "enum", "extern", "fn", "for", "if", "impl", "in", "let", "loop", "match",
                "mod", "move", "mut", "pub", "ref", "return", "static", "struct", "super", "trait",
                "type", "unsafe", "use", "where", "while", "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "bool", "char", "f32", "f64", "i128", "i16", "i32", "i64", "i8", "isize", "str",
                "u128", "u16", "u32", "u64", "u8", "usize",
            ],
            SyntaxClass::Type,
        ),
        (&["false", "true"], SyntaxClass::Constant),
    ],
    before: &[
        ("(", SyntaxClass::Function),
        ("!(", SyntaxClass::Function),
        ("![", SyntaxClass::Function),
        ("!{", SyntaxClass::Function),
    ],
    after: &[("fn", SyntaxClass::Function), (".", SyntaxClass::Property)],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: None,
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::RUST;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_rust() {
        assert_eq!(
            ground(
                &RUST,
                r#"pub fn len<'a>(s: &'a str) -> usize { println!("{}", s.0); MAX_LEN }"#
            ),
            [
                ("pub", SyntaxClass::Keyword),
                ("fn", SyntaxClass::Keyword),
                ("len", SyntaxClass::Function),
                ("<", SyntaxClass::Operator),
                (">", SyntaxClass::Operator),
                ("&", SyntaxClass::Operator),
                ("str", SyntaxClass::Type),
                ("->", SyntaxClass::Operator),
                ("usize", SyntaxClass::Type),
                ("println", SyntaxClass::Function),
                ("!", SyntaxClass::Operator),
                (r#""{}""#, SyntaxClass::String),
                ("0", SyntaxClass::Number),
                ("MAX_LEN", SyntaxClass::Constant),
            ]
        );
        assert_eq!(
            ground(&RUST, r##"let raw = r#"a "b""#; Some(Vec::new())"##),
            [
                ("let", SyntaxClass::Keyword),
                ("=", SyntaxClass::Operator),
                (r##"r#"a "b""#"##, SyntaxClass::String),
                ("Some", SyntaxClass::Function),
                ("Vec", SyntaxClass::Type),
                ("::", SyntaxClass::Operator),
                ("new", SyntaxClass::Function),
            ]
        );
    }
}
