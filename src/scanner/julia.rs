//! Julia's lexical pieces. Symbols such as `:name`, string interpolation, and prefixed
//! string literals such as `r"..."` are not recognized.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static JULIA: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::nesting("#=", "=#", SyntaxClass::Comment),
        Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::spanning("```", "```", SyntaxClass::String),
        Pair::spanning("`", "`", SyntaxClass::String),
        Pair::character("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "abstract",
                "baremodule",
                "begin",
                "break",
                "catch",
                "const",
                "continue",
                "do",
                "else",
                "elseif",
                "end",
                "export",
                "finally",
                "for",
                "function",
                "global",
                "if",
                "import",
                "let",
                "local",
                "macro",
                "module",
                "mutable",
                "primitive",
                "public",
                "quote",
                "return",
                "struct",
                "try",
                "using",
                "where",
                "while",
            ],
            SyntaxClass::Keyword,
        ),
        (&["in", "isa"], SyntaxClass::Operator),
        (
            &["Inf", "NaN", "false", "missing", "nothing", "true"],
            SyntaxClass::Constant,
        ),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("@", SyntaxClass::Function),
        ("function", SyntaxClass::Function),
        ("macro", SyntaxClass::Function),
        (".", SyntaxClass::Property),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"!",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::JULIA;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &JULIA,
                "function push_all!(v::Vector{Int}, n) @inbounds for i in 1:n push!(v, i * 0x1F) end; return nothing end #= a #= b =# =#"
            ),
            [
                ("function", SyntaxClass::Keyword),
                ("push_all!", SyntaxClass::Function),
                ("v", SyntaxClass::Variable),
                ("::", SyntaxClass::Operator),
                ("Vector", SyntaxClass::Type),
                ("Int", SyntaxClass::Type),
                ("n", SyntaxClass::Variable),
                ("inbounds", SyntaxClass::Function),
                ("for", SyntaxClass::Keyword),
                ("i", SyntaxClass::Variable),
                ("in", SyntaxClass::Operator),
                ("1", SyntaxClass::Number),
                ("n", SyntaxClass::Variable),
                ("push!", SyntaxClass::Function),
                ("v", SyntaxClass::Variable),
                ("i", SyntaxClass::Variable),
                ("*", SyntaxClass::Operator),
                ("0x1F", SyntaxClass::Number),
                ("end", SyntaxClass::Keyword),
                ("return", SyntaxClass::Keyword),
                ("nothing", SyntaxClass::Constant),
                ("end", SyntaxClass::Keyword),
                ("#= a #= b =# =#", SyntaxClass::Comment),
            ]
        );
    }
}
