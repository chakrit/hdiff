//! OCaml's lexical pieces. Function application by juxtaposition, type variables such as
//! `'a`, and quoted strings with an identifier such as `{id|...|id}` are not recognized; a
//! word after a labelled argument's `:` reads as a type.

use super::{Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static OCAML: Spec = Spec {
    line_markers: &[],
    delimited: &[
        Pair::nesting("(*", "*)", SyntaxClass::Comment),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::spanning("{|", "|}", SyntaxClass::String),
        Pair::character("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"'"),
    words: &[
        (
            &[
                "and",
                "as",
                "assert",
                "begin",
                "class",
                "constraint",
                "do",
                "done",
                "downto",
                "else",
                "end",
                "exception",
                "external",
                "for",
                "fun",
                "function",
                "functor",
                "if",
                "in",
                "include",
                "inherit",
                "initializer",
                "lazy",
                "let",
                "match",
                "method",
                "module",
                "mutable",
                "new",
                "nonrec",
                "object",
                "of",
                "open",
                "private",
                "rec",
                "sig",
                "struct",
                "then",
                "to",
                "try",
                "type",
                "val",
                "virtual",
                "when",
                "while",
                "with",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &["asr", "land", "lor", "lsl", "lsr", "lxor", "mod", "or"],
            SyntaxClass::Operator,
        ),
        (
            &[
                "bool",
                "bytes",
                "char",
                "exn",
                "float",
                "int",
                "int32",
                "int64",
                "nativeint",
                "string",
                "unit",
            ],
            SyntaxClass::Type,
        ),
        (&["false", "true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("external", SyntaxClass::Function),
        ("type", SyntaxClass::Type),
        ("val", SyntaxClass::Function),
        (".", SyntaxClass::Property),
        (":", SyntaxClass::Type),
    ],
    upper_case: Some(SyntaxClass::Type),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::OCAML;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &OCAML,
                "let rec fact (n : int) = match n with 0 -> 1 | n -> n * fact (n - 1) (* (* nested *) *)"
            ),
            [
                ("let", SyntaxClass::Keyword),
                ("rec", SyntaxClass::Keyword),
                ("fact", SyntaxClass::Function),
                ("n", SyntaxClass::Variable),
                ("int", SyntaxClass::Type),
                ("=", SyntaxClass::Operator),
                ("match", SyntaxClass::Keyword),
                ("n", SyntaxClass::Variable),
                ("with", SyntaxClass::Keyword),
                ("0", SyntaxClass::Number),
                ("->", SyntaxClass::Operator),
                ("1", SyntaxClass::Number),
                ("|", SyntaxClass::Operator),
                ("n", SyntaxClass::Variable),
                ("->", SyntaxClass::Operator),
                ("n", SyntaxClass::Variable),
                ("*", SyntaxClass::Operator),
                ("fact", SyntaxClass::Function),
                ("n", SyntaxClass::Variable),
                ("-", SyntaxClass::Operator),
                ("1", SyntaxClass::Number),
                ("(* (* nested *) *)", SyntaxClass::Comment),
            ]
        );
    }
}
