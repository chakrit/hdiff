//! Groovy's lexical pieces. Slashy and dollar-slashy strings are not recognized, and
//! interpolation reads as string text.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static GROOVY: Spec = Spec {
    line_markers: &[
        Marker::anywhere("//", SyntaxClass::Comment),
        Marker::at_line_start("#!", SyntaxClass::Comment),
    ],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::escaped_spanning("'''", "'''", SyntaxClass::String),
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
                "break",
                "case",
                "catch",
                "class",
                "continue",
                "def",
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
                "in",
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
                "trait",
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
    use super::GROOVY;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &GROOVY,
                r#"def deps = ['core', 'web'].collect { "org.app:$it:${VERSION}" } // modules"#
            ),
            [
                ("def", SyntaxClass::Keyword),
                ("deps", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("'core'", SyntaxClass::String),
                ("'web'", SyntaxClass::String),
                ("collect", SyntaxClass::Property),
                (r#""org.app:$it:${VERSION}""#, SyntaxClass::String),
                ("// modules", SyntaxClass::Comment),
            ]
        );
    }
}
