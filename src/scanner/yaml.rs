//! YAML's lexical pieces. Quoted keys read as strings; a plain scalar followed by `:`
//! (such as `nginx:1.25` or `http://`) reads as a key; block scalars, anchors' and tags'
//! sigils, and a negative number read as strings or operators.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static YAML: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"-./"),
    words: &[
        (
            &[
                "FALSE", "False", "NULL", "Null", "TRUE", "True", "false", "null", "true",
            ],
            SyntaxClass::Constant,
        ),
        (&["-", "---", "..."], SyntaxClass::Operator),
    ],
    before: &[(":", SyntaxClass::Property)],
    after: &[
        ("!", SyntaxClass::Type),
        ("!!", SyntaxClass::Type),
        ("&", SyntaxClass::Variable),
        ("*", SyntaxClass::Variable),
    ],
    upper_case: None,
    capitalized: None,
    other_words: Some(SyntaxClass::String),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::YAML;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &YAML,
                "- { name: web-app, port: 8080, enabled: true, path: /srv/www } # main"
            ),
            [
                ("-", SyntaxClass::Operator),
                ("name", SyntaxClass::Property),
                ("web-app", SyntaxClass::String),
                ("port", SyntaxClass::Property),
                ("8080", SyntaxClass::Number),
                ("enabled", SyntaxClass::Property),
                ("true", SyntaxClass::Constant),
                ("path", SyntaxClass::Property),
                ("/srv/www", SyntaxClass::String),
                ("# main", SyntaxClass::Comment),
            ]
        );
    }
}
