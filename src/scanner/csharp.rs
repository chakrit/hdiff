//! C#'s lexical pieces. A preprocessor directive is the word after `#`; interpolation
//! holes read as string text.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static CSHARP: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::spanning("@\"", "\"", SyntaxClass::String),
        Pair::spanning("$@\"", "\"", SyntaxClass::String),
        Pair::spanning("@$\"", "\"", SyntaxClass::String),
        Pair::escaped_spanning("$\"", "\"", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::character("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "abstract",
                "as",
                "async",
                "await",
                "base",
                "break",
                "case",
                "catch",
                "checked",
                "class",
                "const",
                "continue",
                "default",
                "delegate",
                "do",
                "else",
                "enum",
                "event",
                "explicit",
                "extern",
                "finally",
                "fixed",
                "for",
                "foreach",
                "goto",
                "if",
                "implicit",
                "in",
                "interface",
                "internal",
                "is",
                "lock",
                "namespace",
                "new",
                "operator",
                "out",
                "override",
                "params",
                "private",
                "protected",
                "public",
                "readonly",
                "ref",
                "return",
                "sealed",
                "sizeof",
                "stackalloc",
                "static",
                "struct",
                "switch",
                "throw",
                "try",
                "typeof",
                "unchecked",
                "unsafe",
                "using",
                "var",
                "virtual",
                "volatile",
                "while",
                "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "bool", "byte", "char", "decimal", "double", "dynamic", "float", "int", "long",
                "nint", "nuint", "object", "sbyte", "short", "string", "uint", "ulong", "ushort",
                "void",
            ],
            SyntaxClass::Type,
        ),
        (&["false", "null", "true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[("#", SyntaxClass::Keyword), (".", SyntaxClass::Property)],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::CSHARP;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &CSHARP,
                r#"public async Task<int> SaveAsync(string path) => await File.WriteAllTextAsync(path, $"id={Id}"); // io"#
            ),
            [
                ("public", SyntaxClass::Keyword),
                ("async", SyntaxClass::Keyword),
                ("Task", SyntaxClass::Type),
                ("<", SyntaxClass::Operator),
                ("int", SyntaxClass::Type),
                (">", SyntaxClass::Operator),
                ("SaveAsync", SyntaxClass::Function),
                ("string", SyntaxClass::Type),
                ("path", SyntaxClass::Variable),
                ("=>", SyntaxClass::Operator),
                ("await", SyntaxClass::Keyword),
                ("File", SyntaxClass::Type),
                ("WriteAllTextAsync", SyntaxClass::Function),
                ("path", SyntaxClass::Variable),
                (r#"$"id={Id}""#, SyntaxClass::String),
                ("// io", SyntaxClass::Comment),
            ]
        );
    }
}
