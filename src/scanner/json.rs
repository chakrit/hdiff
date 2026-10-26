//! JSON's lexical pieces, with JSONC comments and JSON5 single quotes and bare keys.
//! Quoted object keys read as strings, not apart from values.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static JSON: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[(
        &["Infinity", "NaN", "false", "null", "true"],
        SyntaxClass::Constant,
    )],
    before: &[(":", SyntaxClass::Property)],
    after: &[],
    upper_case: None,
    capitalized: None,
    other_words: None,
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::JSON;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &JSON,
                r#"{"name": "hdiff", "size": -1.5e3, "ok": true, "tags": [null]} // note"#
            ),
            [
                (r#""name""#, SyntaxClass::String),
                (r#""hdiff""#, SyntaxClass::String),
                (r#""size""#, SyntaxClass::String),
                ("-", SyntaxClass::Operator),
                ("1.5e3", SyntaxClass::Number),
                (r#""ok""#, SyntaxClass::String),
                ("true", SyntaxClass::Constant),
                (r#""tags""#, SyntaxClass::String),
                ("null", SyntaxClass::Constant),
                ("// note", SyntaxClass::Comment),
            ]
        );
    }
}
