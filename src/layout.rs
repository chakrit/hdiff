use crate::{
    document::FileView,
    interaction::DiffLayout,
    render::{RecordAddress, RenderedLine, RenderedLineKind, render_file_lines, render_text_lines},
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

pub fn content_pane_width(screen_width: u16, display_layout: DiffLayout) -> u16 {
    let diff_width = match pane_layout(screen_width, MINIMUM_SCREEN_HEIGHT) {
        PaneLayout::TooNarrow | PaneLayout::TooShort => 0,
        PaneLayout::DiffOnly => screen_width,
        PaneLayout::Split {
            file_list_width,
            separator_padding,
        } => screen_width.saturating_sub(
            file_list_width + separator_padding + SEPARATOR_WIDTH + separator_padding,
        ),
    };

    match display_layout {
        DiffLayout::Unified | DiffLayout::Stacked => diff_width,
        DiffLayout::Vertical => diff_width.saturating_sub(SEPARATOR_WIDTH) / 2,
    }
}

/// Rows each diff pane shows; stacked panes share the height around a one-row separator.
pub fn content_pane_height(screen_height: usize, display_layout: DiffLayout) -> usize {
    match display_layout {
        DiffLayout::Unified | DiffLayout::Vertical => screen_height,
        DiffLayout::Stacked => screen_height.saturating_sub(STACKED_SEPARATOR_HEIGHT) / 2,
    }
}

const STACKED_SEPARATOR_HEIGHT: usize = 1;

pub fn maximum_horizontal_offset(content_width: usize, pane_width: u16) -> u16 {
    match pane_width {
        0 => 0,
        _ => content_width
            .saturating_sub(usize::from(pane_width))
            .try_into()
            .unwrap_or(u16::MAX),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FileListRow {
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    #[cfg(test)]
    pub files: Vec<FileListRow>,
    pub diff_lines: Vec<RenderedLine>,
    pub side_by_side_rows: Vec<SideBySideRow>,
    visibility: Box<[RowVisibility]>,
}

/// When a rendered line shows, derived once so each visibility check is constant time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowVisibility {
    Always,
    /// A context row this many records from the nearest change in its hunk.
    NearChange(u32),
    /// A context row in a hunk without changes.
    Never,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SideBySideRow {
    Shared(usize),
    Paired { before: usize, after: usize },
    BeforeOnly(usize),
    AfterOnly(usize),
}

impl Layout {
    fn new(diff_lines: Vec<RenderedLine>) -> Self {
        let side_by_side_rows = side_by_side_rows(&diff_lines);
        let visibility = row_visibility(&diff_lines);

        Layout {
            #[cfg(test)]
            files: Vec::new(),
            diff_lines,
            side_by_side_rows,
            visibility,
        }
    }

    pub fn rendered_line(&self, index: usize) -> Option<&RenderedLine> {
        self.diff_lines.get(index)
    }

    pub fn line_count(&self, display_layout: DiffLayout) -> usize {
        match display_layout {
            DiffLayout::Unified => self.diff_lines.len(),
            DiffLayout::Vertical | DiffLayout::Stacked => self.side_by_side_rows.len(),
        }
    }

    /// Context rows show within `context_lines` records of a change in their own hunk;
    /// every other row always shows.
    pub fn shows_unified_row(&self, index: usize, context_lines: usize) -> bool {
        match self.visibility.get(index) {
            None | Some(RowVisibility::Always) => true,
            Some(RowVisibility::NearChange(distance)) => {
                usize::try_from(*distance).is_ok_and(|distance| distance <= context_lines)
            }
            Some(RowVisibility::Never) => false,
        }
    }

    pub fn visible_unified_line_count(&self, context_lines: usize) -> usize {
        self.diff_lines
            .iter()
            .enumerate()
            .filter(|(index, _)| self.shows_unified_row(*index, context_lines))
            .count()
    }

    pub fn shows_side_by_side_row(&self, row: &SideBySideRow, context_lines: usize) -> bool {
        match row {
            SideBySideRow::Shared(index) => self.shows_unified_row(*index, context_lines),
            SideBySideRow::Paired { before, after } => {
                self.shows_unified_row(*before, context_lines)
                    || self.shows_unified_row(*after, context_lines)
            }
            SideBySideRow::BeforeOnly(index) | SideBySideRow::AfterOnly(index) => {
                self.shows_unified_row(*index, context_lines)
            }
        }
    }

    pub fn visible_side_by_side_line_count(&self, context_lines: usize) -> usize {
        self.side_by_side_rows
            .iter()
            .filter(|row| self.shows_side_by_side_row(row, context_lines))
            .count()
    }
}

pub fn file_layout(view: &FileView<'_>) -> Layout {
    let diff_lines = render_text_lines(view.leading)
        .into_iter()
        .chain(render_file_lines(view.file))
        .chain(render_text_lines(view.trailing))
        .collect::<Vec<_>>();

    Layout::new(diff_lines)
}

#[cfg(test)]
pub fn layout(
    document: &crate::document::DiffDocument,
    interaction: &crate::interaction::Interaction,
) -> Layout {
    let files = crate::prepared::test_file_rows(document, interaction.selected_file);
    let Some(file) = document.file_views().nth(interaction.selected_file) else {
        return Layout {
            files,
            ..Layout::new(Vec::new())
        };
    };

    let mut layout = file_layout(&file);
    layout.files = files;
    layout
}

fn side_by_side_rows(rendered_lines: &[RenderedLine]) -> Vec<SideBySideRow> {
    let mut rows = Vec::new();
    let mut index = 0;

    while index < rendered_lines.len() {
        let line = &rendered_lines[index];
        if !is_changed_line(line) {
            rows.push(SideBySideRow::Shared(index));
            index += 1;
            continue;
        }

        let mut before = Vec::new();
        let mut after = Vec::new();
        while index < rendered_lines.len() && is_changed_line(&rendered_lines[index]) {
            let line = &rendered_lines[index];
            match line.bytes.first() {
                Some(b'-') => before.push(index),
                Some(b'+') => after.push(index),
                _ => unreachable!("changed rows have a change marker"),
            }
            index += 1;
        }

        let pair_count = before.len().max(after.len());
        for pair_index in 0..pair_count {
            let before = before.get(pair_index).copied();
            let after = after.get(pair_index).copied();
            let row = match (before, after) {
                (Some(before), Some(after)) => SideBySideRow::Paired { before, after },
                (Some(before), None) => SideBySideRow::BeforeOnly(before),
                (None, Some(after)) => SideBySideRow::AfterOnly(after),
                (None, None) => unreachable!("pair count includes one changed row"),
            };
            rows.push(row);
        }
    }

    rows
}

fn row_visibility(rendered_lines: &[RenderedLine]) -> Box<[RowVisibility]> {
    let before = distances_from_previous_change(rendered_lines.iter());
    let mut after = distances_from_previous_change(rendered_lines.iter().rev());
    after.reverse();

    rendered_lines
        .iter()
        .zip(before.into_iter().zip(after))
        .map(|(line, (before, after))| {
            if !is_context_line(line) {
                return RowVisibility::Always;
            }

            before
                .into_iter()
                .chain(after)
                .min()
                .map_or(RowVisibility::Never, RowVisibility::NearChange)
        })
        .collect()
}

/// Walks lines in the given direction, measuring each record's distance in records to the
/// last change seen in the same hunk.
fn distances_from_previous_change<'a>(
    lines: impl Iterator<Item = &'a RenderedLine>,
) -> Vec<Option<u32>> {
    let mut last_change: Option<RecordAddress> = None;

    lines
        .map(|line| {
            let RenderedLineKind::Record(address) = line.kind else {
                return None;
            };
            if is_changed_line(line) {
                last_change = Some(address);
            }

            last_change
                .filter(|change| change.hunk_index == address.hunk_index)
                .map(|change| {
                    u32::try_from(change.record_index.abs_diff(address.record_index))
                        .expect("hunk record distances fit in u32")
                })
        })
        .collect()
}

fn is_context_line(line: &RenderedLine) -> bool {
    matches!(line.kind, RenderedLineKind::Record(_)) && line.bytes.first() == Some(&b' ')
}

fn is_changed_line(line: &RenderedLine) -> bool {
    matches!(line.kind, RenderedLineKind::Record(_))
        && matches!(line.bytes.first(), Some(b'+') | Some(b'-'))
}

#[cfg(test)]
mod tests {
    use super::{
        PaneLayout, SideBySideRow, content_pane_width, file_layout, layout,
        maximum_horizontal_offset, pane_layout,
    };
    use crate::{
        interaction::{DiffLayout, Interaction, ViewPreferences, Viewport},
        parser::parse_unified_diff,
        render::RenderedLineKind,
    };

    #[test]
    fn lays_out_a_selected_file() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-old\n+new\n@@ -1 +1 @@\n-old-again\n+new-again\n",
        )
        .expect("valid multi-file diff");
        let view = file_layout(&document.file_views().nth(1).expect("second file"));
        assert_eq!(view.diff_lines[2].kind, RenderedLineKind::HunkHeader);
        assert_eq!(view.diff_lines[5].kind, RenderedLineKind::HunkHeader);
        assert_eq!(
            view.diff_lines
                .iter()
                .map(|line| line.bytes.as_ref())
                .collect::<Vec<_>>(),
            vec![
                &b"--- a/second\n"[..],
                &b"+++ b/second\n"[..],
                &b"@@ -1 +1 @@\n"[..],
                &b"-old\n"[..],
                &b"+new\n"[..],
                &b"@@ -1 +1 @@\n"[..],
                &b"-old-again\n"[..],
                &b"+new-again\n"[..],
            ]
        );
    }

    #[test]
    fn lays_out_a_metadata_only_file() {
        let document =
            parse_unified_diff(include_bytes!("../tests/fixtures/git-rename-only.patch"))
                .expect("valid metadata-only Git fixture");
        let view = file_layout(&document.file_views().next().expect("first file"));
        assert!(
            view.diff_lines
                .iter()
                .all(|line| matches!(line.kind, RenderedLineKind::Metadata))
        );
        assert_eq!(
            view.diff_lines
                .iter()
                .map(|line| line.bytes.as_ref())
                .collect::<Vec<_>>(),
            include_bytes!("../tests/fixtures/git-rename-only.patch")
                .split_inclusive(|byte| *byte == b'\n')
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
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport::new(80, 8),
        };

        let view = layout(&document, &interaction);

        assert_eq!(view.side_by_side_rows.len(), 6);
        assert_eq!(view.line_count(DiffLayout::Unified), 7);
        assert_eq!(view.line_count(DiffLayout::Vertical), 6);
        assert!(matches!(
            &view.side_by_side_rows[3],
            SideBySideRow::Paired {
                before,
                after,
            } if view.diff_lines[*before].bytes.as_ref() == b"-old first\n"
                && view.diff_lines[*after].bytes.as_ref() == b"+new first\n"
        ));
        assert!(matches!(
            &view.side_by_side_rows[4],
            SideBySideRow::BeforeOnly(before)
                if view.diff_lines[*before].bytes.as_ref() == b"-old second\n"
        ));
    }

    #[test]
    fn shows_context_rows_within_the_context_count_of_a_change_in_their_hunk() {
        let document = parse_unified_diff(
            b"--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n c0\n c1\n c2\n-old\n+new\n c3\n c4\n@@ -20,2 +20,2 @@\n far0\n far1\n",
        )
        .expect("valid diff");
        let view = file_layout(&document.file_views().next().expect("first file"));
        let shown = |context_lines| {
            (0..view.diff_lines.len())
                .filter(|index| view.shows_unified_row(*index, context_lines))
                .map(|index| String::from_utf8_lossy(&view.diff_lines[index].bytes).into_owned())
                .collect::<String>()
        };

        assert_eq!(
            shown(0),
            "--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n-old\n+new\n@@ -20,2 +20,2 @@\n"
        );
        assert_eq!(
            shown(1),
            "--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n c2\n-old\n+new\n c3\n@@ -20,2 +20,2 @@\n"
        );
        assert_eq!(
            shown(usize::MAX),
            "--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n c0\n c1\n c2\n-old\n+new\n c3\n c4\n@@ -20,2 +20,2 @@\n"
        );
    }

    #[test]
    fn side_by_side_rows_reference_owned_lines() {
        let document = parse_unified_diff(b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n")
            .expect("valid diff");
        let view = file_layout(&document.file_views().next().expect("first file"));
        let SideBySideRow::Shared(source_index) = &view.side_by_side_rows[0] else {
            panic!("file header is a shared row");
        };

        let line = view
            .rendered_line(*source_index)
            .expect("side-by-side row should resolve through the layout");

        assert_eq!(line.bytes.as_ref(), b"--- a/file\n");
        assert_eq!(line.kind, RenderedLineKind::FileHeader);
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

    #[test]
    fn derives_the_narrowest_content_pane_width_for_each_layout() {
        assert_eq!(content_pane_width(80, DiffLayout::Unified), 53);
        assert_eq!(content_pane_width(80, DiffLayout::Vertical), 26);
        assert_eq!(content_pane_width(80, DiffLayout::Stacked), 53);
        assert_eq!(content_pane_width(23, DiffLayout::Unified), 23);
        assert_eq!(content_pane_width(23, DiffLayout::Vertical), 11);
    }

    #[test]
    fn bounds_horizontal_movement_to_visible_content() {
        assert_eq!(maximum_horizontal_offset(40, 20), 20);
        assert_eq!(maximum_horizontal_offset(10, 20), 0);
        assert_eq!(maximum_horizontal_offset(40, 0), 0);
        assert_eq!(maximum_horizontal_offset(usize::MAX, 1), u16::MAX);
    }
}
