//! Shell's lexical pieces. Heredocs and `$'...'` strings are not recognized, and a `#`
//! inside a word such as `$#` opens a comment.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static SHELL: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("${", "}", SyntaxClass::Variable),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::spanning("'", "'", SyntaxClass::String),
        Pair::escaped_spanning("`", "`", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"-./"),
    words: &[
        (
            &[
                "case", "declare", "do", "done", "elif", "else", "esac", "export", "fi", "for",
                "function", "if", "in", "local", "readonly", "select", "then", "typeset", "unset",
                "until", "while",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &["-eq", "-ge", "-gt", "-le", "-lt", "-ne"],
            SyntaxClass::Operator,
        ),
        (
            &[
                "alias", "break", "builtin", "cd", "command", "continue", "echo", "eval", "exec",
                "exit", "false", "getopts", "hash", "printf", "pwd", "read", "return", "set",
                "shift", "source", "test", "trap", "true", "type", "ulimit", "umask", "unalias",
                "wait",
            ],
            SyntaxClass::Function,
        ),
    ],
    before: &[("(", SyntaxClass::Function), ("=", SyntaxClass::Variable)],
    after: &[("$", SyntaxClass::Variable)],
    upper_case: None,
    capitalized: None,
    other_words: None,
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::SHELL;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &SHELL,
                r#"if [ $count -gt 0 ]; then echo "found $count" >&2; exit 1; fi # done"#
            ),
            [
                ("if", SyntaxClass::Keyword),
                ("count", SyntaxClass::Variable),
                ("-gt", SyntaxClass::Operator),
                ("0", SyntaxClass::Number),
                ("then", SyntaxClass::Keyword),
                ("echo", SyntaxClass::Function),
                (r#""found $count""#, SyntaxClass::String),
                (">&", SyntaxClass::Operator),
                ("2", SyntaxClass::Number),
                ("exit", SyntaxClass::Function),
                ("1", SyntaxClass::Number),
                ("fi", SyntaxClass::Keyword),
                ("# done", SyntaxClass::Comment),
            ]
        );
    }
}
