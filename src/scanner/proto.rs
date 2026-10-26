//! Protocol Buffers' lexical pieces. Package and option paths read as fields, and an
//! enum value written in lower case reads as a field.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static PROTO: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "edition",
                "enum",
                "extend",
                "extensions",
                "group",
                "import",
                "map",
                "max",
                "message",
                "oneof",
                "option",
                "optional",
                "package",
                "public",
                "repeated",
                "required",
                "reserved",
                "returns",
                "rpc",
                "service",
                "stream",
                "syntax",
                "to",
                "weak",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "bool", "bytes", "double", "fixed32", "fixed64", "float", "int32", "int64",
                "sfixed32", "sfixed64", "sint32", "sint64", "string", "uint32", "uint64",
            ],
            SyntaxClass::Type,
        ),
        (&["false", "inf", "nan", "true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("enum", SyntaxClass::Type),
        ("message", SyntaxClass::Type),
        ("rpc", SyntaxClass::Function),
        ("service", SyntaxClass::Type),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Property),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::PROTO;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &PROTO,
                "repeated google.protobuf.Timestamp seen_at = 4 [deprecated = true]; // old"
            ),
            [
                ("repeated", SyntaxClass::Keyword),
                ("google", SyntaxClass::Property),
                ("protobuf", SyntaxClass::Property),
                ("Timestamp", SyntaxClass::Type),
                ("seen_at", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("4", SyntaxClass::Number),
                ("deprecated", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("true", SyntaxClass::Constant),
                ("// old", SyntaxClass::Comment),
            ]
        );
    }
}
