//! Perl's lexical pieces. Regular expressions, here-documents, quote-like operators with
//! other delimiters, and POD that does not open with `=pod`, `=head` or `=begin` are not
//! recognized; `#` inside a word keeps `$#array` whole.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static PERL: Spec = Spec {
    line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("=pod", "=cut", SyntaxClass::Comment),
        Pair::spanning("=head", "=cut", SyntaxClass::Comment),
        Pair::spanning("=begin", "=cut", SyntaxClass::Comment),
        Pair::escaped_spanning("\"", "\"", SyntaxClass::String),
        Pair::escaped_spanning("'", "'", SyntaxClass::String),
        Pair::escaped_spanning("`", "`", SyntaxClass::String),
        Pair::spanning("q(", ")", SyntaxClass::String),
        Pair::spanning("q{", "}", SyntaxClass::String),
        Pair::spanning("qq(", ")", SyntaxClass::String),
        Pair::spanning("qq{", "}", SyntaxClass::String),
        Pair::spanning("qr/", "/", SyntaxClass::String),
        Pair::spanning("qw(", ")", SyntaxClass::String),
        Pair::spanning("qw/", "/", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"$@#"),
    words: &[
        (
            &[
                "BEGIN", "END", "continue", "do", "else", "elsif", "for", "foreach", "goto", "if",
                "last", "local", "my", "next", "no", "our", "package", "redo", "require", "return",
                "state", "sub", "unless", "until", "use", "while",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "and", "cmp", "eq", "ge", "gt", "le", "lt", "ne", "not", "or", "xor",
            ],
            SyntaxClass::Operator,
        ),
        (
            &["__FILE__", "__LINE__", "__PACKAGE__", "__SUB__", "undef"],
            SyntaxClass::Constant,
        ),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[
        ("sub", SyntaxClass::Function),
        ("->", SyntaxClass::Function),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::PERL;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &PERL,
                "sub total { my ($self, @items) = @_; return scalar(@items) if $self->{count} eq 'x' && $#items > 0x1F; } # sum"
            ),
            [
                ("sub", SyntaxClass::Keyword),
                ("total", SyntaxClass::Function),
                ("my", SyntaxClass::Keyword),
                ("$self", SyntaxClass::Variable),
                ("@items", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("@_", SyntaxClass::Variable),
                ("return", SyntaxClass::Keyword),
                ("scalar", SyntaxClass::Function),
                ("@items", SyntaxClass::Variable),
                ("if", SyntaxClass::Keyword),
                ("$self", SyntaxClass::Variable),
                ("->", SyntaxClass::Operator),
                ("count", SyntaxClass::Variable),
                ("eq", SyntaxClass::Operator),
                ("'x'", SyntaxClass::String),
                ("&&", SyntaxClass::Operator),
                ("$#items", SyntaxClass::Variable),
                (">", SyntaxClass::Operator),
                ("0x1F", SyntaxClass::Number),
                ("# sum", SyntaxClass::Comment),
            ]
        );
    }
}
