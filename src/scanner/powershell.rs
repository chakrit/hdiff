//! PowerShell's lexical pieces. Keywords are case-insensitive; `$` and `-` are part of a
//! word, so `$name`, `Get-Item` and `-eq` are each one word. Backtick escapes, string
//! interpolation, and negative numbers, which read as words, are not recognized; a
//! variable used as an index, as in `$list[$i]`, reads as a type.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static POWERSHELL: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("<#", "#>", SyntaxClass::Comment),
        Pair::spanning("@\"", "\"@", SyntaxClass::String),
        Pair::spanning("@'", "'@", SyntaxClass::String),
        Pair::spanning("\"", "\"", SyntaxClass::String),
        Pair::spanning("'", "'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"$-"),
    words: &[
        (
            &[
                "begin",
                "break",
                "catch",
                "class",
                "continue",
                "data",
                "default",
                "do",
                "dynamicparam",
                "else",
                "elseif",
                "end",
                "enum",
                "exit",
                "filter",
                "finally",
                "for",
                "foreach",
                "function",
                "if",
                "in",
                "param",
                "process",
                "return",
                "switch",
                "throw",
                "trap",
                "try",
                "until",
                "using",
                "while",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "-",
                "--",
                "-and",
                "-as",
                "-band",
                "-bnot",
                "-bor",
                "-bxor",
                "-ceq",
                "-cne",
                "-contains",
                "-eq",
                "-f",
                "-ge",
                "-gt",
                "-ilike",
                "-in",
                "-is",
                "-isnot",
                "-join",
                "-le",
                "-like",
                "-lt",
                "-match",
                "-ne",
                "-not",
                "-notcontains",
                "-notin",
                "-notlike",
                "-notmatch",
                "-or",
                "-replace",
                "-shl",
                "-shr",
                "-split",
                "-xor",
            ],
            SyntaxClass::Operator,
        ),
        (&["$false", "$null", "$true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("filter", SyntaxClass::Function),
        ("function", SyntaxClass::Function),
        (".", SyntaxClass::Property),
        ("[", SyntaxClass::Type),
    ],
    upper_case: None,
    capitalized: Some(SyntaxClass::Function),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Insensitive,
};

#[cfg(test)]
mod tests {
    use super::POWERSHELL;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &POWERSHELL,
                r#"function Get-Total($Items) { foreach ($i in $Items) { if ($i.Count -gt 0x1F) { Write-Host "big" } }; return $true } # sum"#
            ),
            [
                ("function", SyntaxClass::Keyword),
                ("Get-Total", SyntaxClass::Function),
                ("$Items", SyntaxClass::Variable),
                ("foreach", SyntaxClass::Keyword),
                ("$i", SyntaxClass::Variable),
                ("in", SyntaxClass::Keyword),
                ("$Items", SyntaxClass::Variable),
                ("if", SyntaxClass::Keyword),
                ("$i", SyntaxClass::Variable),
                ("Count", SyntaxClass::Property),
                ("-gt", SyntaxClass::Operator),
                ("0x1F", SyntaxClass::Number),
                ("Write-Host", SyntaxClass::Function),
                (r#""big""#, SyntaxClass::String),
                ("return", SyntaxClass::Keyword),
                ("$true", SyntaxClass::Constant),
                ("# sum", SyntaxClass::Comment),
            ]
        );
    }
}
