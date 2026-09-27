//! Which file a diff section names and how it changed, decoded from Git metadata and
//! `---` / `+++` headers. Contract: docs/spec/architecture.md#file-tree.

use std::str::CharIndices;

use crate::document::SourceLine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileState {
    Modified,
    Added,
    Deleted,
    Renamed,
}

/// A file located by its path, or a section whose path cannot be determined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileIdentity {
    Path { state: FileState, path: String },
    Unknown { header: String },
}

impl FileIdentity {
    /// A section with `---` / `+++` headers; `metadata` includes its `diff --git` line.
    pub fn of_unified(
        metadata: &[SourceLine],
        old_path: &HeaderPath,
        new_path: &HeaderPath,
        first_line: &SourceLine,
    ) -> Self {
        let located =
            git_change(metadata).and_then(|change| locate_unified(change, old_path, new_path));

        Self::located_or_unknown(located, first_line)
    }

    /// A Git section without `---` / `+++` headers.
    pub fn of_metadata(header: &SourceLine, lines: &[SourceLine]) -> Self {
        let located = git_change(lines).and_then(|change| locate_metadata(change, header));

        Self::located_or_unknown(located, header)
    }

    fn located_or_unknown(located: Option<(FileState, String)>, first_line: &SourceLine) -> Self {
        match located {
            Some((state, path)) => Self::Path { state, path },
            None => Self::Unknown {
                header: first_line.structural_text.clone(),
            },
        }
    }
}

/// The path a `---` / `+++` header names, decoded, with its comparison-root prefix.
#[derive(Debug, PartialEq, Eq)]
pub enum HeaderPath {
    DevNull,
    Path(String),
    Undecodable,
}

impl HeaderPath {
    /// Reads a header's field after `--- ` or `+++ `, without its timestamp.
    pub fn of_field(field: &str) -> Self {
        let decoded = match field.starts_with('"') {
            true => unquote(field).map(|(path, _)| path),
            false => Some(
                field
                    .split_once('\t')
                    .map_or(field, |(path, _)| path)
                    .to_owned(),
            ),
        };

        let Some(path) = decoded else {
            return Self::Undecodable;
        };

        match path == "/dev/null" {
            true => Self::DevNull,
            false => Self::Path(path),
        }
    }
}

// Locating a file

/// Git's own record of a change, the last such metadata line winning.
enum GitChange {
    Unmarked,
    Added,
    Deleted,
    Renamed(String),
    Copied(String),
}

fn locate_unified(
    change: GitChange,
    old_path: &HeaderPath,
    new_path: &HeaderPath,
) -> Option<(FileState, String)> {
    let (header_state, named_path) = match (old_path, new_path) {
        (HeaderPath::Undecodable, _) | (_, HeaderPath::Undecodable) => return None,
        (HeaderPath::DevNull, HeaderPath::DevNull) => return None,
        (HeaderPath::DevNull, HeaderPath::Path(new)) => (FileState::Added, strip(new, "b/")),
        (HeaderPath::Path(old), HeaderPath::DevNull) => (FileState::Deleted, strip(old, "a/")),
        (HeaderPath::Path(_), HeaderPath::Path(new)) => (FileState::Modified, strip(new, "b/")),
    };

    let located = match change {
        GitChange::Unmarked => (header_state, named_path.to_owned()),
        GitChange::Added => (FileState::Added, named_path.to_owned()),
        GitChange::Deleted => (FileState::Deleted, named_path.to_owned()),
        GitChange::Renamed(destination) => (FileState::Renamed, destination),
        GitChange::Copied(destination) => (FileState::Added, destination),
    };
    Some(located)
}

fn locate_metadata(change: GitChange, header: &SourceLine) -> Option<(FileState, String)> {
    let located = match change {
        GitChange::Unmarked => (FileState::Modified, git_header_path(header)?),
        GitChange::Added => (FileState::Added, git_header_path(header)?),
        GitChange::Deleted => (FileState::Deleted, git_header_path(header)?),
        GitChange::Renamed(destination) => (FileState::Renamed, destination),
        GitChange::Copied(destination) => (FileState::Added, destination),
    };

    Some(located)
}

/// `None` when a destination's quoting cannot be decoded.
fn git_change(lines: &[SourceLine]) -> Option<GitChange> {
    lines
        .iter()
        .map(|line| line.structural_text.as_str())
        .try_fold(GitChange::Unmarked, |change, line| {
            if line.starts_with("new file mode ") {
                return Some(GitChange::Added);
            }
            if line.starts_with("deleted file mode ") {
                return Some(GitChange::Deleted);
            }
            if let Some(path) = line.strip_prefix("rename to ") {
                return quoted_or_plain(path).map(GitChange::Renamed);
            }
            if let Some(path) = line.strip_prefix("copy to ") {
                return quoted_or_plain(path).map(GitChange::Copied);
            }

            Some(change)
        })
}

/// The path of `diff --git a/<path> b/<path>` when both halves name the same path.
fn git_header_path(line: &SourceLine) -> Option<String> {
    let names = line.structural_text.strip_prefix("diff --git ")?;
    let (old, new) = match names.starts_with('"') {
        true => {
            let (old, rest) = unquote(names)?;
            (old, quoted_or_plain(rest.strip_prefix(' ')?)?)
        }
        false => symmetric_halves(names)?,
    };

    let old = strip(&old, "a/");
    let new = strip(&new, "b/");
    (old == new).then(|| new.to_owned())
}

/// Splits unquoted `<old> <new>` at its middle space, as Git does when both names are
/// the same length and may contain spaces.
fn symmetric_halves(names: &str) -> Option<(String, String)> {
    let middle = names.len() / 2;
    let has_middle_space = names.len() % 2 == 1 && names.get(middle..=middle) == Some(" ");
    if !has_middle_space {
        return None;
    }

    Some((names[..middle].to_owned(), names[middle + 1..].to_owned()))
}

fn strip<'a>(path: &'a str, prefix: &str) -> &'a str {
    path.strip_prefix(prefix).unwrap_or(path)
}

// Git's C-style quoting

fn quoted_or_plain(field: &str) -> Option<String> {
    match field.starts_with('"') {
        true => unquote(field).map(|(path, _)| path),
        false => Some(field.to_owned()),
    }
}

/// Decodes Git's C-style quoting at the start of `field`, returning the decoded text
/// (bytes as lossy UTF-8) and what follows the closing quote.
fn unquote(field: &str) -> Option<(String, &str)> {
    let body = field.strip_prefix('"')?;
    let mut bytes = Vec::new();
    let mut characters = body.char_indices();

    while let Some((index, character)) = characters.next() {
        match character {
            '"' => {
                let decoded = String::from_utf8_lossy(&bytes).into_owned();
                return Some((decoded, &body[index + 1..]));
            }
            '\\' => bytes.push(escaped_byte(&mut characters)?),
            _ => bytes.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }

    None
}

fn escaped_byte(characters: &mut CharIndices) -> Option<u8> {
    let (_, escape) = characters.next()?;

    match escape {
        'a' => Some(0x07),
        'b' => Some(0x08),
        't' => Some(b'\t'),
        'n' => Some(b'\n'),
        'v' => Some(0x0b),
        'f' => Some(0x0c),
        'r' => Some(b'\r'),
        '"' => Some(b'"'),
        '\\' => Some(b'\\'),
        '0'..='3' => {
            let high = octal_digit(escape)?;
            let middle = octal_digit(characters.next()?.1)?;
            let low = octal_digit(characters.next()?.1)?;
            Some(high << 6 | middle << 3 | low)
        }
        _ => None,
    }
}

fn octal_digit(character: char) -> Option<u8> {
    character
        .to_digit(8)
        .and_then(|digit| u8::try_from(digit).ok())
}

#[cfg(test)]
mod tests {
    use super::{FileIdentity, FileState, HeaderPath};
    use crate::parser::parse_unified_diff;

    fn identity(input: &str) -> FileIdentity {
        let document = parse_unified_diff(input.as_bytes()).expect("valid diff");
        let file = document.files().next().expect("one file");

        file.identity().clone()
    }

    fn path(state: FileState, path: &str) -> FileIdentity {
        FileIdentity::Path {
            state,
            path: path.to_owned(),
        }
    }

    fn unknown(header: &str) -> FileIdentity {
        FileIdentity::Unknown {
            header: header.to_owned(),
        }
    }

    #[test]
    fn reads_each_git_state() {
        let cases = [
            (
                "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n",
                path(FileState::Modified, "f"),
            ),
            (
                "diff --git a/f b/f\nold mode 100644\nnew mode 100755\n",
                path(FileState::Modified, "f"),
            ),
            (
                "diff --git a/f b/f\nnew file mode 100644\n--- /dev/null\n+++ b/f\n@@ -0,0 +1 @@\n+a\n",
                path(FileState::Added, "f"),
            ),
            (
                "diff --git a/f b/f\ndeleted file mode 100644\n--- a/f\n+++ /dev/null\n@@ -1 +0,0 @@\n-a\n",
                path(FileState::Deleted, "f"),
            ),
            (
                "diff --git a/old b/dir/new\nsimilarity index 100%\nrename from old\nrename to dir/new\n",
                path(FileState::Renamed, "dir/new"),
            ),
            (
                "diff --git a/src b/copy\nsimilarity index 90%\ncopy from src\ncopy to copy\n--- a/src\n+++ b/copy\n@@ -1 +1 @@\n-a\n+b\n",
                path(FileState::Added, "copy"),
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(identity(input), expected, "{input}");
        }
    }

    #[test]
    fn reads_dev_null_headers_without_git_metadata() {
        assert_eq!(
            identity("--- /dev/null\n+++ b/added\n@@ -0,0 +1 @@\n+a\n"),
            path(FileState::Added, "added")
        );
        assert_eq!(
            identity("--- a/gone\n+++ /dev/null\n@@ -1 +0,0 @@\n-a\n"),
            path(FileState::Deleted, "gone")
        );
    }

    #[test]
    fn plain_diff_names_the_new_path_without_a_rename() {
        assert_eq!(
            identity(
                "--- tmp/x1\t2026-09-27 23:45:10\n+++ tmp/x2\t2026-09-27 23:45:11\n@@ -1 +1 @@\n-a\n+b\n"
            ),
            path(FileState::Modified, "tmp/x2")
        );
    }

    #[test]
    fn header_paths_drop_timestamps_and_keep_their_prefix() {
        assert_eq!(
            HeaderPath::of_field("b/x.rs\t2026-09-27 23:45:10"),
            HeaderPath::Path("b/x.rs".to_owned())
        );
        assert_eq!(HeaderPath::of_field("/dev/null"), HeaderPath::DevNull);
        assert_eq!(HeaderPath::of_field("\"b/open"), HeaderPath::Undecodable);
    }

    #[test]
    fn decodes_git_quoted_paths() {
        assert_eq!(
            identity(
                "--- \"a/\\340\\270\\201 \\\"q\\\".txt\"\n+++ \"b/\\340\\270\\201 \\\"q\\\".txt\"\n@@ -1 +1 @@\n-a\n+b\n"
            ),
            path(FileState::Modified, "ก \"q\".txt")
        );
        assert_eq!(
            identity(
                "diff --git \"a/tab\\there\" \"b/tab\\there\"\nold mode 100644\nnew mode 100755\n"
            ),
            path(FileState::Modified, "tab\there")
        );
    }

    #[test]
    fn reads_metadata_only_paths_with_spaces_and_without_prefixes() {
        assert_eq!(
            identity("diff --git a/with space b/with space\nold mode 100644\nnew mode 100755\n"),
            path(FileState::Modified, "with space")
        );
        assert_eq!(
            identity("diff --git x x\nold mode 100644\nnew mode 100755\n"),
            path(FileState::Modified, "x")
        );
    }

    #[test]
    fn marks_ambiguous_and_undecodable_sections_unknown() {
        assert_eq!(
            identity("diff --git a/x b/y z\nold mode 100644\nnew mode 100755\n"),
            unknown("diff --git a/x b/y z")
        );
        assert_eq!(
            identity("--- \"a/broken\\q\"\n+++ \"b/broken\\q\"\n@@ -1 +1 @@\n-a\n+b\n"),
            unknown("--- \"a/broken\\q\"")
        );
    }
}
