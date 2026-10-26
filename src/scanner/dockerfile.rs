//! Dockerfile's lexical pieces. Lowercase instructions, heredocs, references inside
//! strings, and the shell syntax of `RUN` commands are not recognized.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static DOCKERFILE: Spec = Spec {
    line_markers: &[Marker::at_line_start("#", SyntaxClass::Comment)],
    delimited: &[
        Pair::spanning("${", "}", SyntaxClass::Variable),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[(
        &[
            "ADD",
            "ARG",
            "AS",
            "CMD",
            "COPY",
            "ENTRYPOINT",
            "ENV",
            "EXPOSE",
            "FROM",
            "HEALTHCHECK",
            "LABEL",
            "MAINTAINER",
            "ONBUILD",
            "RUN",
            "SHELL",
            "STOPSIGNAL",
            "USER",
            "VOLUME",
            "WORKDIR",
        ],
        SyntaxClass::Keyword,
    )],
    before: &[("=", SyntaxClass::Property)],
    after: &[("$", SyntaxClass::Variable)],
    upper_case: None,
    capitalized: None,
    other_words: None,
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::DOCKERFILE;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(&DOCKERFILE, "COPY --from=build /app/bin ${APP_HOME}/bin"),
            [
                ("COPY", SyntaxClass::Keyword),
                ("--", SyntaxClass::Operator),
                ("from", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("/", SyntaxClass::Operator),
                ("/", SyntaxClass::Operator),
                ("${APP_HOME}", SyntaxClass::Variable),
                ("/", SyntaxClass::Operator),
            ]
        );
    }
}
