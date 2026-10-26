//! XML's lexical pieces. Entities, single-quoted attribute values, DTD internals, and a
//! tag, comment or CDATA section glued to punctuation an operator run swallows (such as
//! `?><root>` or `/><!--`) are not recognized.

use super::{Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static XML: Spec = Spec {
    line_markers: &[],
    delimited: &[
        Pair::spanning("<!--", "-->", SyntaxClass::Comment),
        Pair::spanning("<![CDATA[", "]]>", SyntaxClass::String),
        Pair::escaped("\"", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"-.:"),
    words: &[],
    before: &[("=", SyntaxClass::Property)],
    after: &[
        ("/></", SyntaxClass::Keyword),
        ("/><", SyntaxClass::Keyword),
        ("<", SyntaxClass::Keyword),
        ("<!", SyntaxClass::Keyword),
        ("</", SyntaxClass::Keyword),
        ("<?", SyntaxClass::Keyword),
        ("><", SyntaxClass::Keyword),
        ("></", SyntaxClass::Keyword),
    ],
    upper_case: None,
    capitalized: None,
    other_words: None,
    word_suffixes: b"!?",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::XML;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &XML,
                r#"<svg:rect xml:id="r1" width="10"/> <![CDATA[a < b]]> <!-- note -->"#
            ),
            [
                ("<", SyntaxClass::Operator),
                ("svg:rect", SyntaxClass::Keyword),
                ("xml:id", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                (r#""r1""#, SyntaxClass::String),
                ("width", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                (r#""10""#, SyntaxClass::String),
                ("/>", SyntaxClass::Operator),
                ("<![CDATA[a < b]]>", SyntaxClass::String),
                ("<!-- note -->", SyntaxClass::Comment),
            ]
        );
    }
}
