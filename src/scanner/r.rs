//! R's lexical pieces. Raw strings with brackets other than `(`, backtick-quoted names,
//! and a number written as `.5` are not recognized; `.` is part of a word.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static R: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("r\"(", ")\"", SyntaxClass::String),
        Pair::spanning("R\"(", ")\"", SyntaxClass::String),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::escaped_spanning("'", "'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"."),
    words: &[
        (
            &[
                "break", "else", "for", "function", "if", "in", "next", "repeat", "return", "while",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "FALSE",
                "Inf",
                "NA",
                "NA_character_",
                "NA_complex_",
                "NA_integer_",
                "NA_real_",
                "NULL",
                "NaN",
                "TRUE",
            ],
            SyntaxClass::Constant,
        ),
    ],
    before: &[
        ("(", SyntaxClass::Function),
        ("<- function", SyntaxClass::Function),
        ("<- \\(", SyntaxClass::Function),
        ("= function", SyntaxClass::Function),
    ],
    after: &[("$", SyntaxClass::Property), ("@", SyntaxClass::Property)],
    upper_case: None,
    capitalized: None,
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::R;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &R,
                "clean_data <- function(df, n = 10L) { df$value[is.na(df$value)] <- NA_real_; if (n > 0) return(TRUE) } # fix"
            ),
            [
                ("clean_data", SyntaxClass::Function),
                ("<-", SyntaxClass::Operator),
                ("function", SyntaxClass::Keyword),
                ("df", SyntaxClass::Variable),
                ("n", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("10L", SyntaxClass::Number),
                ("df", SyntaxClass::Variable),
                ("value", SyntaxClass::Property),
                ("is.na", SyntaxClass::Function),
                ("df", SyntaxClass::Variable),
                ("value", SyntaxClass::Property),
                ("<-", SyntaxClass::Operator),
                ("NA_real_", SyntaxClass::Constant),
                ("if", SyntaxClass::Keyword),
                ("n", SyntaxClass::Variable),
                (">", SyntaxClass::Operator),
                ("0", SyntaxClass::Number),
                ("return", SyntaxClass::Keyword),
                ("TRUE", SyntaxClass::Constant),
                ("# fix", SyntaxClass::Comment),
            ]
        );
    }
}
