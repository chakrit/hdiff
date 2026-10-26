//! Elixir's lexical pieces. Atoms such as `:ok`, character literals such as `?a`, string
//! interpolation, and sigils other than `~r`, `~s` and `~w` with common delimiters are not
//! recognized; a keyword-list key such as `name:` reads as a constant.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static ELIXIR: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::escaped_spanning("'''", "'''", SyntaxClass::String),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::escaped_spanning("'", "'", SyntaxClass::String),
        Pair::escaped_spanning("~r/", "/", SyntaxClass::String),
        Pair::escaped_spanning("~r{", "}", SyntaxClass::String),
        Pair::escaped_spanning("~s(", ")", SyntaxClass::String),
        Pair::escaped_spanning("~w(", ")", SyntaxClass::String),
        Pair::escaped_spanning("~w[", "]", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "after",
                "alias",
                "case",
                "catch",
                "cond",
                "def",
                "defdelegate",
                "defexception",
                "defguard",
                "defguardp",
                "defimpl",
                "defmacro",
                "defmacrop",
                "defmodule",
                "defoverridable",
                "defp",
                "defprotocol",
                "defstruct",
                "do",
                "else",
                "end",
                "fn",
                "for",
                "if",
                "import",
                "quote",
                "raise",
                "receive",
                "require",
                "reraise",
                "rescue",
                "try",
                "unless",
                "unquote",
                "use",
                "with",
            ],
            SyntaxClass::Keyword,
        ),
        (&["and", "in", "not", "or", "when"], SyntaxClass::Operator),
        (
            &[
                "__CALLER__",
                "__DIR__",
                "__ENV__",
                "__MODULE__",
                "__STACKTRACE__",
                "false",
                "nil",
                "true",
            ],
            SyntaxClass::Constant,
        ),
    ],
    before: &[("(", SyntaxClass::Function), (": ", SyntaxClass::Constant)],
    after: &[
        ("def", SyntaxClass::Function),
        ("defguard", SyntaxClass::Function),
        ("defguardp", SyntaxClass::Function),
        ("defmacro", SyntaxClass::Function),
        ("defmacrop", SyntaxClass::Function),
        ("defp", SyntaxClass::Function),
        ("@", SyntaxClass::Constant),
        (".", SyntaxClass::Property),
    ],
    upper_case: Some(SyntaxClass::Type),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"?!",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::ELIXIR;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &ELIXIR,
                "defp valid?(%{name: name} = user), do: String.length(name) > 0x1F and user.active # check"
            ),
            [
                ("defp", SyntaxClass::Keyword),
                ("valid?", SyntaxClass::Function),
                ("%", SyntaxClass::Operator),
                ("name", SyntaxClass::Constant),
                ("name", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("user", SyntaxClass::Variable),
                ("do", SyntaxClass::Keyword),
                ("String", SyntaxClass::Type),
                ("length", SyntaxClass::Function),
                ("name", SyntaxClass::Variable),
                (">", SyntaxClass::Operator),
                ("0x1F", SyntaxClass::Number),
                ("and", SyntaxClass::Operator),
                ("user", SyntaxClass::Variable),
                ("active", SyntaxClass::Property),
                ("# check", SyntaxClass::Comment),
            ]
        );
    }
}
