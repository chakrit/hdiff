//! C's lexical pieces. A preprocessor directive is the word after `#`.

use super::{Marker, Pair, Spec, WordBytes};
use crate::syntax::SyntaxClass;

pub static C: Spec = Spec {
    line_markers: &[Marker {
        text: "//",
        class: SyntaxClass::Comment,
    }],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::character("'", SyntaxClass::Number),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "break", "case", "const", "continue", "default", "do", "else", "enum", "extern",
                "for", "goto", "if", "inline", "return", "sizeof", "static", "struct", "switch",
                "typedef", "union", "volatile", "while",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "bool",
                "char",
                "double",
                "float",
                "int",
                "int16_t",
                "int32_t",
                "int64_t",
                "int8_t",
                "intptr_t",
                "long",
                "ptrdiff_t",
                "short",
                "signed",
                "size_t",
                "ssize_t",
                "uint16_t",
                "uint32_t",
                "uint64_t",
                "uint8_t",
                "uintptr_t",
                "unsigned",
                "void",
            ],
            SyntaxClass::Type,
        ),
        (&["NULL", "false", "true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("#", SyntaxClass::Keyword),
        (".", SyntaxClass::Property),
        ("->", SyntaxClass::Property),
        ("enum", SyntaxClass::Type),
        ("struct", SyntaxClass::Type),
        ("union", SyntaxClass::Type),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: None,
    other_words: Some(SyntaxClass::Variable),
};
