//! Makefile's lexical pieces. Automatic variables such as `$@`, nested references, and
//! the shell syntax of recipes are not recognized; a recipe word before `=` or `:` reads
//! as a variable or a target.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static MAKEFILE: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"-./"),
    words: &[
        (
            &[
                "-include", "define", "else", "endef", "endif", "export", "ifdef", "ifeq",
                "ifndef", "ifneq", "include", "override", "private", "sinclude", "undefine",
                "unexport", "vpath",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "abspath",
                "addprefix",
                "addsuffix",
                "basename",
                "call",
                "error",
                "eval",
                "filter-out",
                "findstring",
                "firstword",
                "foreach",
                "info",
                "lastword",
                "notdir",
                "patsubst",
                "realpath",
                "shell",
                "subst",
                "warning",
                "wildcard",
            ],
            SyntaxClass::Function,
        ),
    ],
    before: &[
        ("::=", SyntaxClass::Variable),
        (":=", SyntaxClass::Variable),
        (":", SyntaxClass::Function),
        ("+=", SyntaxClass::Variable),
        ("?=", SyntaxClass::Variable),
        ("!=", SyntaxClass::Variable),
        ("=", SyntaxClass::Variable),
    ],
    after: &[("(", SyntaxClass::Variable), ("{", SyntaxClass::Variable)],
    upper_case: None,
    capitalized: None,
    other_words: None,
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::MAKEFILE;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &MAKEFILE,
                "CFLAGS += -O2 $(shell pkg-config --cflags gtk) # opts"
            ),
            [
                ("CFLAGS", SyntaxClass::Variable),
                ("+=", SyntaxClass::Operator),
                ("shell", SyntaxClass::Function),
                ("# opts", SyntaxClass::Comment),
            ]
        );
    }
}
