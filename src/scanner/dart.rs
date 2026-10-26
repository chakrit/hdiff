//! Dart's lexical pieces. String interpolation reads as string text; annotations read as
//! variables.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static DART: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::nesting("/*", "*/", SyntaxClass::Comment),
        Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::escaped_spanning("'''", "'''", SyntaxClass::String),
        Pair::spanning("r\"", "\"", SyntaxClass::String),
        Pair::spanning("r'", "'", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"$"),
    words: &[
        (
            &[
                "abstract",
                "as",
                "assert",
                "async",
                "await",
                "base",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "covariant",
                "default",
                "deferred",
                "do",
                "else",
                "enum",
                "export",
                "extends",
                "extension",
                "external",
                "factory",
                "final",
                "finally",
                "for",
                "if",
                "implements",
                "import",
                "in",
                "interface",
                "is",
                "late",
                "library",
                "mixin",
                "new",
                "operator",
                "part",
                "required",
                "rethrow",
                "return",
                "sealed",
                "static",
                "super",
                "switch",
                "sync",
                "throw",
                "try",
                "typedef",
                "var",
                "while",
                "with",
                "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &["bool", "double", "dynamic", "int", "num", "void"],
            SyntaxClass::Type,
        ),
        (&["false", "null", "true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[(".", SyntaxClass::Property)],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::DART;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &DART,
                "final items = await repo.fetch(limit: 10); if (items.isEmpty) return const <String>[]; // none"
            ),
            [
                ("final", SyntaxClass::Keyword),
                ("items", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("await", SyntaxClass::Keyword),
                ("repo", SyntaxClass::Variable),
                ("fetch", SyntaxClass::Function),
                ("limit", SyntaxClass::Variable),
                ("10", SyntaxClass::Number),
                ("if", SyntaxClass::Keyword),
                ("items", SyntaxClass::Variable),
                ("isEmpty", SyntaxClass::Property),
                ("return", SyntaxClass::Keyword),
                ("const", SyntaxClass::Keyword),
                ("<", SyntaxClass::Operator),
                ("String", SyntaxClass::Type),
                (">", SyntaxClass::Operator),
                ("// none", SyntaxClass::Comment),
            ]
        );
    }
}
