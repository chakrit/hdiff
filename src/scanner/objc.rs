//! Objective-C's lexical pieces. A preprocessor directive is the word after `#`, and an
//! `@` directive is the word after `@`; message selectors read as variables.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static OBJC: Spec = Spec {
    line_markers: &[Marker::anywhere("//", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped_spanning("@\"", "\"", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::character("'", SyntaxClass::Number),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "__autoreleasing",
                "__block",
                "__strong",
                "__unsafe_unretained",
                "__weak",
                "atomic",
                "break",
                "case",
                "const",
                "continue",
                "default",
                "do",
                "else",
                "enum",
                "extern",
                "for",
                "goto",
                "if",
                "in",
                "inline",
                "nonatomic",
                "readonly",
                "readwrite",
                "return",
                "sizeof",
                "static",
                "struct",
                "switch",
                "typedef",
                "union",
                "volatile",
                "while",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "BOOL",
                "Class",
                "IMP",
                "SEL",
                "bool",
                "char",
                "double",
                "float",
                "id",
                "instancetype",
                "int",
                "int16_t",
                "int32_t",
                "int64_t",
                "int8_t",
                "intptr_t",
                "long",
                "ptrdiff_t",
                "short",
                "signed",
                "size_t",
                "ssize_t",
                "uint16_t",
                "uint32_t",
                "uint64_t",
                "uint8_t",
                "uintptr_t",
                "unsigned",
                "void",
            ],
            SyntaxClass::Type,
        ),
        (
            &["NO", "NULL", "Nil", "YES", "false", "nil", "true"],
            SyntaxClass::Constant,
        ),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("@", SyntaxClass::Keyword),
        ("#", SyntaxClass::Keyword),
        (".", SyntaxClass::Property),
        ("->", SyntaxClass::Property),
        ("enum", SyntaxClass::Type),
        ("struct", SyntaxClass::Type),
        ("union", SyntaxClass::Type),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::OBJC;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &OBJC,
                r#"NSString *title = @"Hello"; if (self.count > 0 && flag == YES) { [self reload]; } // ui"#
            ),
            [
                ("NSString", SyntaxClass::Type),
                ("*", SyntaxClass::Operator),
                ("title", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                (r#"@"Hello""#, SyntaxClass::String),
                ("if", SyntaxClass::Keyword),
                ("self", SyntaxClass::Variable),
                ("count", SyntaxClass::Property),
                (">", SyntaxClass::Operator),
                ("0", SyntaxClass::Number),
                ("&&", SyntaxClass::Operator),
                ("flag", SyntaxClass::Variable),
                ("==", SyntaxClass::Operator),
                ("YES", SyntaxClass::Constant),
                ("self", SyntaxClass::Variable),
                ("reload", SyntaxClass::Variable),
                ("// ui", SyntaxClass::Comment),
            ]
        );
    }
}
