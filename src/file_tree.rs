//! The file list: top-level files first, then files under directory headings, each part
//! in input order; while more input is coming, a `(parsing...)` line ends it. Rules:
//! docs/spec/architecture.md#file-tree.

use std::num::NonZeroUsize;

use crate::{
    document::FileView,
    file_identity::{FileIdentity, FileState},
    screen::ScreenLines,
};

/// One line of the file list: a directory heading, a file with its change mark, or the
/// line saying more files are coming.
#[derive(Debug, PartialEq, Eq)]
pub enum TreeRow {
    Directory(Box<str>),
    File { marker: StateMarker, name: Box<str> },
    Parsing,
}

/// The mark a file row shows for how its file changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateMarker {
    State(FileState),
    Unknown,
    /// The file's section failed to parse.
    Broken,
}

impl StateMarker {
    /// The mark as the file row draws it.
    pub fn label(self) -> &'static str {
        match self {
            Self::State(FileState::Modified) => "~",
            Self::State(FileState::Added) => "+",
            Self::State(FileState::Deleted) => "-",
            Self::State(FileState::Renamed) => "*",
            Self::Unknown => "?",
            Self::Broken => "X",
        }
    }
}

/// The text of the line that ends the file list while more input is coming.
pub const PARSING: &str = "(parsing...)";

/// The name a broken file's row shows when no readable header names it.
const UNNAMED: &str = "(unnamed)";

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

    /// The `height`-row window that shows this row on the `pinned` line of the window
    /// when one is pinned and fits; otherwise the window ends at this row, or starts at
    /// the list's top when this row is within the first `height` rows.
    pub fn window(self, height: NonZeroUsize, pinned: Option<usize>) -> RowWindow {
        let unpinned = self.0.min(height.get() - 1);
        let selected_offset = pinned.map_or(unpinned, |line| line.min(unpinned));

        RowWindow {
            first: Self(self.0 - selected_offset),
            selected_offset,
        }
    }
}

/// A window of file-list rows that shows one selected row.
#[derive(Debug, PartialEq, Eq)]
pub struct RowWindow {
    pub first: TreeRowIndex,
    /// Rows from `first` down to the selected row.
    pub selected_offset: usize,
}

/// The file-list rows of a document with files.
#[derive(Debug, PartialEq, Eq)]
pub struct FileTree {
    rows: Box<[TreeRow]>,
}

/// A file list with the row of the file a page shows.
#[derive(Debug, PartialEq, Eq)]
pub struct FileSelection<'a> {
    pub tree: &'a FileTree,
    pub selected: TreeRowIndex,
    /// The window line the selected row keeps while files arrive above it, if pinned.
    pub pinned_line: Option<usize>,
}

/// Whether more input is coming after the files listed so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrival {
    More,
    Ended,
}

/// The file list of files given in input order: its rows, the row of each file in input
/// order, and the input positions of the files in list order.
#[derive(Debug, PartialEq, Eq)]
pub struct ListedTree {
    pub tree: FileTree,
    pub rows: Box<[TreeRowIndex]>,
    pub order: Box<[usize]>,
}

impl ListedTree {
    /// The file list of `listings`, given in input order, top-level files first; it ends
    /// with a `(parsing...)` line while `arrival` says more input is coming.
    pub fn build(listings: &[Listing], arrival: Arrival) -> Self {
        let mut order = (0..listings.len()).collect::<Vec<_>>();
        order.sort_by_key(|&position| !listings[position].is_top_level());

        let mut tree_rows = TreeRows::default();
        let mut rows = vec![TreeRowIndex::FIRST; listings.len()];
        for &position in &order {
            rows[position] = tree_rows.push(&listings[position]);
        }
        match arrival {
            Arrival::More => tree_rows.rows.push(TreeRow::Parsing),
            Arrival::Ended => {}
        }

        Self {
            tree: tree_rows.finish(),
            rows: rows.into_boxed_slice(),
            order: order.into_boxed_slice(),
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

/// Where a file is listed and what its row shows, kept apart from the file so the list
/// can be rebuilt as files arrive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The directory the file is listed under; none for a top-level file.
    directory: Option<Box<str>>,
    name: Box<str>,
    marker: StateMarker,
}

impl Listing {
    /// Where `view`'s file is listed: by its path's directory, or at the top level under
    /// its header when the path is unknown.
    pub fn of(view: FileView<'_>) -> Self {
        Self::of_identity(&view.file.identity)
    }

    /// Where a file of `identity` is listed.
    pub fn of_identity(identity: &FileIdentity) -> Self {
        match identity {
            FileIdentity::Path { state, path } => {
                Self::of_path(path.as_str(), StateMarker::State(*state))
            }
            FileIdentity::Unknown { header } => {
                Self::top_level(header.as_str(), StateMarker::Unknown)
            }
            FileIdentity::Broken { name: Some(path) } => {
                Self::of_path(path.as_str(), StateMarker::Broken)
            }
            FileIdentity::Broken { name: None } => Self::top_level(UNNAMED, StateMarker::Broken),
        }
    }

    /// `path` split at its last `/` into its directory and its file name.
    fn of_path(path: &str, marker: StateMarker) -> Self {
        match path.rsplit_once('/') {
            Some(("", name)) => Self::top_level(name, marker),
            Some((directory, name)) => Self {
                directory: Some(directory.into()),
                name: name.into(),
                marker,
            },
            None => Self::top_level(path, marker),
        }
    }

    fn top_level(name: &str, marker: StateMarker) -> Self {
        Self {
            directory: None,
            name: name.into(),
            marker,
        }
    }

    /// Whether the file is listed at the top level, before every directory heading.
    fn is_top_level(&self) -> bool {
        self.directory.is_none()
    }
}

/// File-list rows built in list order, with the directory being listed.
#[derive(Default)]
struct TreeRows<'a> {
    rows: Vec<TreeRow>,
    /// The directory of the last file listed; none while listing top-level files.
    directory: Option<&'a str>,
}

impl<'a> TreeRows<'a> {
    /// Lists `listing`'s file after the rows so far, under its directory's heading when
    /// it opens a new group, and returns its row.
    fn push(&mut self, listing: &'a Listing) -> TreeRowIndex {
        let directory = listing.directory.as_deref();
        if directory != self.directory {
            if let Some(directory) = directory {
                let heading = format!("{directory}/");
                self.rows.push(TreeRow::Directory(heading.into()));
            }
            self.directory = directory;
        }

        let row = TreeRowIndex(self.rows.len());
        self.rows.push(TreeRow::File {
            marker: listing.marker,
            name: listing.name.clone(),
        });
        row
    }

    /// The rows listed so far, as the finished file list.
    fn finish(self) -> FileTree {
        FileTree {
            rows: self.rows.into_boxed_slice(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::{Arrival, ListedTree, Listing, RowWindow, TreeRow, TreeRowIndex};
    use crate::{
        document::{DiffDocument, FileView},
        file_identity::FileIdentity,
        parser::parse_patch,
    };

    fn parsed(input: &str) -> DiffDocument {
        parse_patch(input.as_bytes()).expect("valid diff")
    }

    /// The file list of `document`'s files, which holds the whole input.
    fn listing_of(document: &DiffDocument) -> ListedTree {
        let listings = document.file_views().map(Listing::of).collect::<Vec<_>>();

        ListedTree::build(&listings, Arrival::Ended)
    }

    fn modified(path: &str) -> String {
        format!("--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n")
    }

    fn row_text(row: &TreeRow) -> String {
        match row {
            TreeRow::Directory(heading) => heading.to_string(),
            TreeRow::File { marker, name } => format!("{} {name}", marker.label()),
            TreeRow::Parsing => super::PARSING.to_owned(),
        }
    }

    fn rows(input: &str) -> Vec<String> {
        let document = parsed(input);

        listing_of(&document)
            .tree
            .rows()
            .iter()
            .map(row_text)
            .collect()
    }

    /// Each file in list order, as the input names it, and the row its selection marks.
    fn selections(input: &str) -> Vec<(String, String)> {
        let document = parsed(input);
        let listing = listing_of(&document);
        let views = document.file_views().collect::<Vec<FileView<'_>>>();

        listing
            .order
            .iter()
            .map(|&position| {
                let named = match &views[position].file.identity {
                    FileIdentity::Path { path, .. } => path.as_str().to_owned(),
                    FileIdentity::Unknown { header } => header.as_str().to_owned(),
                    FileIdentity::Broken { name } => {
                        name.as_ref().map_or("", |name| name.as_str()).to_owned()
                    }
                };
                let row = listing.rows[position];
                (named, row_text(&listing.tree.rows()[row.get()]))
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
            TreeRowIndex(1).window(height, None),
            RowWindow {
                first: TreeRowIndex(0),
                selected_offset: 1,
            }
        );
        assert_eq!(
            TreeRowIndex(7).window(height, None),
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
