//! F#'s lexical pieces. Function application by juxtaposition, generic parameters such as
//! `'T`, and interpolated string holes are not recognized.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static FSHARP: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::nesting("(*", "*)", SyntaxClass::Comment),
        Pair::spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::spanning("@\"", "\"", SyntaxClass::String),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::character("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"'"),
    words: &[
        (
            &[
                "abstract",
                "and",
                "as",
                "assert",
                "base",
                "begin",
                "class",
                "default",
                "delegate",
                "do",
                "done",
                "downcast",
                "downto",
                "elif",
                "else",
                "end",
                "exception",
                "extern",
                "for",
                "fun",
                "function",
                "global",
                "if",
                "in",
                "inherit",
                "inline",
                "interface",
                "internal",
                "lazy",
                "let",
                "match",
                "member",
                "module",
                "mutable",
                "namespace",
                "new",
                "of",
                "open",
                "override",
                "private",
                "public",
                "rec",
                "return",
                "static",
                "struct",
                "then",
                "to",
                "try",
                "type",
                "upcast",
                "use",
                "val",
                "when",
                "while",
                "with",
                "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "bool",
                "byte",
                "char",
                "decimal",
                "double",
                "float",
                "float32",
                "int",
                "int16",
                "int32",
                "int64",
                "int8",
                "nativeint",
                "obj",
                "sbyte",
                "single",
                "string",
                "uint",
                "uint16",
                "uint32",
                "uint64",
                "uint8",
                "unativeint",
                "unit",
            ],
            SyntaxClass::Type,
        ),
        (&["false", "null", "true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[("type", SyntaxClass::Type), (".", SyntaxClass::Property)],
    upper_case: Some(SyntaxClass::Type),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::FSHARP;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &FSHARP,
                r#"let inline area (r: float) = Math.Sqrt(r) * 2.0 |> printfn "%f" // area"#
            ),
            [
                ("let", SyntaxClass::Keyword),
                ("inline", SyntaxClass::Keyword),
                ("area", SyntaxClass::Function),
                ("r", SyntaxClass::Variable),
                ("float", SyntaxClass::Type),
                ("=", SyntaxClass::Operator),
                ("Math", SyntaxClass::Type),
                ("Sqrt", SyntaxClass::Function),
                ("r", SyntaxClass::Variable),
                ("*", SyntaxClass::Operator),
                ("2.0", SyntaxClass::Number),
                ("|>", SyntaxClass::Operator),
                ("printfn", SyntaxClass::Variable),
                (r#""%f""#, SyntaxClass::String),
                ("// area", SyntaxClass::Comment),
            ]
        );
    }
}
