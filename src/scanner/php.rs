//! PHP's lexical pieces. Heredoc and nowdoc strings are not recognized, and a `#[`
//! attribute reads as a comment.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static PHP: Spec = Spec {
    line_markers: &[
        Marker::anywhere("//", SyntaxClass::Comment),
        Marker::anywhere("#", SyntaxClass::Comment),
    ],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::escaped_spanning("'", "'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"$"),
    words: &[
        (
            &[
                "abstract",
                "and",
                "as",
                "break",
                "case",
                "catch",
                "class",
                "clone",
                "const",
                "continue",
                "declare",
                "default",
                "do",
                "echo",
                "else",
                "elseif",
                "enddeclare",
                "endfor",
                "endforeach",
                "endif",
                "endswitch",
                "endwhile",
                "enum",
                "extends",
                "final",
                "finally",
                "fn",
                "for",
                "foreach",
                "function",
                "global",
                "goto",
                "if",
                "implements",
                "include",
                "include_once",
                "instanceof",
                "insteadof",
                "interface",
                "match",
                "namespace",
                "new",
                "or",
                "print",
                "private",
                "protected",
                "public",
                "readonly",
                "require",
                "require_once",
                "return",
                "static",
                "switch",
                "throw",
                "trait",
                "try",
                "use",
                "var",
                "while",
                "xor",
                "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "array", "bool", "callable", "float", "int", "iterable", "mixed", "never",
                "object", "string", "void",
            ],
            SyntaxClass::Type,
        ),
        (&["false", "null", "true"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("->", SyntaxClass::Property),
        ("?->", SyntaxClass::Property),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::PHP;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &PHP,
                "public function find(int $id): ?User { return $this->repo->get($id) ?? null; } // lookup"
            ),
            [
                ("public", SyntaxClass::Keyword),
                ("function", SyntaxClass::Keyword),
                ("find", SyntaxClass::Function),
                ("int", SyntaxClass::Type),
                ("$id", SyntaxClass::Variable),
                ("?", SyntaxClass::Operator),
                ("User", SyntaxClass::Type),
                ("return", SyntaxClass::Keyword),
                ("$this", SyntaxClass::Variable),
                ("->", SyntaxClass::Operator),
                ("repo", SyntaxClass::Property),
                ("->", SyntaxClass::Operator),
                ("get", SyntaxClass::Function),
                ("$id", SyntaxClass::Variable),
                ("??", SyntaxClass::Operator),
                ("null", SyntaxClass::Constant),
                ("// lookup", SyntaxClass::Comment),
            ]
        );
    }
}
