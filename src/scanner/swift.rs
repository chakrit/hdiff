//! Swift's lexical pieces. Extended raw strings with more than one `#` and regex literals
//! are not recognized; attributes read as types.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static SWIFT: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::nesting("/*", "*/", SyntaxClass::Comment),
        Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::spanning("#\"", "\"#", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"$"),
    words: &[
        (
            &[
                "as",
                "associatedtype",
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "continue",
                "convenience",
                "default",
                "defer",
                "deinit",
                "do",
                "dynamic",
                "else",
                "enum",
                "extension",
                "fallthrough",
                "fileprivate",
                "final",
                "for",
                "func",
                "guard",
                "if",
                "import",
                "in",
                "indirect",
                "init",
                "inout",
                "internal",
                "is",
                "lazy",
                "let",
                "mutating",
                "nonmutating",
                "open",
                "operator",
                "override",
                "precedencegroup",
                "private",
                "protocol",
                "public",
                "repeat",
                "rethrows",
                "return",
                "some",
                "static",
                "struct",
                "subscript",
                "super",
                "switch",
                "throw",
                "throws",
                "try",
                "typealias",
                "unowned",
                "var",
                "weak",
                "where",
                "while",
            ],
            SyntaxClass::Keyword,
        ),
        (&["false", "nil", "true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("func", SyntaxClass::Function),
        (".", SyntaxClass::Property),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::SWIFT;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &SWIFT,
                "func load<T>(_ id: Int) async throws -> T? { guard let user = cache[id] else { return nil }; return user.value }"
            ),
            [
                ("func", SyntaxClass::Keyword),
                ("load", SyntaxClass::Function),
                ("<", SyntaxClass::Operator),
                ("T", SyntaxClass::Type),
                (">", SyntaxClass::Operator),
                ("_", SyntaxClass::Variable),
                ("id", SyntaxClass::Variable),
                ("Int", SyntaxClass::Type),
                ("async", SyntaxClass::Keyword),
                ("throws", SyntaxClass::Keyword),
                ("->", SyntaxClass::Operator),
                ("T", SyntaxClass::Type),
                ("?", SyntaxClass::Operator),
                ("guard", SyntaxClass::Keyword),
                ("let", SyntaxClass::Keyword),
                ("user", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("cache", SyntaxClass::Variable),
                ("id", SyntaxClass::Variable),
                ("else", SyntaxClass::Keyword),
                ("return", SyntaxClass::Keyword),
                ("nil", SyntaxClass::Constant),
                ("return", SyntaxClass::Keyword),
                ("user", SyntaxClass::Variable),
                ("value", SyntaxClass::Property),
            ]
        );
    }
}
