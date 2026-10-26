//! Lua's lexical pieces. Long brackets of more than one `=` level are not recognized, and
//! a word after the `..` operator reads as a property.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static LUA: Spec = Spec {
    line_markers: &[Marker::anywhere("--", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("--[[", "]]", SyntaxClass::Comment),
        Pair::spanning("--[=[", "]=]", SyntaxClass::Comment),
        Pair::spanning("[[", "]]", SyntaxClass::String),
        Pair::spanning("[=[", "]=]", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "break", "do", "else", "elseif", "end", "for", "function", "goto", "if", "in",
                "local", "repeat", "return", "then", "until", "while",
            ],
            SyntaxClass::Keyword,
        ),
        (&["and", "not", "or"], SyntaxClass::Operator),
        (&["false", "nil", "true"], SyntaxClass::Constant),
    ],
    before: &[
        ("(", SyntaxClass::Function),
        ("\"", SyntaxClass::Function),
        ("'", SyntaxClass::Function),
        ("{", SyntaxClass::Function),
    ],
    after: &[
        ("function", SyntaxClass::Function),
        (".", SyntaxClass::Property),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: None,
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::LUA;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &LUA,
                r#"local function greet(name, n) if n ~= 0x1F then return string.format("hi %s", name or DEFAULT) end end -- done"#
            ),
            [
                ("local", SyntaxClass::Keyword),
                ("function", SyntaxClass::Keyword),
                ("greet", SyntaxClass::Function),
                ("name", SyntaxClass::Variable),
                ("n", SyntaxClass::Variable),
                ("if", SyntaxClass::Keyword),
                ("n", SyntaxClass::Variable),
                ("~=", SyntaxClass::Operator),
                ("0x1F", SyntaxClass::Number),
                ("then", SyntaxClass::Keyword),
                ("return", SyntaxClass::Keyword),
                ("string", SyntaxClass::Variable),
                ("format", SyntaxClass::Function),
                (r#""hi %s""#, SyntaxClass::String),
                ("name", SyntaxClass::Variable),
                ("or", SyntaxClass::Operator),
                ("DEFAULT", SyntaxClass::Constant),
                ("end", SyntaxClass::Keyword),
                ("end", SyntaxClass::Keyword),
                ("-- done", SyntaxClass::Comment),
            ]
        );
    }
}
