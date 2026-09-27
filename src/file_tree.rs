//! The file list: files under directory headings, sorted within commit groups.
//! Contract: docs/spec/architecture.md#file-tree.

use std::str::CharIndices;

use crate::document::{DiffDocument, DiffFile, Section, SourceLine, sanitize};

const SHORT_HASH_LENGTH: usize = 7;

/// A file's change as its file-list marker shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileState {
    Modified,
    Added,
    Deleted,
    Renamed,
}

impl FileState {
    fn marker(self) -> &'static str {
        match self {
            Self::Modified => "~",
            Self::Added => "+",
            Self::Deleted => "-",
            Self::Renamed => "*",
        }
    }
}

/// A file located by its path, or an entry whose path cannot be determined.
#[derive(Debug, PartialEq, Eq)]
enum FileEntry {
    Path { state: FileState, path: String },
    Unknown { header: String },
}

impl FileEntry {
    fn of(file: &DiffFile) -> Self {
        let located = match file {
            DiffFile::Metadata { header, lines } => metadata_file(header, lines),
            DiffFile::Unified(file) => unified_file(&file.metadata, &file.old_path, &file.new_path),
        };

        match located {
            Some((state, path)) => Self::Path { state, path },
            None => Self::Unknown {
                header: first_line(file).structural_text.clone(),
            },
        }
    }

    fn marker(&self) -> &'static str {
        match self {
            Self::Path { state, .. } => state.marker(),
            Self::Unknown { .. } => "?",
        }
    }

    /// The entry's directory (empty at the top level) and the name its row shows.
    fn placement(&self) -> (&str, &str) {
        match self {
            Self::Path { path, .. } => path.rsplit_once('/').unwrap_or(("", path)),
            Self::Unknown { header } => ("", header),
        }
    }

    /// Paths bytewise, as Git orders them; unknown entries after every path.
    fn list_order(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        match (self, other) {
            (Self::Path { path, .. }, Self::Path { path: other, .. }) => path.cmp(other),
            (Self::Path { .. }, Self::Unknown { .. }) => Ordering::Less,
            (Self::Unknown { .. }, Self::Path { .. }) => Ordering::Greater,
            (Self::Unknown { .. }, Self::Unknown { .. }) => Ordering::Equal,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TreeRow {
    Commit(String),
    Directory(String),
    File { marker: &'static str, name: String },
}

/// File-list rows plus the mapping between list positions and input files. A file's
/// list position is its selection index.
#[derive(Debug, PartialEq, Eq)]
pub struct FileTree {
    rows: Box<[TreeRow]>,
    input_order: Box<[usize]>,
    file_rows: Box<[usize]>,
}

impl FileTree {
    pub fn build(document: &DiffDocument) -> Self {
        let groups = commit_groups(document);

        let mut rows = Vec::new();
        let mut input_order = Vec::new();
        let mut file_rows = Vec::new();
        for group in groups {
            if let Some(heading) = group.heading {
                rows.push(TreeRow::Commit(heading));
            }
            let mut directory = "";
            for (input, entry) in &group.files {
                let (file_directory, name) = entry.placement();
                if file_directory != directory {
                    rows.push(TreeRow::Directory(directory_heading(&sanitize(
                        file_directory,
                    ))));
                    directory = file_directory;
                }
                input_order.push(*input);
                file_rows.push(rows.len());
                rows.push(TreeRow::File {
                    marker: entry.marker(),
                    name: sanitize(name),
                });
            }
        }

        Self {
            rows: rows.into_boxed_slice(),
            input_order: input_order.into_boxed_slice(),
            file_rows: file_rows.into_boxed_slice(),
        }
    }

    pub fn rows(&self) -> &[TreeRow] {
        &self.rows
    }

    /// Input file indices in list order.
    pub fn input_order(&self) -> &[usize] {
        &self.input_order
    }

    /// The row index of the file at list position `file`.
    pub fn file_row(&self, file: usize) -> Option<usize> {
        self.file_rows.get(file).copied()
    }
}

// Entry derivation

/// Git's own record of a change: its state and, for a rename or copy, the destination.
#[derive(Default)]
struct GitChange {
    state: Option<FileState>,
    destination: Option<String>,
}

fn metadata_file(header: &SourceLine, lines: &[SourceLine]) -> Option<(FileState, String)> {
    let change = git_change(lines)?;
    let state = change.state.unwrap_or(FileState::Modified);

    match change.destination {
        Some(path) => Some((state, path)),
        None => Some((state, git_header_path(header)?)),
    }
}

fn unified_file(
    metadata: &[SourceLine],
    old_header: &str,
    new_header: &str,
) -> Option<(FileState, String)> {
    let change = git_change(metadata)?;
    let old = header_path(old_header, "a/")?;
    let new = header_path(new_header, "b/")?;
    let (header_state, header_path) = match (old, new) {
        (HeaderPath::Null, HeaderPath::Null) => return None,
        (HeaderPath::Null, HeaderPath::Path(new)) => (FileState::Added, new),
        (HeaderPath::Path(old), HeaderPath::Null) => (FileState::Deleted, old),
        (HeaderPath::Path(_), HeaderPath::Path(new)) => (FileState::Modified, new),
    };

    let state = change.state.unwrap_or(header_state);
    let path = change.destination.unwrap_or(header_path);
    Some((state, path))
}

/// `None` when a destination's quoting cannot be decoded.
fn git_change(lines: &[SourceLine]) -> Option<GitChange> {
    lines
        .iter()
        .map(|line| line.structural_text.as_str())
        .try_fold(GitChange::default(), |change, line| {
            if line.starts_with("new file mode ") {
                return Some(GitChange {
                    state: Some(FileState::Added),
                    ..change
                });
            }
            if line.starts_with("deleted file mode ") {
                return Some(GitChange {
                    state: Some(FileState::Deleted),
                    ..change
                });
            }
            if let Some(path) = line.strip_prefix("rename to ") {
                return Some(GitChange {
                    state: Some(FileState::Renamed),
                    destination: Some(quoted_or_plain(path)?),
                });
            }
            if let Some(path) = line.strip_prefix("copy to ") {
                return Some(GitChange {
                    state: Some(FileState::Added),
                    destination: Some(quoted_or_plain(path)?),
                });
            }
            Some(change)
        })
}

enum HeaderPath {
    Null,
    Path(String),
}

/// The path a `---` / `+++` header names, without its timestamp or `prefix`; `None`
/// when its quoting cannot be decoded.
fn header_path(field: &str, prefix: &str) -> Option<HeaderPath> {
    let path = match field.starts_with('"') {
        true => unquote(field)?.0,
        false => field
            .split_once('\t')
            .map_or(field, |(path, _timestamp)| path)
            .to_owned(),
    };

    match path.as_str() {
        "/dev/null" => Some(HeaderPath::Null),
        _ => Some(HeaderPath::Path(without_prefix(path, prefix))),
    }
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
    let old = without_prefix(old, "a/");
    let new = without_prefix(new, "b/");

    (old == new).then_some(new)
}

/// Splits unquoted `a/<path> b/<path>` where both halves are the same length, as Git
/// does for paths that contain spaces.
fn symmetric_halves(names: &str) -> Option<(String, String)> {
    let path_length = names.len().checked_sub(5)? / 2;
    let old = names.get(..2 + path_length)?;
    let new = names.get(2 + path_length + 1..)?;
    let separated = names.get(2 + path_length..2 + path_length + 1) == Some(" ");

    separated.then(|| (old.to_owned(), new.to_owned()))
}

fn without_prefix(path: String, prefix: &str) -> String {
    match path.strip_prefix(prefix) {
        Some(stripped) => stripped.to_owned(),
        None => path,
    }
}

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

fn first_line(file: &DiffFile) -> &SourceLine {
    match file {
        DiffFile::Metadata { header, .. } => header,
        DiffFile::Unified(file) => file.metadata.first().unwrap_or(&file.old_header),
    }
}

// Commit groups

/// Files that share a commit heading, as (input index, entry) in list order.
struct CommitGroup {
    heading: Option<String>,
    files: Vec<(usize, FileEntry)>,
}

fn commit_groups(document: &DiffDocument) -> Vec<CommitGroup> {
    let mut groups = Vec::new();
    let mut current = CommitGroup {
        heading: None,
        files: Vec::new(),
    };
    let mut input = 0;
    for section in &document.sections {
        match section {
            Section::Text(lines) => {
                if let Some(heading) = commit_heading(lines) {
                    let next = CommitGroup {
                        heading: Some(heading),
                        files: Vec::new(),
                    };
                    groups.push(std::mem::replace(&mut current, next));
                }
            }
            Section::File(file) => {
                current.files.push((input, FileEntry::of(file)));
                input += 1;
            }
        }
    }
    groups.push(current);

    groups.retain(|group| !group.files.is_empty());
    for group in &mut groups {
        group.files.sort_by(|(_, a), (_, b)| a.list_order(b));
    }
    groups
}

fn directory_heading(directory: &str) -> String {
    match directory {
        "" => "./".to_owned(),
        _ => format!("{directory}/"),
    }
}

/// The heading of the last `commit <hash>` line in surrounding text: the short hash and
/// the commit's first indented message line.
fn commit_heading(lines: &[SourceLine]) -> Option<String> {
    let (start, hash) = lines
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, line)| Some((index, commit_hash(&line.structural_text)?)))?;
    let short_hash = &hash[..SHORT_HASH_LENGTH];
    let subject = lines[start + 1..].iter().find_map(|line| {
        let message = line.structural_text.strip_prefix("    ")?.trim();
        (!message.is_empty()).then_some(message)
    });

    let heading = match subject {
        Some(subject) => format!("{short_hash} {subject}"),
        None => short_hash.to_owned(),
    };
    Some(heading)
}

fn commit_hash(line: &str) -> Option<&str> {
    let hash = line.strip_prefix("commit ")?.split(' ').next()?;
    let is_hash = hash.len() >= SHORT_HASH_LENGTH && hash.bytes().all(|b| b.is_ascii_hexdigit());

    is_hash.then_some(hash)
}

#[cfg(test)]
mod tests {
    use super::{FileEntry, FileState, FileTree, TreeRow};
    use crate::parser::parse_unified_diff;

    fn tree(input: &str) -> FileTree {
        FileTree::build(&parse_unified_diff(input.as_bytes()).expect("valid diff"))
    }

    fn modified(path: &str) -> String {
        format!("--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n")
    }

    fn rows(input: &str) -> Vec<String> {
        tree(input)
            .rows()
            .iter()
            .map(|row| match row {
                TreeRow::Commit(heading) => format!("# {heading}"),
                TreeRow::Directory(heading) => heading.clone(),
                TreeRow::File { marker, name } => format!("{marker} {name}"),
            })
            .collect()
    }

    fn entry(input: &str) -> FileEntry {
        let document = parse_unified_diff(input.as_bytes()).expect("valid diff");
        FileEntry::of(document.files().next().expect("one file"))
    }

    fn path(state: FileState, path: &str) -> FileEntry {
        FileEntry::Path {
            state,
            path: path.to_owned(),
        }
    }

    #[test]
    fn orders_files_bytewise_on_the_full_path_as_git_does() {
        let input = ["b_new", "a0", "a/x", "a.rs", "a.b", "a-b/x", "0first"]
            .map(modified)
            .concat();

        assert_eq!(
            rows(&input),
            [
                "~ 0first", "a-b/", "~ x", "./", "~ a.b", "~ a.rs", "a/", "~ x", "./", "~ a0",
                "~ b_new",
            ]
        );
        assert_eq!(tree(&input).input_order(), [6, 5, 4, 3, 2, 1, 0]);
    }

    #[test]
    fn stores_each_files_row() {
        let tree = tree(&["src/b.rs", "README.md", "src/a.rs"].map(modified).concat());

        assert_eq!(tree.file_row(0), Some(0), "README.md");
        assert_eq!(tree.file_row(1), Some(2), "src/a.rs under its heading");
        assert_eq!(tree.file_row(2), Some(3), "src/b.rs");
        assert_eq!(tree.file_row(3), None);
    }

    #[test]
    fn headings_name_only_directories_that_hold_files() {
        let input = ["docs/spec/roadmap.md", "src/terminal/view.rs", "src/lib.rs"]
            .map(modified)
            .concat();

        assert_eq!(
            rows(&input),
            [
                "docs/spec/",
                "~ roadmap.md",
                "src/",
                "~ lib.rs",
                "src/terminal/",
                "~ view.rs",
            ]
        );
    }

    #[test]
    fn keeps_repeated_paths_as_separate_files() {
        let input = ["x", "x"].map(modified).concat();

        assert_eq!(rows(&input), ["~ x", "~ x"]);
        assert_eq!(tree(&input).input_order(), [0, 1]);
    }

    #[test]
    fn marks_each_git_state() {
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
            assert_eq!(entry(input), expected, "{input}");
        }
    }

    #[test]
    fn reads_dev_null_headers_without_git_metadata() {
        assert_eq!(
            entry("--- /dev/null\n+++ b/added\n@@ -0,0 +1 @@\n+a\n"),
            path(FileState::Added, "added")
        );
        assert_eq!(
            entry("--- a/gone\n+++ /dev/null\n@@ -1 +0,0 @@\n-a\n"),
            path(FileState::Deleted, "gone")
        );
    }

    #[test]
    fn plain_diff_names_the_new_path_without_a_rename() {
        assert_eq!(
            entry(
                "--- tmp/x1\t2026-09-27 23:45:10\n+++ tmp/x2\t2026-09-27 23:45:11\n@@ -1 +1 @@\n-a\n+b\n"
            ),
            path(FileState::Modified, "tmp/x2")
        );
    }

    #[test]
    fn decodes_git_quoted_paths() {
        assert_eq!(
            entry(
                "--- \"a/\\340\\270\\201 \\\"q\\\".txt\"\n+++ \"b/\\340\\270\\201 \\\"q\\\".txt\"\n@@ -1 +1 @@\n-a\n+b\n"
            ),
            path(FileState::Modified, "ก \"q\".txt")
        );
        assert_eq!(
            entry(
                "diff --git \"a/tab\\there\" \"b/tab\\there\"\nold mode 100644\nnew mode 100755\n"
            ),
            path(FileState::Modified, "tab\there")
        );
    }

    #[test]
    fn reads_metadata_only_paths_with_spaces() {
        assert_eq!(
            entry("diff --git a/with space b/with space\nold mode 100644\nnew mode 100755\n"),
            path(FileState::Modified, "with space")
        );
    }

    #[test]
    fn marks_ambiguous_and_undecodable_entries_unknown() {
        let ambiguous = "diff --git a/x b/y z\nold mode 100644\nnew mode 100755\n";
        let undecodable = "--- \"a/broken\\q\"\n+++ \"b/broken\\q\"\n@@ -1 +1 @@\n-a\n+b\n";

        assert_eq!(
            entry(ambiguous),
            FileEntry::Unknown {
                header: "diff --git a/x b/y z".to_owned()
            }
        );
        assert_eq!(
            entry(undecodable),
            FileEntry::Unknown {
                header: "--- \"a/broken\\q\"".to_owned()
            }
        );
        let input = format!("{ambiguous}diff --git a/z b/z\n{}", modified("z"));
        assert_eq!(rows(&input), ["~ z", "? diff --git a/x b/y z"]);
        assert_eq!(tree(&input).input_order(), [1, 0]);
    }

    #[test]
    fn sorts_within_commit_groups_under_commit_headings() {
        let input = format!(
            "commit 0123456789abcdef0123456789abcdef01234567 (HEAD -> main)\n\
             Author: A <a@example.com>\n\n    Second change\n\n    Body.\n\n{}{}\
             commit fedcba9876543210fedcba9876543210fedcba98\n\
             Author: A <a@example.com>\n\n    First change\n\n{}",
            modified("z"),
            modified("a"),
            modified("m"),
        );

        assert_eq!(
            rows(&input),
            [
                "# 0123456 Second change",
                "~ a",
                "~ z",
                "# fedcba9 First change",
                "~ m",
            ]
        );
        assert_eq!(tree(&input).input_order(), [1, 0, 2]);
    }

    #[test]
    fn restarts_directory_headings_in_each_commit_group() {
        let input = format!(
            "commit 0123456789abcdef0123456789abcdef01234567\n\n    One\n\n{}\
             commit fedcba9876543210fedcba9876543210fedcba98\n\n    Two\n\n{}",
            modified("src/a.rs"),
            modified("src/b.rs"),
        );

        assert_eq!(
            rows(&input),
            [
                "# 0123456 One",
                "src/",
                "~ a.rs",
                "# fedcba9 Two",
                "src/",
                "~ b.rs"
            ]
        );
    }

    #[test]
    fn names_a_group_after_the_last_commit_in_its_text() {
        let input = format!(
            "commit 1111111111111111111111111111111111111111\n\n    Merge\n\n\
             commit 2222222222222222222222222222222222222222\n\n    Change\n\n{}",
            modified("f"),
        );

        assert_eq!(rows(&input), ["# 2222222 Change", "~ f"]);
    }

    #[test]
    fn other_surrounding_text_does_not_split_the_list() {
        let input = "Only in a: only\ndiff -ru a/y b/y\n--- a/y\n+++ b/y\n@@ -1 +1 @@\n-1\n+2\n\
                     diff -ru a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-1\n+2\n";

        assert_eq!(rows(input), ["~ x", "~ y"]);
    }
}
