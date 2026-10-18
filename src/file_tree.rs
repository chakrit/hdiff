//! The file list: top-level files first, then files under directory headings, each part
//! in input order. Contract: docs/spec/architecture.md#file-tree.

use crate::{
    document::FileView,
    file_identity::{FileIdentity, FileState},
    source_text::sanitize,
};

#[derive(Debug, PartialEq, Eq)]
pub enum TreeRow {
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
        let identities = views.iter().map(|view| view.file.identity()).enumerate();
        let (top_level, nested): (Vec<_>, Vec<_>) =
            identities.partition(|(_, identity)| placement(identity).0.is_empty());

        // Top-level files come first, so a directory heading never needs to be closed.
        let mut rows = Vec::new();
        let mut files = Vec::new();
        let mut directory = "";
        for (view, identity) in top_level.into_iter().chain(nested) {
            let (file_directory, name) = placement(identity);
            if file_directory != directory {
                rows.push(TreeRow::Directory(format!(
                    "{}/",
                    sanitize(file_directory.as_bytes())
                )));
                directory = file_directory;
            }
            files.push(ListedFile {
                view,
                row: rows.len(),
            });
            rows.push(TreeRow::File {
                marker: marker(identity),
                name: sanitize(name.as_bytes()),
            });
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
                TreeRow::Directory(heading) => heading.clone(),
                TreeRow::File { marker, name } => format!("{} {name}", marker.label()),
            })
            .collect()
    }

    fn view_order(input: &str) -> Vec<usize> {
        tree(input).files().iter().map(|file| file.view).collect()
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
        assert_eq!(view_order(&input), [1, 3, 0, 2, 4]);
    }

    #[test]
    fn stores_each_files_row() {
        let tree = tree(&["src/b.rs", "README.md", "src/a.rs"].map(modified).concat());
        let listed = tree
            .files()
            .iter()
            .map(|file| (file.view, file.row))
            .collect::<Vec<_>>();

        assert_eq!(listed, [(1, 0), (0, 2), (2, 3)]);
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
                "- gone",
                "+ new",
                "* renamed",
                "? diff --git a/x b/y z",
                "dir/",
                "~ mode",
            ]
        );
        assert_eq!(view_order(input), [0, 1, 2, 3, 4]);
    }
}
