//! Scala's lexical pieces. String interpolation reads as string text, and the
//! interpolator before a string reads as a word.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static SCALA: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::nesting("/*", "*/", SyntaxClass::Comment),
        Pair::spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::character("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "abstract",
                "case",
                "catch",
                "class",
                "def",
                "do",
                "else",
                "enum",
                "export",
                "extends",
                "final",
                "finally",
                "for",
                "forSome",
                "given",
                "if",
                "implicit",
                "import",
                "lazy",
                "match",
                "new",
                "object",
                "override",
                "package",
                "private",
                "protected",
                "return",
                "sealed",
                "then",
                "throw",
                "trait",
                "try",
                "type",
                "using",
                "val",
                "var",
                "while",
                "with",
                "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (&["false", "null", "true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[("def", SyntaxClass::Function), (".", SyntaxClass::Property)],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::SCALA;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &SCALA,
                "override def total(xs: List[Int]): Int = xs.foldLeft(0)(_ + _) // sum"
            ),
            [
                ("override", SyntaxClass::Keyword),
                ("def", SyntaxClass::Keyword),
                ("total", SyntaxClass::Function),
                ("xs", SyntaxClass::Variable),
                ("List", SyntaxClass::Type),
                ("Int", SyntaxClass::Type),
                ("Int", SyntaxClass::Type),
                ("=", SyntaxClass::Operator),
                ("xs", SyntaxClass::Variable),
                ("foldLeft", SyntaxClass::Function),
                ("0", SyntaxClass::Number),
                ("_", SyntaxClass::Variable),
                ("+", SyntaxClass::Operator),
                ("_", SyntaxClass::Variable),
                ("// sum", SyntaxClass::Comment),
            ]
        );
    }
}
