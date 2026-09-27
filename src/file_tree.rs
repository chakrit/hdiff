//! The file list: files under directory headings, sorted within commit groups.
//! Contract: docs/spec/architecture.md#file-tree.

use std::cmp::Ordering;

use crate::{
    document::{FileView, SourceLine, sanitize},
    file_identity::{FileIdentity, FileState},
};

/// Git abbreviates hashes to at least four digits (`--abbrev`).
const MINIMUM_HASH_LENGTH: usize = 4;
const HEADING_HASH_LENGTH: usize = 7;

#[derive(Debug, PartialEq, Eq)]
pub enum TreeRow {
    Commit(String),
    Directory(String),
    File { marker: Marker, name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    State(FileState),
    Unknown,
}

impl Marker {
    pub fn label(self) -> &'static str {
        match self {
            Self::State(FileState::Modified) => "~",
            Self::State(FileState::Added) => "+",
            Self::State(FileState::Deleted) => "-",
            Self::State(FileState::Renamed) => "*",
            Self::Unknown => "?",
        }
    }
}

/// A file in list order: its index in the views the tree was built from, and its row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListedFile {
    pub view: usize,
    pub row: usize,
}

/// File-list rows and the files they list. A file's list position is its selection
/// index.
#[derive(Debug, PartialEq, Eq)]
pub struct FileTree {
    rows: Box<[TreeRow]>,
    files: Box<[ListedFile]>,
}

impl FileTree {
    pub fn build(views: &[FileView<'_>]) -> Self {
        let groups = commit_groups(views);

        let mut rows = Vec::new();
        let mut files = Vec::new();
        for group in groups {
            if let Some(heading) = group.heading {
                rows.push(TreeRow::Commit(heading));
            }
            let mut directory = "";
            for (view, identity) in group.files {
                let (file_directory, name) = placement(identity);
                if file_directory != directory {
                    rows.push(TreeRow::Directory(directory_heading(file_directory)));
                    directory = file_directory;
                }
                files.push(ListedFile {
                    view,
                    row: rows.len(),
                });
                rows.push(TreeRow::File {
                    marker: marker(identity),
                    name: sanitize(name),
                });
            }
        }

        Self {
            rows: rows.into_boxed_slice(),
            files: files.into_boxed_slice(),
        }
    }

    pub fn rows(&self) -> &[TreeRow] {
        &self.rows
    }

    /// Files in list order.
    pub fn files(&self) -> &[ListedFile] {
        &self.files
    }
}

// Rows

fn marker(identity: &FileIdentity) -> Marker {
    match identity {
        FileIdentity::Path { state, .. } => Marker::State(*state),
        FileIdentity::Unknown { .. } => Marker::Unknown,
    }
}

/// The file's directory (empty at the top level) and the name its row shows.
fn placement(identity: &FileIdentity) -> (&str, &str) {
    match identity {
        FileIdentity::Path { path, .. } => path.rsplit_once('/').unwrap_or(("", path)),
        FileIdentity::Unknown { header } => ("", header),
    }
}

fn directory_heading(directory: &str) -> String {
    match directory {
        "" => "./".to_owned(),
        _ => format!("{}/", sanitize(directory)),
    }
}

// Commit groups

/// Files that share a commit heading, as (view index, identity) in list order.
struct CommitGroup<'a> {
    heading: Option<String>,
    files: Vec<(usize, &'a FileIdentity)>,
}

fn commit_groups<'a>(views: &[FileView<'a>]) -> Vec<CommitGroup<'a>> {
    let mut groups = Vec::new();
    let mut current = CommitGroup {
        heading: None,
        files: Vec::new(),
    };
    for (index, view) in views.iter().enumerate() {
        if let Some(heading) = commit_heading(view.leading) {
            let next = CommitGroup {
                heading: Some(heading),
                files: Vec::new(),
            };
            groups.push(std::mem::replace(&mut current, next));
        }
        current.files.push((index, view.file.identity()));
    }
    groups.push(current);

    groups.retain(|group| !group.files.is_empty());
    for group in &mut groups {
        group.files.sort_by(|(_, a), (_, b)| list_order(a, b));
    }
    groups
}

/// Paths bytewise, as Git orders them; unknown entries after every path, stable.
fn list_order(a: &FileIdentity, b: &FileIdentity) -> Ordering {
    match (a, b) {
        (FileIdentity::Path { path, .. }, FileIdentity::Path { path: other, .. }) => {
            path.cmp(other)
        }
        (FileIdentity::Path { .. }, FileIdentity::Unknown { .. }) => Ordering::Less,
        (FileIdentity::Unknown { .. }, FileIdentity::Path { .. }) => Ordering::Greater,
        (FileIdentity::Unknown { .. }, FileIdentity::Unknown { .. }) => Ordering::Equal,
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
    let short_hash = hash.get(..HEADING_HASH_LENGTH).unwrap_or(hash);
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
    let is_hash = hash.len() >= MINIMUM_HASH_LENGTH && hash.bytes().all(|b| b.is_ascii_hexdigit());

    is_hash.then_some(hash)
}

#[cfg(test)]
mod tests {
    use super::{FileTree, TreeRow};
    use crate::{document::FileView, parser::parse_unified_diff};

    fn tree(input: &str) -> FileTree {
        let document = parse_unified_diff(input.as_bytes()).expect("valid diff");
        let views = document.file_views().collect::<Vec<FileView>>();

        FileTree::build(&views)
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
                TreeRow::File { marker, name } => format!("{} {name}", marker.label()),
            })
            .collect()
    }

    fn view_order(input: &str) -> Vec<usize> {
        tree(input).files().iter().map(|file| file.view).collect()
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
        assert_eq!(view_order(&input), [6, 5, 4, 3, 2, 1, 0]);
    }

    #[test]
    fn stores_each_files_row() {
        let tree = tree(&["src/b.rs", "README.md", "src/a.rs"].map(modified).concat());
        let listed = tree
            .files()
            .iter()
            .map(|file| (file.view, file.row))
            .collect::<Vec<_>>();

        assert_eq!(listed, [(1, 0), (2, 2), (0, 3)]);
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
        assert_eq!(view_order(&input), [0, 1]);
    }

    #[test]
    fn marks_each_state_on_its_row() {
        let input = "diff --git a/gone b/gone\ndeleted file mode 100644\n\
                     diff --git a/new b/new\nnew file mode 100644\n\
                     diff --git a/old b/renamed\nrename from old\nrename to renamed\n\
                     diff --git a/x b/y z\nold mode 100644\nnew mode 100755\n\
                     diff --git a/dir/mode b/dir/mode\nold mode 100644\nnew mode 100755\n";

        assert_eq!(
            rows(input),
            [
                "dir/",
                "~ mode",
                "./",
                "- gone",
                "+ new",
                "* renamed",
                "? diff --git a/x b/y z",
            ]
        );
        assert_eq!(view_order(input), [4, 0, 1, 2, 3]);
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
        assert_eq!(view_order(&input), [1, 0, 2]);
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
