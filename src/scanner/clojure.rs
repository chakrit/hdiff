//! Clojure's lexical pieces. The word after `(` is the called function. Character
//! literals such as `\a`, the `#_` discard form, and negative numbers, which read as
//! words, are not recognized.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static CLOJURE: Spec = Spec {
    line_markers: &[Marker::anywhere(";", SyntaxClass::Comment)],
    delimited: &[Pair::escaped_spanning("\"", "\"", SyntaxClass::String)],
    word_bytes: WordBytes::with(b"-*+!?<>=/.&%$"),
    words: &[
        (
            &[
                "case",
                "catch",
                "cond",
                "condp",
                "def",
                "defmacro",
                "defmethod",
                "defmulti",
                "defn",
                "defn-",
                "defonce",
                "defprotocol",
                "defrecord",
                "deftype",
                "do",
                "doseq",
                "dotimes",
                "finally",
                "fn",
                "for",
                "if",
                "if-let",
                "if-not",
                "let",
                "letfn",
                "loop",
                "new",
                "ns",
                "quote",
                "recur",
                "set!",
                "throw",
                "try",
                "var",
                "when",
                "when-let",
                "when-not",
                "while",
            ],
            SyntaxClass::Keyword,
        ),
        (&["and", "or"], SyntaxClass::Operator),
        (&["false", "nil", "true"], SyntaxClass::Constant),
    ],
    before: &[],
    after: &[
        ("(", SyntaxClass::Function),
        (":", SyntaxClass::Constant),
        ("::", SyntaxClass::Constant),
        ("defmacro", SyntaxClass::Function),
        ("defmulti", SyntaxClass::Function),
        ("defn", SyntaxClass::Function),
        ("defn-", SyntaxClass::Function),
    ],
    upper_case: None,
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::CLOJURE;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &CLOJURE,
                r#"(defn greet [name] (str "Hi, " name (when-not (empty? name) :ok) 0x1F)) ; greet"#
            ),
            [
                ("defn", SyntaxClass::Keyword),
                ("greet", SyntaxClass::Function),
                ("name", SyntaxClass::Variable),
                ("str", SyntaxClass::Function),
                (r#""Hi, ""#, SyntaxClass::String),
                ("name", SyntaxClass::Variable),
                ("when-not", SyntaxClass::Keyword),
                ("empty?", SyntaxClass::Function),
                ("name", SyntaxClass::Variable),
                ("ok", SyntaxClass::Constant),
                ("0x1F", SyntaxClass::Number),
                ("; greet", SyntaxClass::Comment),
            ]
        );
    }
}
