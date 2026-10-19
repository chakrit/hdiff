//! The file list: top-level files first, then files under directory headings, each part
//! in input order. Rules: docs/spec/architecture.md#file-tree.

use std::num::NonZeroUsize;

use crate::{
    document::{DiffFiles, FileView},
    file_identity::{FileIdentity, FileState},
    non_empty::NonEmpty,
    screen::ScreenLines,
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
    pub const FIRST: Self = Self(0);

    pub fn get(self) -> usize {
        self.0
    }

    /// The screen line this row takes among the list's lines: a tree row is one line.
    pub fn screen_line(self) -> ScreenLines {
        ScreenLines::new(self.0)
    }

    /// The `height`-row window that ends at this row, or starts at the list's top when
    /// this row is within the first `height` rows.
    pub fn window(self, height: NonZeroUsize) -> RowWindow {
        let selected_offset = self.0.min(height.get() - 1);

        RowWindow {
            first: Self(self.0 - selected_offset),
            selected_offset,
        }
    }
}

/// A window of file-list rows that shows one selected row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowWindow {
    pub first: TreeRowIndex,
    /// Rows from `first` down to the selected row.
    pub selected_offset: usize,
}

/// A file in list order, with the tree row that shows it.
#[derive(Clone, Copy)]
pub struct ListedFile<'a> {
    pub view: FileView<'a>,
    pub row: TreeRowIndex,
}

/// The file-list rows; a document without files lists none.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FileTree {
    rows: Box<[TreeRow]>,
}

/// A built file tree and its files in list order. A file's list position is its
/// selection index.
pub struct ListedTree<'a> {
    pub tree: FileTree,
    pub files: NonEmpty<ListedFile<'a>>,
}

impl<'a> ListedTree<'a> {
    pub fn build(files: &'a DiffFiles) -> Self {
        let placed = files.views().map(|view| Placed::of(*view));
        let ordered = list_order(&placed);

        let mut rows = Vec::new();
        let mut directory = "";
        let listed = ordered.map(|file| {
            if file.directory != directory {
                let heading = format!("{}/", file.directory);
                rows.push(TreeRow::Directory(heading.into()));
                directory = file.directory;
            }
            let row = TreeRowIndex(rows.len());
            rows.push(TreeRow::File {
                marker: file.marker,
                name: file.name.into(),
            });
            ListedFile {
                view: file.view,
                row,
            }
        });

        Self {
            tree: FileTree {
                rows: rows.into_boxed_slice(),
            },
            files: listed,
        }
    }
}

impl FileTree {
    pub fn rows(&self) -> &[TreeRow] {
        &self.rows
    }

    /// The screen lines the list takes: one per row.
    pub fn line_count(&self) -> ScreenLines {
        ScreenLines::new(self.rows.len())
    }

    /// The rows from `first` down.
    pub fn rows_from(&self, first: TreeRowIndex) -> &[TreeRow] {
        &self.rows[first.0..]
    }
}

// Rows

/// A file view with where its row goes and what it shows.
#[derive(Clone, Copy)]
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

    fn is_top_level(&self) -> bool {
        self.directory.is_empty()
    }
}

/// `placed` in list order: top-level files come first, so a directory heading never needs
/// to be closed, and each group keeps input order.
fn list_order<'a>(placed: &NonEmpty<Placed<'a>>) -> NonEmpty<Placed<'a>> {
    let first = *placed.first();
    let (top_level, nested): (Vec<_>, Vec<_>) = placed
        .rest()
        .iter()
        .copied()
        .partition(Placed::is_top_level);
    let mut top_level = top_level.into_iter();

    match (first.is_top_level(), top_level.next()) {
        (true, next) => NonEmpty::new(first, next.into_iter().chain(top_level).chain(nested)),
        (false, Some(head)) => NonEmpty::new(head, top_level.chain([first]).chain(nested)),
        (false, None) => NonEmpty::new(first, nested),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::{ListedTree, RowWindow, TreeRow, TreeRowIndex};
    use crate::{document::DiffDocument, file_identity::FileIdentity, parser::parse_patch};

    fn document(input: &str) -> DiffDocument {
        parse_patch(input.as_bytes()).expect("valid diff")
    }

    fn listing(document: &DiffDocument) -> ListedTree<'_> {
        let DiffDocument::Files(files) = document else {
            panic!("a document with files");
        };
        ListedTree::build(files)
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

    #[test]
    fn a_window_ends_at_its_selection_or_starts_at_the_top() {
        let height = NonZeroUsize::new(3).expect("nonzero height");

        assert_eq!(
            TreeRowIndex(1).window(height),
            RowWindow {
                first: TreeRowIndex(0),
                selected_offset: 1,
            }
        );
        assert_eq!(
            TreeRowIndex(7).window(height),
            RowWindow {
                first: TreeRowIndex(5),
                selected_offset: 2,
            }
        );
    }

    /// docs/spec/architecture.md#file-tree: root-level files first.
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
