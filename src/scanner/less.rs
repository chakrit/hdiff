//! Less's lexical pieces. Variables stay plain, and read as properties where declared,
//! before `:`; type selectors, pseudo-classes, interpolation and plain values stay plain;
//! a `//` inside an unquoted `url(...)` starts a comment.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static LESS: Spec = Spec {
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
                "@charset",
                "@container",
                "@font-face",
                "@import",
                "@keyframes",
                "@layer",
                "@media",
                "@namespace",
                "@page",
                "@plugin",
                "@supports",
                "when",
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
    use super::LESS;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &LESS,
                r#".button { .rounded(4px); color: @primary; width: ~"calc(100% - 4px)"; } // btn"#
            ),
            [
                ("button", SyntaxClass::Property),
                ("rounded", SyntaxClass::Function),
                ("4px", SyntaxClass::Number),
                ("color", SyntaxClass::Property),
                ("width", SyntaxClass::Property),
                ("~", SyntaxClass::Operator),
                (r#""calc(100% - 4px)""#, SyntaxClass::String),
                ("// btn", SyntaxClass::Comment),
            ]
        );
    }
}
