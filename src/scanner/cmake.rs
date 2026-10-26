//! CMake's lexical pieces. Generator expressions, `$ENV{...}` and `$CACHE{...}`, and
//! references inside quoted arguments are not recognized.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static CMAKE: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("#[[", "]]", SyntaxClass::Comment),
        Pair::spanning("#[=[", "]=]", SyntaxClass::Comment),
        Pair::spanning("[[", "]]", SyntaxClass::String),
        Pair::spanning("[=[", "]=]", SyntaxClass::String),
        Pair::nesting("${", "}", SyntaxClass::Variable),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "block",
                "break",
                "continue",
                "else",
                "elseif",
                "endblock",
                "endforeach",
                "endfunction",
                "endif",
                "endmacro",
                "endwhile",
                "foreach",
                "function",
                "if",
                "macro",
                "return",
                "while",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "and",
                "defined",
                "equal",
                "exists",
                "greater",
                "greater_equal",
                "in_list",
                "is_directory",
                "less",
                "less_equal",
                "matches",
                "not",
                "or",
                "strequal",
                "strgreater",
                "strless",
                "version_equal",
                "version_greater",
                "version_less",
            ],
            SyntaxClass::Operator,
        ),
        (&["false", "off", "on", "true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: None,
    other_words: None,
    word_suffixes: b"",
    word_case: WordCase::Insensitive,
};

#[cfg(test)]
mod tests {
    use super::CMAKE;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &CMAKE,
                r#"target_link_libraries(app PUBLIC ${DEPS} "lib.a") # link"#
            ),
            [
                ("target_link_libraries", SyntaxClass::Function),
                ("PUBLIC", SyntaxClass::Constant),
                ("${DEPS}", SyntaxClass::Variable),
                (r#""lib.a""#, SyntaxClass::String),
                ("# link", SyntaxClass::Comment),
            ]
        );
    }
}
