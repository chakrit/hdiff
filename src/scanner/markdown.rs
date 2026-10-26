//! Markdown's lexical pieces. Single `*` and `_` emphasis, setext headings, indented
//! code, link labels, and fence bodies' own languages are not recognized; numbers and
//! operator punctuation in running text keep their code classes.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static MARKDOWN: Spec = Spec {
    line_markers: &[Marker::at_line_start("#", SyntaxClass::Keyword)],
    delimited: &[
        Pair::spanning("```", "```", SyntaxClass::String),
        Pair::spanning("~~~", "~~~", SyntaxClass::String),
        Pair::spanning("``", "``", SyntaxClass::String),
        Pair::spanning("`", "`", SyntaxClass::String),
        Pair::escaped("**", SyntaxClass::Variable),
        Pair::escaped("__", SyntaxClass::Variable),
        Pair::escaped_spanning("](", ")", SyntaxClass::Property),
        Pair::spanning("<!--", "-->", SyntaxClass::Comment),
    ],
    word_bytes: WordBytes::with(b"-"),
    words: &[(&["-", "---"], SyntaxClass::Operator)],
    before: &[],
    after: &[],
    upper_case: None,
    capitalized: None,
    other_words: None,
    word_suffixes: b"!?:",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::MARKDOWN;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &MARKDOWN,
                "- See **the guide** and `cargo test`, or [docs](https://x.dev/a). It's well-known!"
            ),
            [
                ("-", SyntaxClass::Operator),
                ("**the guide**", SyntaxClass::Variable),
                ("`cargo test`", SyntaxClass::String),
                ("](https://x.dev/a)", SyntaxClass::Property),
            ]
        );
    }
}
