//! Python's lexical pieces.

use super::{Marker, Pair, Spec, WordBytes};
use crate::syntax::SyntaxClass;

pub static PYTHON: Spec = Spec {
    line_markers: &[Marker {
        text: "#",
        class: SyntaxClass::Comment,
    }],
    delimited: &[
        Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::escaped_spanning("'''", "'''", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "as", "assert", "async", "await", "break", "case", "class", "continue", "def",
                "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
                "import", "lambda", "match", "nonlocal", "pass", "raise", "return", "try", "while",
                "with", "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (&["and", "in", "is", "not", "or"], SyntaxClass::Operator),
        (&["False", "None", "True"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("@", SyntaxClass::Function),
        ("def", SyntaxClass::Function),
        (".", SyntaxClass::Property),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: None,
    other_words: Some(SyntaxClass::Variable),
};
