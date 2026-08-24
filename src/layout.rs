use crate::{
    document::{DiffDocument, sanitize},
    interaction::{DiffLayout, Interaction},
    render::{RenderedLine, RenderedLineKind, render_file_lines},
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
    pub line_kinds: Vec<RenderedLineKind>,
    pub side_by_side_rows: Vec<SideBySideRow>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SideBySideRow {
    Shared(SideBySideLine),
    Paired {
        before: Option<SideBySideLine>,
        after: Option<SideBySideLine>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideBySideLine {
    pub bytes: Vec<u8>,
    pub kind: RenderedLineKind,
    pub source_index: usize,
}

impl Layout {
    pub fn line_count(&self, display_layout: DiffLayout) -> usize {
        match display_layout {
            DiffLayout::Unified => self.diff_lines.len(),
            DiffLayout::Vertical | DiffLayout::Stacked => self.side_by_side_rows.len(),
        }
    }
}

pub fn layout(document: &DiffDocument, interaction: &Interaction) -> Layout {
    let files = document
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let label = file_list_label(file);
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
            line_kinds: Vec::new(),
            side_by_side_rows: Vec::new(),
        };
    };
    let rendered_lines = render_file_lines(file);
    let side_by_side_rows = side_by_side_rows(&rendered_lines);
    let (diff_lines, line_kinds): (Vec<_>, Vec<_>) = rendered_lines
        .into_iter()
        .map(|line| (line.bytes, line.kind))
        .unzip();
    Layout {
        files,
        diff_lines,
        line_kinds,
        side_by_side_rows,
    }
}

fn side_by_side_rows(rendered_lines: &[RenderedLine]) -> Vec<SideBySideRow> {
    let mut rows = Vec::new();
    let mut index = 0;

    while index < rendered_lines.len() {
        let line = &rendered_lines[index];
        if !is_changed_line(line) {
            rows.push(SideBySideRow::Shared(side_by_side_line(line, index)));
            index += 1;
            continue;
        }

        let mut before = Vec::new();
        let mut after = Vec::new();
        while index < rendered_lines.len() && is_changed_line(&rendered_lines[index]) {
            let line = &rendered_lines[index];
            let side_line = side_by_side_line(line, index);
            match line.bytes.first() {
                Some(b'-') => before.push(side_line),
                Some(b'+') => after.push(side_line),
                _ => unreachable!("changed rows have a change marker"),
            }
            index += 1;
        }

        let pair_count = before.len().max(after.len());
        for pair_index in 0..pair_count {
            rows.push(SideBySideRow::Paired {
                before: before.get(pair_index).cloned(),
                after: after.get(pair_index).cloned(),
            });
        }
    }

    rows
}

fn is_changed_line(line: &RenderedLine) -> bool {
    matches!(line.kind, RenderedLineKind::Record(_))
        && matches!(line.bytes.first(), Some(b'+') | Some(b'-'))
}

fn side_by_side_line(line: &RenderedLine, source_index: usize) -> SideBySideLine {
    SideBySideLine {
        bytes: line.bytes.clone(),
        kind: line.kind,
        source_index,
    }
}

fn file_list_label(file: &crate::document::DiffFile) -> String {
    match file {
        crate::document::DiffFile::Metadata { lines } => lines
            .first()
            .map(|line| git_metadata_label(&sanitize(&line.text)))
            .unwrap_or_default(),
        crate::document::DiffFile::Unified(file) => match file.old_path.as_str() {
            "/dev/null" => display_path(&file.new_path),
            _ => display_path(&file.old_path),
        },
    }
}

fn git_metadata_label(header: &str) -> String {
    let Some(paths) = header.strip_prefix("diff --git ") else {
        return header.to_owned();
    };
    let Some((old_path, new_path)) = paths.split_once(" b/") else {
        return header.to_owned();
    };
    let old_path = display_path(old_path);
    let new_path = display_path(&format!("b/{new_path}"));

    match old_path == new_path {
        true => old_path,
        false => format!("{old_path} → {new_path}"),
    }
}

fn display_path(path: &str) -> String {
    let sanitized = sanitize(path);
    sanitized
        .strip_prefix("a/")
        .or_else(|| sanitized.strip_prefix("b/"))
        .unwrap_or(&sanitized)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{FileListRow, PaneLayout, SideBySideRow, layout, pane_layout};
    use crate::{
        interaction::{DiffLayout, Interaction, Viewport},
        parser::parse_unified_diff,
        render::RenderedLineKind,
    };

    #[test]
    fn derives_selected_file_rows() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-old\n+new\n@@ -1 +1 @@\n-old-again\n+new-again\n",
        )
        .expect("valid multi-file diff");
        let interaction = Interaction {
            selected_file: 1,
            layout: DiffLayout::Unified,
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
                    label: "first".to_owned(),
                    selected: false,
                },
                FileListRow {
                    label: "second".to_owned(),
                    selected: true,
                },
            ]
        );
        assert_eq!(view.line_kinds[2], RenderedLineKind::HunkHeader);
        assert_eq!(view.line_kinds[5], RenderedLineKind::HunkHeader);
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
    fn lays_out_a_metadata_only_file() {
        let document =
            parse_unified_diff(include_bytes!("../tests/fixtures/git-rename-only.patch"))
                .expect("valid metadata-only Git fixture");
        let interaction = Interaction {
            selected_file: 0,
            layout: DiffLayout::Unified,
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };

        let view = layout(&document, &interaction);

        assert_eq!(view.files[0].label, "old-name.txt → new-name.txt");
        assert!(
            view.line_kinds
                .iter()
                .all(|kind| matches!(kind, RenderedLineKind::Metadata))
        );
        assert_eq!(
            view.diff_lines,
            include_bytes!("../tests/fixtures/git-rename-only.patch")
                .split_inclusive(|byte| *byte == b'\n')
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn aligns_asymmetric_changed_blocks_with_blank_vertical_peers() {
        let document = parse_unified_diff(
            b"--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        )
        .expect("valid asymmetric diff");
        let interaction = Interaction {
            selected_file: 0,
            layout: DiffLayout::Unified,
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };

        let view = layout(&document, &interaction);

        assert_eq!(view.side_by_side_rows.len(), 6);
        assert_eq!(view.line_count(DiffLayout::Unified), 7);
        assert_eq!(view.line_count(DiffLayout::Vertical), 6);
        assert!(matches!(
            &view.side_by_side_rows[3],
            SideBySideRow::Paired {
                before: Some(before),
                after: Some(after),
            } if before.bytes == b"-old first\n" && after.bytes == b"+new first\n"
        ));
        assert!(matches!(
            &view.side_by_side_rows[4],
            SideBySideRow::Paired {
                before: Some(before),
                after: None,
            } if before.bytes == b"-old second\n"
        ));
    }

    #[test]
    fn labels_an_added_file_with_its_new_path() {
        let document =
            parse_unified_diff(b"--- /dev/null\n+++ b/added-file.txt\n@@ -0,0 +1 @@\n+new\n")
                .expect("valid added-file diff");
        let interaction = Interaction {
            selected_file: 0,
            layout: DiffLayout::Unified,
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };

        let view = layout(&document, &interaction);

        assert_eq!(view.files[0].label, "added-file.txt");
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
