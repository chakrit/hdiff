//! The file list: top-level files first, then files under directory headings, each part
//! in input order. Contract: docs/spec/architecture.md#file-tree.

use crate::{
    document::FileView,
    file_identity::{FileIdentity, FileState},
};

#[derive(Debug, PartialEq, Eq)]
pub enum TreeRow {
    Directory(Box<str>),
    File { marker: StateMarker, name: Box<str> },
}

/// The mark a file row shows for how its file changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateMarker {
    State(FileState),
    Unknown,
}

impl StateMarker {
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

/// The position of one row among a [`FileTree`]'s rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeRowIndex(usize);

impl TreeRowIndex {
    pub fn get(self) -> usize {
        self.0
    }
}

/// A file in list order, with the tree row that shows it.
#[derive(Clone, Copy)]
pub struct ListedFile<'a> {
    pub view: FileView<'a>,
    pub row: TreeRowIndex,
}

/// The file-list rows.
#[derive(Debug, PartialEq, Eq)]
pub struct FileTree {
    rows: Box<[TreeRow]>,
}

/// A built file tree and its files in list order. A file's list position is its
/// selection index.
pub struct ListedTree<'a> {
    pub tree: FileTree,
    pub files: Box<[ListedFile<'a>]>,
}

impl<'a> ListedTree<'a> {
    pub fn build(views: impl IntoIterator<Item = FileView<'a>>) -> Self {
        let placed = views.into_iter().map(Placed::of);
        let (top_level, nested): (Vec<_>, Vec<_>) =
            placed.partition(|file| file.directory.is_empty());

        // Top-level files come first, so a directory heading never needs to be closed.
        let mut rows = Vec::new();
        let mut files = Vec::new();
        let mut directory = "";
        for file in top_level.into_iter().chain(nested) {
            if file.directory != directory {
                let heading = format!("{}/", file.directory);
                rows.push(TreeRow::Directory(heading.into()));
                directory = file.directory;
            }
            files.push(ListedFile {
                view: file.view,
                row: TreeRowIndex(rows.len()),
            });
            rows.push(TreeRow::File {
                marker: file.marker,
                name: file.name.into(),
            });
        }

        Self {
            tree: FileTree {
                rows: rows.into_boxed_slice(),
            },
            files: files.into_boxed_slice(),
        }
    }
}

impl FileTree {
    pub fn rows(&self) -> &[TreeRow] {
        &self.rows
    }
}

// Rows

/// A file view with where its row goes and what it shows.
struct Placed<'a> {
    view: FileView<'a>,
    /// Empty at the top level.
    directory: &'a str,
    name: &'a str,
    marker: StateMarker,
}

impl<'a> Placed<'a> {
    fn of(view: FileView<'a>) -> Self {
        let (directory, name, marker) = match &view.file.identity {
            FileIdentity::Path { state, path } => {
                let path = path.as_str();
                let (directory, name) = path.rsplit_once('/').unwrap_or(("", path));
                (directory, name, StateMarker::State(*state))
            }
            FileIdentity::Unknown { header } => ("", header.as_str(), StateMarker::Unknown),
        };

        Self {
            view,
            directory,
            name,
            marker,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ListedTree, TreeRow};
    use crate::{document::DiffDocument, file_identity::FileIdentity, parser::parse_patch};

    fn document(input: &str) -> DiffDocument {
        parse_patch(input.as_bytes()).expect("valid diff")
    }

    fn listing(document: &DiffDocument) -> ListedTree<'_> {
        ListedTree::build(document.file_views())
    }

    fn modified(path: &str) -> String {
        format!("--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n")
    }

    fn row_text(row: &TreeRow) -> String {
        match row {
            TreeRow::Directory(heading) => heading.to_string(),
            TreeRow::File { marker, name } => format!("{} {name}", marker.label()),
        }
    }

    fn rows(input: &str) -> Vec<String> {
        let document = document(input);

        listing(&document)
            .tree
            .rows()
            .iter()
            .map(row_text)
            .collect()
    }

    /// Each file in list order, as the input names it, and the row its selection marks.
    fn selections(input: &str) -> Vec<(String, String)> {
        let document = document(input);
        let listing = listing(&document);

        listing
            .files
            .iter()
            .map(|file| {
                let named = match &file.view.file.identity {
                    FileIdentity::Path { path, .. } => path.as_str().to_owned(),
                    FileIdentity::Unknown { header } => header.as_str().to_owned(),
                };
                (named, row_text(&listing.tree.rows()[file.row.get()]))
            })
            .collect()
    }

    fn selected(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(path, row)| (path.to_string(), row.to_string()))
            .collect()
    }

    /// Contract: docs/spec/architecture.md#file-tree (root-level files first).
    #[test]
    fn lists_top_level_files_first_then_the_rest_in_input_order() {
        let input = ["src/b.rs", "z.md", "lib/a.rs", "a.md", "src/c.rs"]
            .map(modified)
            .concat();

        assert_eq!(
            rows(&input),
            [
                "~ z.md", "~ a.md", "src/", "~ b.rs", "lib/", "~ a.rs", "src/", "~ c.rs"
            ]
        );
        assert_eq!(
            selections(&input),
            selected(&[
                ("z.md", "~ z.md"),
                ("a.md", "~ a.md"),
                ("src/b.rs", "~ b.rs"),
                ("lib/a.rs", "~ a.rs"),
                ("src/c.rs", "~ c.rs"),
            ])
        );
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
                "src/terminal/",
                "~ view.rs",
                "src/",
                "~ lib.rs",
            ]
        );
    }

    #[test]
    fn keeps_repeated_paths_as_separate_files() {
        let input = ["x", "x"].map(modified).concat();

        assert_eq!(rows(&input), ["~ x", "~ x"]);
        assert_eq!(selections(&input), selected(&[("x", "~ x"), ("x", "~ x")]));
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
                "- gone",
                "+ new",
                "* renamed",
                "? diff --git a/x b/y z",
                "dir/",
                "~ mode",
            ]
        );
        assert_eq!(
            selections(input),
            selected(&[
                ("gone", "- gone"),
                ("new", "+ new"),
                ("renamed", "* renamed"),
                ("diff --git a/x b/y z", "? diff --git a/x b/y z"),
                ("dir/mode", "~ mode"),
            ])
        );
    }
}
