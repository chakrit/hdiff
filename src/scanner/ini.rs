//! INI's lexical pieces, also read for systemd units and Git config. Values read as
//! strings; a key with spaces or set with `:`, and a comment after a value, are not
//! recognized, and a value's leading `/` right after `=` joins the operator.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static INI: Spec = Spec {
    line_markers: &[
        Marker::at_line_start(";", SyntaxClass::Comment),
        Marker::at_line_start("#", SyntaxClass::Comment),
    ],
    delimited: &[Pair::escaped("\"", SyntaxClass::String)],
    word_bytes: WordBytes::with(b"-./"),
    words: &[],
    before: &[("=", SyntaxClass::Property)],
    after: &[("[", SyntaxClass::Type)],
    upper_case: None,
    capitalized: None,
    other_words: Some(SyntaxClass::String),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::INI;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(&INI, "ExecStart = /usr/bin/app --port 8080"),
            [
                ("ExecStart", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("/usr/bin/app", SyntaxClass::String),
                ("--port", SyntaxClass::String),
                ("8080", SyntaxClass::Number),
            ]
        );
    }
}
