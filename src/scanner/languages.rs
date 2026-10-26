//! The languages the scanner reads, each with the file names and extensions that select
//! it. Rules: docs/spec/architecture.md#technology-direction.

use super::{Spec, c::C, go::GO, javascript::JAVASCRIPT, python::PYTHON, rust::RUST};

/// A language the scanner reads, with what selects it: a whole file name, or else a file
/// name's extension.
struct Language {
    file_names: &'static [&'static str],
    extensions: &'static [&'static str],
    spec: &'static Spec,
}

/// Every language the scanner reads. An extension two languages share selects the one
/// listed first.
static LANGUAGES: &[Language] = &[
    Language {
        file_names: &[],
        extensions: &["c", "h"],
        spec: &C,
    },
    Language {
        file_names: &[],
        extensions: &["go"],
        spec: &GO,
    },
    Language {
        file_names: &[],
        extensions: &["js", "jsx"],
        spec: &JAVASCRIPT,
    },
    Language {
        file_names: &[],
        extensions: &["py"],
        spec: &PYTHON,
    },
    Language {
        file_names: &[],
        extensions: &["rs"],
        spec: &RUST,
    },
];

/// The spec of the language `path` selects: by its whole file name, or else by its
/// extension; none for any other path.
pub fn of_path(path: &str) -> Option<&'static Spec> {
    let file_name = path.rsplit_once('/').map_or(path, |(_, name)| name);
    let by_name = LANGUAGES
        .iter()
        .find(|language| language.file_names.contains(&file_name));
    let by_extension = || {
        let (_, extension) = file_name.rsplit_once('.')?;
        LANGUAGES
            .iter()
            .find(|language| language.extensions.contains(&extension))
    };

    by_name.or_else(by_extension).map(|language| language.spec)
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::{super::go::GO, of_path};

    #[test]
    fn selects_a_language_by_file_name_then_by_extension() {
        assert!(of_path("cmd/main.go").is_some_and(|spec| ptr::eq(spec, &GO)));
        assert!(of_path("README").is_none());
        assert!(of_path("archive.tar").is_none());
    }
}
