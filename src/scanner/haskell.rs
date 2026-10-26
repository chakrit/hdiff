//! Haskell's lexical pieces. Function application by juxtaposition, operators that start
//! with `--` such as `-->`, and quasi-quotes are not recognized.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static HASKELL: Spec = Spec {
    line_markers: &[Marker::anywhere("--", SyntaxClass::Comment)],
    delimited: &[
        Pair::nesting("{-", "-}", SyntaxClass::Comment),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::character("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"'"),
    words: &[
        (
            &[
                "case",
                "class",
                "data",
                "default",
                "deriving",
                "do",
                "else",
                "forall",
                "foreign",
                "hiding",
                "if",
                "import",
                "in",
                "infix",
                "infixl",
                "infixr",
                "instance",
                "let",
                "module",
                "newtype",
                "of",
                "qualified",
                "then",
                "type",
                "where",
            ],
            SyntaxClass::Keyword,
        ),
        (&["False", "True"], SyntaxClass::Constant),
    ],
    before: &[("::", SyntaxClass::Function)],
    after: &[],
    upper_case: Some(SyntaxClass::Type),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::HASKELL;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &HASKELL,
                r#"go n' = let x = 0x1F in case n' of 0 -> Just 'a'; _ -> Nothing {- {- n -} -} where f = "s" -- end"#
            ),
            [
                ("go", SyntaxClass::Variable),
                ("n'", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("let", SyntaxClass::Keyword),
                ("x", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("0x1F", SyntaxClass::Number),
                ("in", SyntaxClass::Keyword),
                ("case", SyntaxClass::Keyword),
                ("n'", SyntaxClass::Variable),
                ("of", SyntaxClass::Keyword),
                ("0", SyntaxClass::Number),
                ("->", SyntaxClass::Operator),
                ("Just", SyntaxClass::Type),
                ("'a'", SyntaxClass::String),
                ("_", SyntaxClass::Variable),
                ("->", SyntaxClass::Operator),
                ("Nothing", SyntaxClass::Type),
                ("{- {- n -} -}", SyntaxClass::Comment),
                ("where", SyntaxClass::Keyword),
                ("f", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                (r#""s""#, SyntaxClass::String),
                ("-- end", SyntaxClass::Comment),
            ]
        );
    }
}
