//! Kotlin's lexical pieces. Annotations read as types; string templates read as string
//! text.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static KOTLIN: Spec = Spec {
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
                "annotation",
                "as",
                "break",
                "by",
                "catch",
                "class",
                "companion",
                "constructor",
                "continue",
                "crossinline",
                "do",
                "else",
                "enum",
                "external",
                "final",
                "finally",
                "for",
                "fun",
                "if",
                "import",
                "in",
                "infix",
                "init",
                "inline",
                "inner",
                "interface",
                "internal",
                "is",
                "lateinit",
                "noinline",
                "object",
                "open",
                "operator",
                "override",
                "package",
                "private",
                "protected",
                "public",
                "reified",
                "return",
                "sealed",
                "super",
                "suspend",
                "tailrec",
                "throw",
                "try",
                "typealias",
                "typeof",
                "val",
                "var",
                "vararg",
                "when",
                "where",
                "while",
            ],
            SyntaxClass::Keyword,
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
    use super::KOTLIN;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &KOTLIN,
                "override fun render(items: List<Item>): String = items.joinToString { it.name } // ui"
            ),
            [
                ("override", SyntaxClass::Keyword),
                ("fun", SyntaxClass::Keyword),
                ("render", SyntaxClass::Function),
                ("items", SyntaxClass::Variable),
                ("List", SyntaxClass::Type),
                ("<", SyntaxClass::Operator),
                ("Item", SyntaxClass::Type),
                (">", SyntaxClass::Operator),
                ("String", SyntaxClass::Type),
                ("=", SyntaxClass::Operator),
                ("items", SyntaxClass::Variable),
                ("joinToString", SyntaxClass::Property),
                ("it", SyntaxClass::Variable),
                ("name", SyntaxClass::Property),
                ("// ui", SyntaxClass::Comment),
            ]
        );
    }
}
