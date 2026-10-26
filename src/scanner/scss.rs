//! SCSS's lexical pieces. A variable reads as a property where it is declared, before
//! `:`; type selectors, pseudo-classes, placeholders, interpolation and plain values stay
//! plain; a `//` inside an unquoted `url(...)` starts a comment.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static SCSS: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"-@"),
    words: &[
        (
            &[
                "@at-root",
                "@charset",
                "@content",
                "@debug",
                "@each",
                "@else",
                "@error",
                "@extend",
                "@font-face",
                "@for",
                "@forward",
                "@function",
                "@if",
                "@import",
                "@include",
                "@keyframes",
                "@media",
                "@mixin",
                "@namespace",
                "@page",
                "@return",
                "@supports",
                "@use",
                "@warn",
                "@while",
                "from",
                "in",
                "through",
                "to",
            ],
            SyntaxClass::Keyword,
        ),
        (&["false", "null", "true"], SyntaxClass::Constant),
        (&["and", "not", "only", "or"], SyntaxClass::Operator),
    ],
    before: &[("(", SyntaxClass::Function), (":", SyntaxClass::Property)],
    after: &[
        ("!", SyntaxClass::Keyword),
        ("#", SyntaxClass::Property),
        ("$", SyntaxClass::Variable),
        (".", SyntaxClass::Property),
        ("@include", SyntaxClass::Function),
    ],
    upper_case: None,
    capitalized: None,
    other_words: None,
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::SCSS;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &SCSS,
                "@include mq($bp) { &:hover { color: darken($brand, 10%); } } // hover"
            ),
            [
                ("@include", SyntaxClass::Keyword),
                ("mq", SyntaxClass::Function),
                ("bp", SyntaxClass::Variable),
                ("&:", SyntaxClass::Operator),
                ("color", SyntaxClass::Property),
                ("darken", SyntaxClass::Function),
                ("brand", SyntaxClass::Variable),
                ("10", SyntaxClass::Number),
                ("%", SyntaxClass::Operator),
                ("// hover", SyntaxClass::Comment),
            ]
        );
    }
}
