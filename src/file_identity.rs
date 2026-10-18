//! Which file a diff section names and how it changed, decoded from Git metadata and
//! `---` / `+++` headers. Contract: docs/spec/architecture.md#file-tree.

use crate::{
    headers::{FileHeader, GitHeader, HeaderPath, quoted_or_plain, unquote},
    side::BySide,
    source_text::{SourceLine, sanitize},
};

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
    Path { state: FileState, path: DisplayText },
    Unknown { header: DisplayText },
}

impl FileIdentity {
    /// A section with `---` / `+++` headers, after its Git header when it has one.
    pub fn of_unified(git: Option<&GitHeader>, headers: &BySide<FileHeader>) -> Self {
        let git_line = git.map(|git| &git.line);
        let metadata = git.map_or(&[][..], |git| &git.metadata);
        let paths = headers.as_ref().map(|header| &header.path);
        let located =
            git_change(metadata).and_then(|change| locate_unified(change, git_line, paths));

        let first_line = git_line.unwrap_or(&headers.old.line);
        located.unwrap_or_else(|| Self::unknown(first_line))
    }

    /// A Git section without `---` / `+++` headers.
    pub fn of_metadata(git: &GitHeader) -> Self {
        let located =
            git_change(&git.metadata).and_then(|change| locate_metadata(change, &git.line));

        located.unwrap_or_else(|| Self::unknown(&git.line))
    }

    fn unknown(first_line: &SourceLine) -> Self {
        Self::Unknown {
            header: DisplayText(first_line.sanitized().into()),
        }
    }
}

/// Text safe to draw: decoded paths may hold control characters, which this drops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayText(Box<str>);

impl DisplayText {
    /// The visible text of a decoded path or header.
    pub fn of_decoded(text: &str) -> Self {
        Self(sanitize(text.as_bytes()).into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Locating a file

/// Git's own record of a change, the last such metadata line winning.
enum GitChange {
    Unmarked,
    Added,
    Deleted,
    Renamed(Box<str>),
    Copied(Box<str>),
}

fn locate_unified(
    change: GitChange,
    git_line: Option<&SourceLine>,
    paths: BySide<&HeaderPath>,
) -> Option<FileIdentity> {
    // A `/dev/null` side leaves one header path, so Git sections name the file from
    // their `diff --git` line; plain diffs show the path as written.
    let one_sided = |path: &str| match git_line {
        Some(line) => git_header_path(line),
        None => Some(path.into()),
    };
    let (header_state, named_path) = match (paths.old, paths.new) {
        (HeaderPath::Undecodable, _) | (_, HeaderPath::Undecodable) => return None,
        (HeaderPath::DevNull, HeaderPath::DevNull) => return None,
        (HeaderPath::DevNull, HeaderPath::Path(new)) => (FileState::Added, one_sided(new)?),
        (HeaderPath::Path(old), HeaderPath::DevNull) => (FileState::Deleted, one_sided(old)?),
        (HeaderPath::Path(old), HeaderPath::Path(new)) => {
            (FileState::Modified, shared_name(old, new).into())
        }
    };

    change.identity(header_state, || Some(named_path))
}

fn locate_metadata(change: GitChange, git_line: &SourceLine) -> Option<FileIdentity> {
    change.identity(FileState::Modified, || git_header_path(git_line))
}

impl GitChange {
    /// The file this change names: a rename or copy names its destination, any other
    /// change the path `named` gives, as `unmarked` unless Git marks it added or deleted.
    fn identity(
        self,
        unmarked: FileState,
        named: impl FnOnce() -> Option<Box<str>>,
    ) -> Option<FileIdentity> {
        let (state, path) = match self {
            Self::Unmarked => (unmarked, named()?),
            Self::Added => (FileState::Added, named()?),
            Self::Deleted => (FileState::Deleted, named()?),
            Self::Renamed(destination) => (FileState::Renamed, destination),
            Self::Copied(destination) => (FileState::Added, destination),
        };

        Some(FileIdentity::Path {
            state,
            path: DisplayText::of_decoded(&path),
        })
    }
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
fn git_header_path(line: &SourceLine) -> Option<Box<str>> {
    let names = line.sanitized().strip_prefix("diff --git ")?;
    if !names.starts_with('"') {
        return unquoted_git_name(names);
    }

    let (old, rest) = unquote(names)?;
    let new = quoted_or_plain(rest.strip_prefix(' ')?)?;
    shared_suffix(&old, &new).map(Box::from)
}

/// Unquoted names may contain spaces; the halves split at the one space whose sides
/// share a path suffix.
fn unquoted_git_name(names: &str) -> Option<Box<str>> {
    let mut named = names
        .match_indices(' ')
        .filter_map(|(space, _)| shared_suffix(&names[..space], &names[space + 1..]));
    let name = named.next()?;

    named.next().is_none().then(|| name.into())
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

#[cfg(test)]
mod tests {
    use super::{DisplayText, FileIdentity, FileState};
    use crate::parser::parse_patch;

    fn identity(input: &str) -> FileIdentity {
        let document = parse_patch(input.as_bytes()).expect("valid diff");
        let file = document.files().next().expect("one file");

        file.identity.clone()
    }

    fn path(state: FileState, path: &str) -> FileIdentity {
        FileIdentity::Path {
            state,
            path: DisplayText::of_decoded(path),
        }
    }

    fn unknown(header: &str) -> FileIdentity {
        FileIdentity::Unknown {
            header: DisplayText::of_decoded(header),
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
