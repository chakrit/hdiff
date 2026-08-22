use crate::{
    document::{DiffDocument, sanitize},
    interaction::Interaction,
    render::render_file,
};

#[derive(Debug, PartialEq, Eq)]
pub struct FileListRow {
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    pub files: Vec<FileListRow>,
    pub diff_lines: Vec<Vec<u8>>,
    pub hunk_offsets: Vec<usize>,
}

pub fn layout(document: &DiffDocument, interaction: &Interaction) -> Layout {
    let files = document
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let label = match file {
                crate::document::DiffFile::Metadata { lines } => lines
                    .first()
                    .map(|line| sanitize(&line.text))
                    .unwrap_or_default(),
                crate::document::DiffFile::Unified(file) => match file.old_path.as_str() {
                    "/dev/null" => sanitize(&file.new_path),
                    _ => sanitize(&file.old_path),
                },
            };
            FileListRow {
                label,
                selected: index == interaction.selected_file,
            }
        })
        .collect();
    let Some(file) = document.files.get(interaction.selected_file) else {
        return Layout {
            files,
            diff_lines: Vec::new(),
            hunk_offsets: Vec::new(),
        };
    };
    let diff_lines = render_file(file)
        .split_inclusive(|byte| *byte == b'\n')
        .map(ToOwned::to_owned)
        .collect();
    let hunk_offsets = match file {
        crate::document::DiffFile::Metadata { .. } => Vec::new(),
        crate::document::DiffFile::Unified(file) => {
            let mut line_offset = file.metadata.len() + 2;
            file.hunks
                .iter()
                .map(|hunk| {
                    let offset = line_offset;
                    line_offset += 1 + hunk.records.len();
                    offset
                })
                .collect()
        }
    };

    Layout {
        files,
        diff_lines,
        hunk_offsets,
    }
}

#[cfg(test)]
mod tests {
    use super::{FileListRow, layout};
    use crate::{
        interaction::{Interaction, Viewport},
        parser::parse_unified_diff,
    };

    #[test]
    fn derives_selected_file_rows_and_hunk_offsets() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-old\n+new\n@@ -1 +1 @@\n-old-again\n+new-again\n",
        )
        .expect("valid multi-file diff");
        let interaction = Interaction {
            selected_file: 1,
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };

        let view = layout(&document, &interaction);

        assert_eq!(
            view.files,
            vec![
                FileListRow {
                    label: "a/first".to_owned(),
                    selected: false,
                },
                FileListRow {
                    label: "a/second".to_owned(),
                    selected: true,
                },
            ]
        );
        assert_eq!(view.hunk_offsets, vec![2, 5]);
        assert_eq!(
            view.diff_lines,
            vec![
                b"--- a/second\n".to_vec(),
                b"+++ b/second\n".to_vec(),
                b"@@ -1 +1 @@\n".to_vec(),
                b"-old\n".to_vec(),
                b"+new\n".to_vec(),
                b"@@ -1 +1 @@\n".to_vec(),
                b"-old-again\n".to_vec(),
                b"+new-again\n".to_vec(),
            ]
        );
    }

    #[test]
    fn lays_out_a_metadata_only_file_without_hunk_offsets() {
        let document =
            parse_unified_diff(include_bytes!("../tests/fixtures/git-rename-only.patch"))
                .expect("valid metadata-only Git fixture");
        let interaction = Interaction {
            selected_file: 0,
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };

        let view = layout(&document, &interaction);

        assert_eq!(
            view.files[0].label,
            "diff --git a/old-name.txt b/new-name.txt"
        );
        assert_eq!(view.hunk_offsets, Vec::<usize>::new());
        assert_eq!(
            view.diff_lines,
            include_bytes!("../tests/fixtures/git-rename-only.patch")
                .split_inclusive(|byte| *byte == b'\n')
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn labels_an_added_file_with_its_new_path() {
        let document =
            parse_unified_diff(b"--- /dev/null\n+++ b/added-file.txt\n@@ -0,0 +1 @@\n+new\n")
                .expect("valid added-file diff");
        let interaction = Interaction {
            selected_file: 0,
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };

        let view = layout(&document, &interaction);

        assert_eq!(view.files[0].label, "b/added-file.txt");
    }
}
