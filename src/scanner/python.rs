//! Python's lexical pieces.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static PYTHON: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
        Pair::escaped_spanning("'''", "'''", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "as", "assert", "async", "await", "break", "case", "class", "continue", "def",
                "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
                "import", "lambda", "match", "nonlocal", "pass", "raise", "return", "try", "while",
                "with", "yield",
            ],
            SyntaxClass::Keyword,
        ),
        (&["and", "in", "is", "not", "or"], SyntaxClass::Operator),
        (&["False", "None", "True"], SyntaxClass::Constant),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("@", SyntaxClass::Function),
        ("def", SyntaxClass::Function),
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
    use super::PYTHON;
    use crate::scanner::{ScanState, scanned};
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_python() {
        assert_eq!(
            ground(
                &PYTHON,
                "@cache def load(self): return self.items if x is not None else MAX # done"
            ),
            [
                ("cache", SyntaxClass::Function),
                ("def", SyntaxClass::Keyword),
                ("load", SyntaxClass::Function),
                ("self", SyntaxClass::Variable),
                ("return", SyntaxClass::Keyword),
                ("self", SyntaxClass::Variable),
                ("items", SyntaxClass::Property),
                ("if", SyntaxClass::Keyword),
                ("x", SyntaxClass::Variable),
                ("is", SyntaxClass::Operator),
                ("not", SyntaxClass::Operator),
                ("None", SyntaxClass::Constant),
                ("else", SyntaxClass::Keyword),
                ("MAX", SyntaxClass::Constant),
                ("# done", SyntaxClass::Comment),
            ]
        );
        let (opening, inside) = scanned(&PYTHON, ScanState::default(), "doc = '''one");
        let (closing, after) = scanned(&PYTHON, inside, "two''' + 'x'");
        assert_eq!(
            opening,
            [
                ("doc", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("'''one", SyntaxClass::String),
            ]
        );
        assert_eq!(
            closing,
            [
                ("two'''", SyntaxClass::String),
                ("+", SyntaxClass::Operator),
                ("'x'", SyntaxClass::String),
            ]
        );
        assert_eq!(after, ScanState::default());
    }
}
