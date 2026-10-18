//! Which file a diff section names and how it changed, decoded from Git metadata and
//! `---` / `+++` headers. Contract: docs/spec/architecture.md#file-tree.

use std::str::CharIndices;

use crate::source_text::SourceLine;

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
    /// A section with `---` / `+++` headers; `metadata` is empty outside Git and otherwise
    /// starts with its `diff --git` line.
    pub fn of_unified(
        metadata: &[SourceLine],
        old_path: &HeaderPath,
        new_path: &HeaderPath,
        first_line: &SourceLine,
    ) -> Self {
        let git_header = metadata.first();
        let located = git_change(metadata)
            .and_then(|change| locate_unified(change, git_header, old_path, new_path));

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
                header: first_line.sanitized().to_owned(),
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
    git_header: Option<&SourceLine>,
    old_path: &HeaderPath,
    new_path: &HeaderPath,
) -> Option<(FileState, String)> {
    // A `/dev/null` side leaves one header path, so Git sections name the file from
    // their `diff --git` line; plain diffs show the path as written.
    let one_sided = |path: &String| match git_header {
        Some(header) => git_header_path(header),
        None => Some(path.clone()),
    };
    let (header_state, named_path) = match (old_path, new_path) {
        (HeaderPath::Undecodable, _) | (_, HeaderPath::Undecodable) => return None,
        (HeaderPath::DevNull, HeaderPath::DevNull) => return None,
        (HeaderPath::DevNull, HeaderPath::Path(new)) => (FileState::Added, one_sided(new)?),
        (HeaderPath::Path(old), HeaderPath::DevNull) => (FileState::Deleted, one_sided(old)?),
        (HeaderPath::Path(old), HeaderPath::Path(new)) => {
            (FileState::Modified, shared_name(old, new).to_owned())
        }
    };

    let located = match change {
        GitChange::Unmarked => (header_state, named_path),
        GitChange::Added => (FileState::Added, named_path),
        GitChange::Deleted => (FileState::Deleted, named_path),
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
        .map(|line| line.sanitized())
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

/// The name a `diff --git <old> <new>` line gives: the path suffix its halves share.
fn git_header_path(line: &SourceLine) -> Option<String> {
    let names = line.sanitized().strip_prefix("diff --git ")?;
    if !names.starts_with('"') {
        return unquoted_git_name(names);
    }

    let (old, rest) = unquote(names)?;
    let new = quoted_or_plain(rest.strip_prefix(' ')?)?;
    shared_suffix(&old, &new).map(str::to_owned)
}

/// Unquoted names may contain spaces; the halves split at the one space whose sides
/// share a path suffix.
fn unquoted_git_name(names: &str) -> Option<String> {
    let mut named = names
        .match_indices(' ')
        .filter_map(|(space, _)| shared_suffix(&names[..space], &names[space + 1..]));
    let name = named.next()?;

    named.next().is_none().then(|| name.to_owned())
}

// Naming a file from two paths

/// The shared path suffix of `old` and `new`, or all of `new` when they share none.
fn shared_name<'a>(old: &str, new: &'a str) -> &'a str {
    let shared = shared_suffix(old, new);
    shared.unwrap_or(new)
}

/// The longest run of trailing `/`-separated components both paths end with, as
/// written in `new`. Drops whatever comparison roots precede it, such as `a/` and `b/`.
fn shared_suffix<'a>(old: &str, new: &'a str) -> Option<&'a str> {
    let shared_components = old
        .rsplit('/')
        .zip(new.rsplit('/'))
        .take_while(|(old_component, new_component)| old_component == new_component)
        .count();
    if shared_components == 0 {
        return None;
    }

    let separators = shared_components - 1;
    let length = new
        .rsplit('/')
        .take(shared_components)
        .map(str::len)
        .sum::<usize>()
        + separators;
    Some(&new[new.len() - length..])
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

    /// Contract: docs/spec/architecture.md#file-tree (path naming).
    #[test]
    fn plain_dev_null_sections_show_the_path_as_written() {
        assert_eq!(
            identity("--- /dev/null\n+++ b/added\n@@ -0,0 +1 @@\n+a\n"),
            path(FileState::Added, "b/added")
        );
        assert_eq!(
            identity("--- a/gone\n+++ /dev/null\n@@ -1 +0,0 @@\n-a\n"),
            path(FileState::Deleted, "a/gone")
        );
    }

    #[test]
    fn names_a_file_by_the_path_suffix_both_headers_share() {
        let cases = [
            ("a/src/x.rs", "b/src/x.rs", "src/x.rs"),
            ("i/src/x.rs", "w/src/x.rs", "src/x.rs"),
            ("src/x.rs", "src/x.rs", "src/x.rs"),
            ("/tmp/x/foo", "/tmp/y/foo", "foo"),
            ("tmp/x1", "tmp/x2", "tmp/x2"),
        ];

        for (old, new, expected) in cases {
            let input = format!("--- {old}\n+++ {new}\n@@ -1 +1 @@\n-a\n+b\n");
            assert_eq!(
                identity(&input),
                path(FileState::Modified, expected),
                "{old} {new}"
            );
        }
    }

    #[test]
    fn git_dev_null_sections_take_their_name_from_the_git_header() {
        assert_eq!(
            identity(
                "diff --git i/src/new w/src/new\nnew file mode 100644\n--- /dev/null\n+++ w/src/new\n@@ -0,0 +1 @@\n+a\n"
            ),
            path(FileState::Added, "src/new")
        );
        assert_eq!(
            identity(
                "diff --git src/gone src/gone\ndeleted file mode 100644\n--- src/gone\n+++ /dev/null\n@@ -1 +0,0 @@\n-a\n"
            ),
            path(FileState::Deleted, "src/gone")
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
