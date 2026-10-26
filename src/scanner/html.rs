//! HTML's lexical pieces, also read for Vue and Svelte. `<script>` and `<style>` bodies,
//! template interpolation, entities, single-quoted attribute values, and a tag or comment
//! glued to punctuation an operator run swallows (such as `%</td>` or `</div><!--`) are
//! not recognized.

use super::{Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static HTML: Spec = Spec {
    line_markers: &[],
    delimited: &[
        Pair::spanning("<!--", "-->", SyntaxClass::Comment),
        Pair::escaped("\"", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"-.:@"),
    words: &[],
    before: &[("=", SyntaxClass::Property)],
    after: &[
        ("/></", SyntaxClass::Keyword),
        ("/><", SyntaxClass::Keyword),
        ("<", SyntaxClass::Keyword),
        ("<!", SyntaxClass::Constant),
        ("</", SyntaxClass::Keyword),
        ("=", SyntaxClass::String),
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
    use super::HTML;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &HTML,
                r#"<input type="text" :value="name" disabled/> <a href="/docs">Read <em>more</em></a> <!-- nav -->"#
            ),
            [
                ("<", SyntaxClass::Operator),
                ("input", SyntaxClass::Keyword),
                ("type", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                (r#""text""#, SyntaxClass::String),
                (":value", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                (r#""name""#, SyntaxClass::String),
                ("/>", SyntaxClass::Operator),
                ("<", SyntaxClass::Operator),
                ("a", SyntaxClass::Keyword),
                ("href", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                (r#""/docs""#, SyntaxClass::String),
                (">", SyntaxClass::Operator),
                ("<", SyntaxClass::Operator),
                ("em", SyntaxClass::Keyword),
                (">", SyntaxClass::Operator),
                ("</", SyntaxClass::Operator),
                ("em", SyntaxClass::Keyword),
                ("></", SyntaxClass::Operator),
                ("a", SyntaxClass::Keyword),
                (">", SyntaxClass::Operator),
                ("<!-- nav -->", SyntaxClass::Comment),
            ]
        );
    }
}
