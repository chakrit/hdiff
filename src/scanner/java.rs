//! Java's lexical pieces. Annotations read as types.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static JAVA: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::character("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"$"),
    words: &[
        (
            &[
                "abstract",
                "assert",
                "break",
                "case",
                "catch",
                "class",
                "continue",
                "default",
                "do",
                "else",
                "enum",
                "extends",
                "final",
                "finally",
                "for",
                "if",
                "implements",
                "import",
                "instanceof",
                "interface",
                "native",
                "new",
                "package",
                "private",
                "protected",
                "public",
                "return",
                "static",
                "strictfp",
                "super",
                "switch",
                "synchronized",
                "throw",
                "throws",
                "transient",
                "try",
                "volatile",
                "while",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "boolean", "byte", "char", "double", "float", "int", "long", "short", "void",
            ],
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
    use super::JAVA;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &JAVA,
                r#"@Override public String label(List<Item> items) { return items.isEmpty() ? "none" : PREFIX + items.size(); } // short"#
            ),
            [
                ("Override", SyntaxClass::Type),
                ("public", SyntaxClass::Keyword),
                ("String", SyntaxClass::Type),
                ("label", SyntaxClass::Function),
                ("List", SyntaxClass::Type),
                ("<", SyntaxClass::Operator),
                ("Item", SyntaxClass::Type),
                (">", SyntaxClass::Operator),
                ("items", SyntaxClass::Variable),
                ("return", SyntaxClass::Keyword),
                ("items", SyntaxClass::Variable),
                ("isEmpty", SyntaxClass::Function),
                ("?", SyntaxClass::Operator),
                (r#""none""#, SyntaxClass::String),
                ("PREFIX", SyntaxClass::Constant),
                ("+", SyntaxClass::Operator),
                ("items", SyntaxClass::Variable),
                ("size", SyntaxClass::Function),
                ("// short", SyntaxClass::Comment),
            ]
        );
    }
}
