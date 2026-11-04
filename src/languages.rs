//! The languages hdiff highlights, from the data `languages/generate.lua` writes from
//! Neovim and nvim-treesitter: the file type a path selects, and the Tree-sitter grammars
//! compiled in. A file type names both the scanner's language and the compiled grammar.
//! Rules: docs/spec/architecture.md#technology-direction.

mod file_types;
pub mod grammars;

use file_types::{EXTENSIONS, FILE_NAMES, PATTERNS_AFTER_EXTENSIONS, PATTERNS_BEFORE_EXTENSIONS};
use tree_sitter::Language;

use crate::{
    document::{DiffFile, FileBody},
    headers::{FileHeader, HeaderPath},
    side::BySide,
};

/// A Tree-sitter grammar compiled into hdiff: the file types it highlights, its language,
/// and its highlight query translated for the `tree-sitter` crate.
pub struct Grammar {
    pub name: &'static str,
    pub file_types: &'static [&'static str],
    pub language: fn() -> Language,
    pub highlights: &'static str,
}

/// A literal a file name matches at its start, at its end, or anywhere.
#[derive(Debug, Clone, Copy)]
pub enum NamePattern {
    Prefix(&'static str),
    Suffix(&'static str),
    Contains(&'static str),
}

impl NamePattern {
    fn matches(self, file_name: &str) -> bool {
        match self {
            Self::Prefix(literal) => file_name.starts_with(literal),
            Self::Suffix(literal) => file_name.ends_with(literal),
            Self::Contains(literal) => file_name.contains(literal),
        }
    }
}

/// The file type `path` selects, in Neovim's order: its whole file name, then the name
/// patterns tried before extensions, its extension, and the remaining name patterns.
pub fn file_type(path: &str) -> Option<&'static str> {
    let file_name = path.rsplit_once('/').map_or(path, |(_, name)| name);
    let by_name = look_up(FILE_NAMES, file_name);
    let by_early_pattern = || first_match(PATTERNS_BEFORE_EXTENSIONS, file_name);
    let by_extension = || {
        let (_, extension) = file_name.rsplit_once('.')?;
        look_up(EXTENSIONS, extension)
    };
    let by_late_pattern = || first_match(PATTERNS_AFTER_EXTENSIONS, file_name);

    by_name
        .or_else(by_early_pattern)
        .or_else(by_extension)
        .or_else(by_late_pattern)
}

/// The file type each side of `file` selects by its path; a side without a path, and a
/// file without headers, selects none.
pub fn of_file(file: &DiffFile) -> BySide<Option<&'static str>> {
    match &file.body {
        FileBody::Unified(body) => of_headers(body.headers.as_ref()),
        FileBody::Broken(body) => match &*body.read.headers {
            [old, new] => of_headers(BySide { old, new }),
            _ => BySide::default(),
        },
        FileBody::Metadata(_) => BySide::default(),
    }
}

/// The file type each side's header path selects.
pub fn of_headers(headers: BySide<&FileHeader>) -> BySide<Option<&'static str>> {
    headers.map(|header| match &header.path {
        HeaderPath::Path(path) => file_type(path),
        HeaderPath::DevNull | HeaderPath::Undecodable => None,
    })
}

fn look_up(table: &[(&str, &'static str)], key: &str) -> Option<&'static str> {
    let index = table.binary_search_by_key(&key, |(entry, _)| entry).ok()?;
    Some(table[index].1)
}

fn first_match(patterns: &[(NamePattern, &'static str)], file_name: &str) -> Option<&'static str> {
    patterns
        .iter()
        .find(|(pattern, _)| pattern.matches(file_name))
        .map(|(_, file_type)| *file_type)
}

#[cfg(test)]
mod tests {
    use tree_sitter::Query;

    use super::{EXTENSIONS, FILE_NAMES, file_type, grammars::GRAMMARS};

    #[test]
    fn selects_by_file_name_pattern_and_extension_in_neovims_order() {
        assert_eq!(file_type("cmd/main.go"), Some("go"));
        assert_eq!(file_type("CMakeLists.txt"), Some("cmake"));
        assert_eq!(file_type("build/GNUmakefile"), Some("make"));
        assert_eq!(file_type("Dockerfile.dev"), Some("dockerfile"));
        assert_eq!(file_type("include/list.h"), Some("c"));
        assert_eq!(file_type("README"), Some("text"));
        assert_eq!(file_type("notes.hdiff-unknown"), None);
    }

    #[test]
    fn every_highlight_query_compiles_against_its_grammar() {
        for grammar in GRAMMARS {
            let compiled = Query::new(&(grammar.language)(), grammar.highlights);
            assert!(
                compiled.is_ok(),
                "{} query: {:?}",
                grammar.name,
                compiled.err()
            );
        }
    }

    #[test]
    fn name_and_extension_tables_are_sorted_for_lookup() {
        for table in [FILE_NAMES, EXTENSIONS] {
            assert!(table.is_sorted_by_key(|(key, _)| *key));
        }
    }
}
