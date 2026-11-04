//! The file type a path selects, from the table `languages/generate.lua` writes from
//! Neovim's filetype detection. A file type names both the scanner's language and the
//! compiled Tree-sitter grammar. Rules: docs/spec/architecture.md#technology-direction.

mod file_types;

use file_types::{EXTENSIONS, FILE_NAMES, PATTERNS_AFTER_EXTENSIONS, PATTERNS_BEFORE_EXTENSIONS};

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
    use super::{EXTENSIONS, FILE_NAMES, file_type};

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
    fn name_and_extension_tables_are_sorted_for_lookup() {
        for table in [FILE_NAMES, EXTENSIONS] {
            assert!(table.is_sorted_by_key(|(key, _)| *key));
        }
    }
}
