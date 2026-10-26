//! Erlang's lexical pieces. Lowercase words are atoms and capitalized words are variables.
//! Character literals such as `$a` and variables that start with `_` and a letter are not
//! recognized; a variable before `:`, as in `Class:Reason`, reads as a module.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static ERLANG: Spec = Spec {
    line_markers: &[Marker::anywhere("%", SyntaxClass::Comment)],
    delimited: &[
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::Constant),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "after",
                "begin",
                "behaviour",
                "callback",
                "case",
                "catch",
                "cond",
                "define",
                "else",
                "end",
                "endif",
                "export",
                "export_type",
                "fun",
                "if",
                "ifdef",
                "ifndef",
                "include",
                "include_lib",
                "let",
                "maybe",
                "module",
                "of",
                "opaque",
                "receive",
                "record",
                "spec",
                "try",
                "when",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "and", "andalso", "band", "bnot", "bor", "bsl", "bsr", "bxor", "div", "not", "or",
                "orelse", "rem", "xor",
            ],
            SyntaxClass::Operator,
        ),
        (&["_"], SyntaxClass::Variable),
    ],
    before: &[
        ("(", SyntaxClass::Function),
        ("::", SyntaxClass::Variable),
        (":=", SyntaxClass::Constant),
        (":", SyntaxClass::Type),
    ],
    after: &[
        ("fun", SyntaxClass::Function),
        ("?", SyntaxClass::Constant),
        ("#", SyntaxClass::Type),
    ],
    upper_case: Some(SyntaxClass::Variable),
    capitalized: Some(SyntaxClass::Variable),
    other_words: Some(SyntaxClass::Constant),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::ERLANG;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &ERLANG,
                "handle(Req, #state{count = N} = S) when N > 0 -> {reply, lists:reverse(Req), ?MODULE}. % reply"
            ),
            [
                ("handle", SyntaxClass::Function),
                ("Req", SyntaxClass::Variable),
                ("state", SyntaxClass::Type),
                ("count", SyntaxClass::Constant),
                ("=", SyntaxClass::Operator),
                ("N", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("S", SyntaxClass::Variable),
                ("when", SyntaxClass::Keyword),
                ("N", SyntaxClass::Variable),
                (">", SyntaxClass::Operator),
                ("0", SyntaxClass::Number),
                ("->", SyntaxClass::Operator),
                ("reply", SyntaxClass::Constant),
                ("lists", SyntaxClass::Type),
                ("reverse", SyntaxClass::Function),
                ("Req", SyntaxClass::Variable),
                ("?", SyntaxClass::Operator),
                ("MODULE", SyntaxClass::Constant),
                ("% reply", SyntaxClass::Comment),
            ]
        );
    }
}
