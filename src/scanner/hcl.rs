//! HCL's lexical pieces, read for Terraform. Heredocs and template interpolation inside
//! strings are not recognized, and a bare name before `==` reads as an attribute.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static HCL: Spec = Spec {
    line_markers: &[
        Marker::anywhere("#", SyntaxClass::Comment),
        Marker::anywhere("//", SyntaxClass::Comment),
    ],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped("\"", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "check",
                "data",
                "dynamic",
                "else",
                "endfor",
                "endif",
                "for",
                "if",
                "import",
                "in",
                "locals",
                "module",
                "moved",
                "output",
                "provider",
                "removed",
                "resource",
                "terraform",
                "variable",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "any", "bool", "list", "map", "number", "object", "set", "string", "tuple",
            ],
            SyntaxClass::Type,
        ),
        (&["false", "null", "true"], SyntaxClass::Constant),
    ],
    before: &[
        ("(", SyntaxClass::Function),
        ("=>", SyntaxClass::Variable),
        ("=", SyntaxClass::Property),
        ("{", SyntaxClass::Type),
    ],
    after: &[(".", SyntaxClass::Property)],
    upper_case: None,
    capitalized: None,
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::HCL;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &HCL,
                r#"tags = merge(var.tags, { Name = "web-${count.index}" }) # x"#
            ),
            [
                ("tags", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("merge", SyntaxClass::Function),
                ("var", SyntaxClass::Variable),
                ("tags", SyntaxClass::Property),
                ("Name", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                (r#""web-${count.index}""#, SyntaxClass::String),
                ("# x", SyntaxClass::Comment),
            ]
        );
    }
}
