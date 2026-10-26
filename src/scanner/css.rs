//! CSS's lexical pieces. Type selectors, pseudo-classes and plain values stay plain; an
//! element before a pseudo-class reads as a property, a hex color as an id, and a
//! negative number as a plain word.

use super::{Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static CSS: Spec = Spec {
    line_markers: &[],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"-@"),
    words: &[
        (
            &[
                "@charset",
                "@container",
                "@counter-style",
                "@font-face",
                "@import",
                "@keyframes",
                "@layer",
                "@media",
                "@namespace",
                "@page",
                "@property",
                "@supports",
                "from",
                "to",
            ],
            SyntaxClass::Keyword,
        ),
        (&["and", "not", "only", "or"], SyntaxClass::Operator),
    ],
    before: &[("(", SyntaxClass::Function), (":", SyntaxClass::Property)],
    after: &[
        ("!", SyntaxClass::Keyword),
        ("#", SyntaxClass::Property),
        (".", SyntaxClass::Property),
    ],
    upper_case: None,
    capitalized: None,
    other_words: None,
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::CSS;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &CSS,
                ".card > a:hover, #main { font-size: calc(100% - 2rem) !important; color: #fff; } /* note */"
            ),
            [
                ("card", SyntaxClass::Property),
                (">", SyntaxClass::Operator),
                ("a", SyntaxClass::Property),
                ("main", SyntaxClass::Property),
                ("font-size", SyntaxClass::Property),
                ("calc", SyntaxClass::Function),
                ("100", SyntaxClass::Number),
                ("%", SyntaxClass::Operator),
                ("2rem", SyntaxClass::Number),
                ("!", SyntaxClass::Operator),
                ("important", SyntaxClass::Keyword),
                ("color", SyntaxClass::Property),
                ("fff", SyntaxClass::Property),
                ("/* note */", SyntaxClass::Comment),
            ]
        );
    }
}
