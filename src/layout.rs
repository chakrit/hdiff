use crate::{
    document::{DiffDocument, sanitize},
    interaction::Interaction,
    render::{RecordAddress, render_file_lines},
};

const MINIMUM_SCREEN_WIDTH: u16 = 20;
const MINIMUM_SCREEN_HEIGHT: u16 = 3;
const FILE_LIST_WIDTH: u16 = 24;
const SEPARATOR_WIDTH: u16 = 1;
const SEPARATOR_PADDING: u16 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum PaneLayout {
    TooNarrow,
    TooShort,
    DiffOnly,
    Split {
        file_list_width: u16,
        separator_padding: u16,
    },
}

pub fn pane_layout(width: u16, height: u16) -> PaneLayout {
    if width < MINIMUM_SCREEN_WIDTH {
        return PaneLayout::TooNarrow;
    }
    if height < MINIMUM_SCREEN_HEIGHT {
        return PaneLayout::TooShort;
    }
    if width < FILE_LIST_WIDTH + SEPARATOR_PADDING + SEPARATOR_WIDTH + SEPARATOR_PADDING + 1 {
        return PaneLayout::DiffOnly;
    }

    PaneLayout::Split {
        file_list_width: FILE_LIST_WIDTH,
        separator_padding: SEPARATOR_PADDING,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FileListRow {
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    pub files: Vec<FileListRow>,
    pub diff_lines: Vec<Vec<u8>>,
    pub record_addresses: Vec<Option<RecordAddress>>,
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
            record_addresses: Vec::new(),
            hunk_offsets: Vec::new(),
        };
    };
    let rendered_lines = render_file_lines(file);
    let diff_lines = rendered_lines
        .iter()
        .map(|line| line.bytes.clone())
        .collect();
    let record_addresses = rendered_lines.into_iter().map(|line| line.record).collect();
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
        record_addresses,
        hunk_offsets,
    }
}

#[cfg(test)]
mod tests {
    use super::{FileListRow, PaneLayout, layout, pane_layout};
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

    #[test]
    fn derives_compact_pane_visibility_from_terminal_size() {
        let cases = [
            ((19, 3), PaneLayout::TooNarrow),
            ((20, 2), PaneLayout::TooShort),
            ((27, 3), PaneLayout::DiffOnly),
            (
                (28, 3),
                PaneLayout::Split {
                    file_list_width: 24,
                    separator_padding: 1,
                },
            ),
        ];

        for ((width, height), expected) in cases {
            assert_eq!(pane_layout(width, height), expected, "{width}×{height}");
        }
    }
}
