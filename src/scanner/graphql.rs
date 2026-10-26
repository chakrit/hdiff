//! GraphQL's lexical pieces. Arguments read as fields, and a type written in ALL_CAPS
//! other than `ID` reads as an enum value.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static GRAPHQL: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "directive",
                "enum",
                "extend",
                "fragment",
                "implements",
                "input",
                "interface",
                "mutation",
                "on",
                "query",
                "repeatable",
                "scalar",
                "schema",
                "subscription",
                "type",
                "union",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &["Boolean", "Float", "ID", "Int", "String"],
            SyntaxClass::Type,
        ),
        (&["false", "null", "true"], SyntaxClass::Constant),
    ],
    before: &[],
    after: &[
        ("$", SyntaxClass::Variable),
        (".", SyntaxClass::Function),
        ("@", SyntaxClass::Function),
        ("fragment", SyntaxClass::Function),
        ("mutation", SyntaxClass::Function),
        ("query", SyntaxClass::Function),
        ("subscription", SyntaxClass::Function),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Property),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::GRAPHQL;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &GRAPHQL,
                "query GetUser($id: ID!, $all: Boolean = false) { user(id: $id) @include(if: $all) { ...UserFields name } } # q"
            ),
            [
                ("query", SyntaxClass::Keyword),
                ("GetUser", SyntaxClass::Function),
                ("id", SyntaxClass::Variable),
                ("ID", SyntaxClass::Type),
                ("!", SyntaxClass::Operator),
                ("all", SyntaxClass::Variable),
                ("Boolean", SyntaxClass::Type),
                ("=", SyntaxClass::Operator),
                ("false", SyntaxClass::Constant),
                ("user", SyntaxClass::Property),
                ("id", SyntaxClass::Property),
                ("id", SyntaxClass::Variable),
                ("include", SyntaxClass::Function),
                ("if", SyntaxClass::Property),
                ("all", SyntaxClass::Variable),
                ("UserFields", SyntaxClass::Function),
                ("name", SyntaxClass::Property),
                ("# q", SyntaxClass::Comment),
            ]
        );
    }
}
