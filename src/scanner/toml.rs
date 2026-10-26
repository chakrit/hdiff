//! TOML's lexical pieces. Quoted keys read as strings, dates and times as numbers and
//! operators, and a negative number as a key.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static TOML: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::spanning("'''", "'''", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::spanning("'", "'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"-"),
    words: &[
        (&["false", "true"], SyntaxClass::Constant),
        (&["inf", "nan"], SyntaxClass::Number),
    ],
    before: &[],
    after: &[],
    upper_case: None,
    capitalized: None,
    other_words: Some(SyntaxClass::Property),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::TOML;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &TOML,
                r#"serde-json = { version = "1.0", features = ['std'], optional = true } # json"#
            ),
            [
                ("serde-json", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("version", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                (r#""1.0""#, SyntaxClass::String),
                ("features", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("'std'", SyntaxClass::String),
                ("optional", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("true", SyntaxClass::Constant),
                ("# json", SyntaxClass::Comment),
            ]
        );
    }
}
