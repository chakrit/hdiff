//! Which file a diff section names and how it changed, decoded from Git metadata and
//! `---` / `+++` headers. Rules: docs/spec/architecture.md#file-tree.

use crate::{
    headers::{
        FileHeader, GitHeader, GitMetadata, GitNames, HeaderPath, git_header_names, quoted_or_plain,
    },
    non_empty::NonEmpty,
    side::BySide,
    source_text::{SourceLine, sanitize},
};

/// How a section changed its file.
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
    pub fn of_unified(
        git: Option<&GitHeader<Box<[SourceLine]>>>,
        headers: &BySide<FileHeader>,
    ) -> Self {
        let git_line = git.map(|git| &git.line);
        let paths = headers.as_ref().map(|header| &header.path);
        let change = match git {
            Some(git) => git_change(git),
            None => Some(GitChange::Unmarked),
        };
        let located = change.and_then(|change| change.identity(|| read_headers(git_line, paths)));

        let first_line = git_line.unwrap_or(&headers.old.line);
        located.unwrap_or_else(|| Self::unknown(first_line))
    }

    /// A Git section without `---` / `+++` headers.
    pub fn of_metadata(git: &GitHeader<NonEmpty<SourceLine>>) -> Self {
        let located = git_change(git).and_then(|change| {
            change.identity(|| {
                let path = git_header_path(&git.line)?;
                Some(Located::new(FileState::Modified, path))
            })
        });

        located.unwrap_or_else(|| Self::unknown(&git.line))
    }

    /// A section named by its first line because its path cannot be determined.
    fn unknown(first_line: &SourceLine) -> Self {
        Self::Unknown {
            header: DisplayText::of_line(first_line),
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

    /// The visible text of a source line, which its sanitized text already is.
    pub fn of_line(line: &SourceLine) -> Self {
        Self(line.sanitized().into())
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

impl GitChange {
    /// The file this change names: a rename or copy names its destination without
    /// consulting `headers`; any other change takes the path `headers` names, in the
    /// state they imply unless Git marks the file added or deleted.
    fn identity(self, headers: impl FnOnce() -> Option<Located>) -> Option<FileIdentity> {
        let located = match self {
            Self::Unmarked => headers()?,
            Self::Added => headers()?.in_state(FileState::Added),
            Self::Deleted => headers()?.in_state(FileState::Deleted),
            Self::Renamed(path) => Located::new(FileState::Renamed, path),
            Self::Copied(path) => Located::new(FileState::Added, path),
        };

        Some(FileIdentity::Path {
            state: located.state,
            path: DisplayText::of_decoded(&located.path),
        })
    }
}

/// A file's state and decoded path, before the path is made safe to draw.
struct Located {
    state: FileState,
    path: Box<str>,
}

impl Located {
    fn new(state: FileState, path: Box<str>) -> Self {
        Self { state, path }
    }

    /// The same path, with the state a Git header states instead.
    fn in_state(self, state: FileState) -> Self {
        Self { state, ..self }
    }
}

/// The state the `---` / `+++` headers imply and the path they name.
fn read_headers(git_line: Option<&SourceLine>, paths: BySide<&HeaderPath>) -> Option<Located> {
    // A `/dev/null` side leaves one header path, so Git sections name the file from
    // their `diff --git` line; plain diffs show the path as written.
    let one_sided = |path: &str| match git_line {
        Some(line) => git_header_path(line),
        None => Some(path.into()),
    };

    match (paths.old, paths.new) {
        (HeaderPath::Undecodable, _) | (_, HeaderPath::Undecodable) => None,
        (HeaderPath::DevNull, HeaderPath::DevNull) => None,
        (HeaderPath::DevNull, HeaderPath::Path(new)) => {
            Some(Located::new(FileState::Added, one_sided(new)?))
        }
        (HeaderPath::Path(old), HeaderPath::DevNull) => {
            Some(Located::new(FileState::Deleted, one_sided(old)?))
        }
        (HeaderPath::Path(old), HeaderPath::Path(new)) => Some(Located::new(
            FileState::Modified,
            shared_name(old, new).into(),
        )),
    }
}

/// The change `git`'s extended headers record, the last marking line winning; lines that
/// mark no change leave it unmarked. `None` when a rename or copy destination's quoting
/// cannot be decoded.
fn git_change<H>(git: &GitHeader<H>) -> Option<GitChange>
where
    for<'a> &'a H: IntoIterator<Item = &'a SourceLine>,
{
    (&git.extended_headers)
        .into_iter()
        .map(|line| line.sanitized())
        .try_fold(GitChange::Unmarked, |change, line| {
            match GitMetadata::of(line) {
                Some(GitMetadata::Added) => Some(GitChange::Added),
                Some(GitMetadata::Deleted) => Some(GitChange::Deleted),
                Some(GitMetadata::RenameTo(field)) => {
                    quoted_or_plain(field).map(GitChange::Renamed)
                }
                Some(GitMetadata::CopyTo(field)) => quoted_or_plain(field).map(GitChange::Copied),
                Some(GitMetadata::Other) | None => Some(change),
            }
        })
}

/// The name a `diff --git <old> <new>` line gives: the path suffix its halves share.
fn git_header_path(line: &SourceLine) -> Option<Box<str>> {
    let names = git_header_names(line.sanitized())?;

    match GitNames::of(names)? {
        GitNames::Split(BySide { old, new }) => shared_suffix(&old, &new).map(Box::from),
        GitNames::Unsplit(names) => unquoted_git_name(names),
    }
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

    /// docs/spec/architecture.md#file-tree: path naming.
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
    fn a_rename_names_its_destination_whatever_its_headers_hold() {
        assert_eq!(
            identity(
                "diff --git a/old b/new\nsimilarity index 90%\nrename from old\nrename to new\n--- \"a/broken\\q\"\n+++ b/new\n@@ -1 +1 @@\n-a\n+b\n"
            ),
            path(FileState::Renamed, "new")
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

    /// docs/spec/architecture.md#file-tree: a header that names no path is an unknown
    /// entry, labelled with its header line.
    #[test]
    fn marks_sections_with_an_empty_header_path_unknown() {
        for (old, new) in [("", "b/x"), ("a/x", ""), ("", "")] {
            let input = format!("--- {old}\n+++ {new}\n@@ -1 +1 @@\n-a\n+b\n");

            assert_eq!(identity(&input), unknown(&format!("--- {old}")), "{input}");
        }
    }
}
