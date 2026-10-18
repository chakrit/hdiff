use crate::{
    document::{FileView, RecordAddress, RecordKind, Side},
    line_breaks::wrapped_line_count,
    preferences::{DiffLayout, FileList, ViewPreferences, Wrapping},
    render::{LineKind, RenderedLine, render_file_lines, render_text_lines},
    source_text::SourceLine,
    tab_stops::expanded_width,
};

pub const MARKER_GUTTER_WIDTH: usize = 2;

/// Rows each diff pane's continuity-marker frame takes: one above and one below.
pub const PANE_FRAME_HEIGHT: usize = 2;
/// Columns each diff pane's continuity-marker frame takes: one on either side.
pub const PANE_FRAME_WIDTH: u16 = 2;

const MINIMUM_SCREEN_WIDTH: u16 = 20;
const FILE_LIST_WIDTH: u16 = 24;
const SEPARATOR_WIDTH: u16 = 1;
const SEPARATOR_PADDING: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneLayout {
    TooNarrow,
    TooShort,
    DiffOnly,
    Split {
        file_list_width: u16,
        separator_padding: u16,
    },
}

/// The screen's panes; too short when a diff pane would have no content line.
pub fn pane_layout(width: u16, height: u16, preferences: ViewPreferences) -> PaneLayout {
    if width < MINIMUM_SCREEN_WIDTH {
        return PaneLayout::TooNarrow;
    }
    if content_pane_height(height, preferences.layout) == 0 {
        return PaneLayout::TooShort;
    }

    match shows_file_list(width, preferences.file_list) {
        true => PaneLayout::Split {
            file_list_width: FILE_LIST_WIDTH,
            separator_padding: SEPARATOR_PADDING,
        },
        false => PaneLayout::DiffOnly,
    }
}

/// The file list is shown, and its column, padded separator, and at least one diff column
/// fit.
fn shows_file_list(width: u16, file_list: FileList) -> bool {
    let fits = width > FILE_COLUMN_WIDTH;

    match file_list {
        FileList::Shown => fits,
        FileList::Hidden => false,
    }
}

/// The file list and its padded separator.
const FILE_COLUMN_WIDTH: u16 =
    FILE_LIST_WIDTH + SEPARATOR_PADDING + SEPARATOR_WIDTH + SEPARATOR_PADDING;

/// Columns of the narrowest diff pane's content, inside its frame.
pub fn content_pane_width(screen_width: u16, preferences: ViewPreferences) -> u16 {
    if screen_width < MINIMUM_SCREEN_WIDTH {
        return 0;
    }
    let diff_width = match shows_file_list(screen_width, preferences.file_list) {
        true => screen_width - FILE_COLUMN_WIDTH,
        false => screen_width,
    };

    let pane_width = match preferences.layout {
        DiffLayout::Unified | DiffLayout::Stacked => diff_width,
        DiffLayout::Vertical => diff_width.saturating_sub(SEPARATOR_WIDTH) / 2,
    };
    pane_width.saturating_sub(PANE_FRAME_WIDTH)
}

/// Rows of each diff pane, frame included; stacked panes share the height around a
/// one-row separator.
pub fn pane_height(screen_height: u16, display_layout: DiffLayout) -> usize {
    let screen_height = usize::from(screen_height);

    match display_layout {
        DiffLayout::Unified | DiffLayout::Vertical => screen_height,
        DiffLayout::Stacked => screen_height.saturating_sub(STACKED_SEPARATOR_HEIGHT) / 2,
    }
}

/// Rows of each diff pane's content, inside its frame.
pub fn content_pane_height(screen_height: u16, display_layout: DiffLayout) -> usize {
    pane_height(screen_height, display_layout).saturating_sub(PANE_FRAME_HEIGHT)
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
pub struct Layout {
    pub diff_lines: Vec<RenderedLine>,
    pub side_by_side_rows: Vec<SideBySideRow>,
    line_metrics: Box<[LineMetrics]>,
}

/// Per-line facts derived once so bounds and visibility never rescan the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineMetrics {
    visibility: RowVisibility,
    /// Terminal cells the line occupies, including the two-cell marker gutter.
    width: u32,
}

/// When a rendered line shows.
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
        let line_metrics = row_visibility(&diff_lines)
            .into_iter()
            .zip(&diff_lines)
            .map(|(visibility, line)| LineMetrics {
                visibility,
                width: u32::try_from(expanded_width(line.payload()) + MARKER_GUTTER_WIDTH)
                    .expect("rendered line widths fit in u32"),
            })
            .collect();

        Layout {
            diff_lines,
            side_by_side_rows,
            line_metrics,
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
        match self
            .line_metrics
            .get(index)
            .map(|metrics| metrics.visibility)
        {
            None | Some(RowVisibility::Always) => true,
            Some(RowVisibility::NearChange(distance)) => {
                usize::try_from(distance).is_ok_and(|distance| distance <= context_lines)
            }
            Some(RowVisibility::Never) => false,
        }
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

    /// The rows `display_layout` shows at `context_lines`, indexing `diff_lines` (unified)
    /// or `side_by_side_rows` (vertical and stacked), each broken into lines by `wrap`.
    pub fn scroll_extent(
        &self,
        display_layout: DiffLayout,
        context_lines: usize,
        wrap: Wrap,
    ) -> ScrollExtent {
        match display_layout {
            DiffLayout::Unified => ScrollExtent::from_rows(
                (0..self.diff_lines.len())
                    .filter(|index| self.shows_unified_row(*index, context_lines))
                    .map(|index| VisibleRow {
                        index,
                        width: wrap.visible_width(self.line_width(index)),
                        height: self.line_height(index, wrap),
                    }),
            ),
            DiffLayout::Vertical | DiffLayout::Stacked => ScrollExtent::from_rows(
                self.side_by_side_rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| self.shows_side_by_side_row(row, context_lines))
                    .map(|(index, row)| VisibleRow {
                        index,
                        width: wrap.visible_width(self.side_by_side_width(row)),
                        height: self.side_by_side_height(row, wrap),
                    }),
            ),
        }
    }

    fn side_by_side_width(&self, row: &SideBySideRow) -> usize {
        match row {
            SideBySideRow::Shared(index)
            | SideBySideRow::BeforeOnly(index)
            | SideBySideRow::AfterOnly(index) => self.line_width(*index),
            SideBySideRow::Paired { before, after } => {
                self.line_width(*before).max(self.line_width(*after))
            }
        }
    }

    fn side_by_side_height(&self, row: &SideBySideRow, wrap: Wrap) -> usize {
        match row {
            SideBySideRow::Shared(index)
            | SideBySideRow::BeforeOnly(index)
            | SideBySideRow::AfterOnly(index) => self.line_height(*index, wrap),
            SideBySideRow::Paired { before, after } => self
                .line_height(*before, wrap)
                .max(self.line_height(*after, wrap)),
        }
    }

    fn line_width(&self, index: usize) -> usize {
        usize::try_from(self.line_metrics[index].width).expect("u32 widths fit in usize")
    }

    /// Screen lines the line takes; only lines wider than the wrap width are measured.
    fn line_height(&self, index: usize, wrap: Wrap) -> usize {
        let Wrap::At(cells) = wrap else {
            return 1;
        };
        let payload_width = self.line_width(index) - MARKER_GUTTER_WIDTH;
        if payload_width <= cells {
            return 1;
        }

        wrapped_line_count(self.diff_lines[index].payload(), payload_width, cells)
    }
}

/// How rows break into screen lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    /// Every row is one line, scrolled horizontally.
    Off,
    /// Payloads wrap at this many cells.
    At(usize),
}

impl Wrap {
    /// The wrap `preferences` select on a screen `screen_width` cells wide: both
    /// side-by-side panes wrap at the narrower pane's payload width.
    pub fn of(preferences: ViewPreferences, screen_width: u16) -> Self {
        let pane_width = usize::from(content_pane_width(screen_width, preferences));

        match preferences.wrapping {
            Wrapping::Disabled => Self::Off,
            Wrapping::Enabled => Self::At(pane_width.saturating_sub(MARKER_GUTTER_WIDTH).max(1)),
        }
    }

    /// Cells of a row `width` cells wide that the pane must scroll through.
    fn visible_width(self, width: usize) -> usize {
        match self {
            Self::Off => width,
            Self::At(cells) => width.min(cells + MARKER_GUTTER_WIDTH),
        }
    }
}

struct VisibleRow {
    index: usize,
    width: usize,
    height: usize,
}

/// One screen line: line `line` of row-list index `row`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenLine {
    pub row: usize,
    pub line: usize,
}

/// The rows one layout shows at one context count and wrap, their screen lines, and the
/// widest row. A viewport's vertical offset indexes these lines. Contract:
/// `docs/spec/architecture.md` (Viewport coordinates).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScrollExtent {
    /// Visible rows in display order, as indices into the layout's row list.
    rows: Box<[u32]>,
    /// For each visible row, the line just past its last screen line.
    line_ends: Box<[u32]>,
    /// Terminal cells of the widest visible row, including the marker gutter.
    width: usize,
}

impl ScrollExtent {
    fn from_rows(rows: impl Iterator<Item = VisibleRow>) -> Self {
        let mut indices = Vec::new();
        let mut line_ends = Vec::new();
        let mut line_end = 0;
        let mut width = 0;

        for row in rows {
            line_end += row.height;
            indices.push(u32::try_from(row.index).expect("row indices fit in u32"));
            line_ends.push(u32::try_from(line_end).expect("line counts fit in u32"));
            width = width.max(row.width);
        }
        Self {
            rows: indices.into_boxed_slice(),
            line_ends: line_ends.into_boxed_slice(),
            width,
        }
    }

    pub fn line_count(&self) -> usize {
        self.line_ends.last().map_or(0, |end| widen(*end))
    }

    pub fn width(&self) -> usize {
        self.width
    }

    /// Up to `limit` screen lines from line `offset`.
    pub fn lines(&self, offset: usize, limit: usize) -> impl Iterator<Item = ScreenLine> + '_ {
        let first = self.line_ends.partition_point(|end| widen(*end) <= offset);

        (first..self.rows.len())
            .flat_map(move |position| {
                let row = widen(self.rows[position]);
                let row_start = self.line_start(position);
                let row_end = widen(self.line_ends[position]);

                (row_start.max(offset)..row_end).map(move |line| ScreenLine {
                    row,
                    line: line - row_start,
                })
            })
            .take(limit)
    }

    /// The offset of line `line` of row-list index `index`, clamped to the row's lines; a
    /// hidden row maps to the first line of the first visible row after it.
    pub fn offset_of(&self, index: usize, line: usize) -> usize {
        let position = self.rows.partition_point(|row| widen(*row) < index);
        let start = self.line_start(position);
        let shown = self
            .rows
            .get(position)
            .is_some_and(|row| widen(*row) == index);

        match shown {
            true => start + line.min(widen(self.line_ends[position]) - start - 1),
            false => start,
        }
    }

    fn line_start(&self, position: usize) -> usize {
        match position {
            0 => 0,
            _ => widen(self.line_ends[position - 1]),
        }
    }
}

fn widen(value: u32) -> usize {
    usize::try_from(value).expect("u32 values fit in usize")
}

pub fn file_layout(view: &FileView<'_>) -> Layout {
    let diff_lines = render_text_lines(view.leading)
        .into_iter()
        .chain(render_file_lines(view.file))
        .chain(render_text_lines(view.trailing))
        .collect::<Vec<_>>();

    Layout::new(diff_lines)
}

/// The layout of a document that holds only surrounding text.
pub fn text_layout<'a>(lines: impl IntoIterator<Item = &'a SourceLine>) -> Layout {
    Layout::new(render_text_lines(lines))
}

fn side_by_side_rows(rendered_lines: &[RenderedLine]) -> Vec<SideBySideRow> {
    let mut rows = Vec::new();
    let mut index = 0;

    while index < rendered_lines.len() {
        if rendered_lines[index].kind().changed_side().is_none() {
            rows.push(SideBySideRow::Shared(index));
            index += 1;
            continue;
        }

        let mut before = Vec::new();
        let mut after = Vec::new();
        while let Some(side) = rendered_lines
            .get(index)
            .and_then(|line| line.kind().changed_side())
        {
            match side {
                Side::Old => before.push(index),
                Side::New => after.push(index),
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

fn row_visibility(rendered_lines: &[RenderedLine]) -> Vec<RowVisibility> {
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
            let LineKind::Record { address, .. } = line.kind() else {
                return None;
            };
            if line.kind().changed_side().is_some() {
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
    line.kind().record_kind() == Some(RecordKind::Context)
}

#[cfg(test)]
mod tests {
    use super::{
        PaneLayout, ScreenLine, SideBySideRow, Wrap, content_pane_height, content_pane_width,
        file_layout, maximum_horizontal_offset, pane_layout,
    };
    use crate::{
        parser::parse_unified_diff,
        preferences::{DiffLayout, FileList, ViewPreferences, Wrapping},
        render::LineKind,
    };

    #[test]
    fn lays_out_a_selected_file() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-old\n+new\n@@ -1 +1 @@\n-old-again\n+new-again\n",
        )
        .expect("valid multi-file diff");
        let view = file_layout(&document.file_views().nth(1).expect("second file"));
        assert_eq!(view.diff_lines[2].kind(), LineKind::HunkHeader);
        assert_eq!(view.diff_lines[5].kind(), LineKind::HunkHeader);
        assert_eq!(
            view.diff_lines
                .iter()
                .map(|line| line.text().as_bytes())
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
                .all(|line| matches!(line.kind(), LineKind::Metadata))
        );
        assert_eq!(
            view.diff_lines
                .iter()
                .map(|line| line.text().as_bytes())
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
        let view = file_layout(&document.file_views().next().expect("first file"));

        assert_eq!(view.side_by_side_rows.len(), 6);
        assert_eq!(view.line_count(DiffLayout::Unified), 7);
        assert_eq!(view.line_count(DiffLayout::Vertical), 6);
        assert!(matches!(
            &view.side_by_side_rows[3],
            SideBySideRow::Paired {
                before,
                after,
            } if view.diff_lines[*before].text().as_bytes() == b"-old first\n"
                && view.diff_lines[*after].text().as_bytes() == b"+new first\n"
        ));
        assert!(matches!(
            &view.side_by_side_rows[4],
            SideBySideRow::BeforeOnly(before)
                if view.diff_lines[*before].text().as_bytes() == b"-old second\n"
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
            view.scroll_extent(DiffLayout::Unified, context_lines, Wrap::Off)
                .lines(0, usize::MAX)
                .map(|line| {
                    String::from_utf8_lossy(view.diff_lines[line.row].text().as_bytes())
                        .into_owned()
                })
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
    fn measures_the_widest_visible_row_in_terminal_cells() {
        let document = parse_unified_diff(
            "--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n far context that is hidden at one line\n near\n-old\n+\t界界界界界界界界界界\n"
                .as_bytes(),
        )
        .expect("valid Unicode diff");
        let view = file_layout(&document.file_views().next().expect("first file"));

        let width = |display_layout, context_lines| {
            view.scroll_extent(display_layout, context_lines, Wrap::Off)
                .width()
        };

        assert_eq!(width(DiffLayout::Unified, 1), 24);
        assert_eq!(width(DiffLayout::Vertical, 1), 24);
        assert_eq!(width(DiffLayout::Unified, 2), 40);
    }

    #[test]
    fn maps_a_row_to_its_visible_offset_or_the_next_visible_row() {
        let document = parse_unified_diff(
            b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n",
        )
        .expect("valid diff");
        let view = file_layout(&document.file_views().next().expect("first file"));
        let extent = view.scroll_extent(DiffLayout::Unified, 1, Wrap::Off);

        // Visible rows: 0 (---), 1 (+++), 2 (@@), 5 (c2), 6 (-old), 7 (+new), 8 (c3).
        assert_eq!(extent.offset_of(5, 0), 3);
        assert_eq!(
            extent.offset_of(3, 0),
            3,
            "hidden c0 maps to the next visible row"
        );
        assert_eq!(
            extent.offset_of(9, 0),
            7,
            "past the end maps to the row count"
        );
    }

    #[test]
    fn counts_the_wrapped_lines_of_each_visible_row() {
        let document = parse_unified_diff(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-abcdefghijklmnopqrstuvwxyz0123456789abcdefghi\n+abc\n context\n",
        )
        .expect("valid diff");
        let view = file_layout(&document.file_views().next().expect("first file"));
        let unified = view.scroll_extent(DiffLayout::Unified, 3, Wrap::At(20));
        let vertical = view.scroll_extent(DiffLayout::Vertical, 3, Wrap::At(20));
        let line = |row, line| ScreenLine { row, line };

        assert_eq!(
            unified.line_count(),
            8,
            "the 45-cell deletion takes three lines"
        );
        assert_eq!(
            unified.lines(4, 3).collect::<Vec<_>>(),
            [line(3, 1), line(3, 2), line(4, 0)]
        );
        assert_eq!(unified.offset_of(3, 1), 4);
        assert_eq!(unified.offset_of(3, 9), 5, "clamped to the row's last line");
        assert_eq!(unified.offset_of(4, 0), 6);
        assert_eq!(
            unified.width(),
            22,
            "wrapped rows never scroll horizontally"
        );
        assert_eq!(
            vertical.line_count(),
            7,
            "the paired row takes its taller side's three lines"
        );
        assert_eq!(
            vertical.lines(5, 2).collect::<Vec<_>>(),
            [line(3, 2), line(4, 0)]
        );
    }

    fn preferences(layout: DiffLayout, wrapping: Wrapping, file_list: FileList) -> ViewPreferences {
        ViewPreferences {
            wrapping,
            file_list,
            ..ViewPreferences::initial(layout)
        }
    }

    #[test]
    fn wraps_both_side_by_side_panes_at_the_narrower_payload_width() {
        let wrap = |layout, wrapping| Wrap::of(preferences(layout, wrapping, FileList::Shown), 80);

        assert_eq!(wrap(DiffLayout::Vertical, Wrapping::Enabled), Wrap::At(22));
        assert_eq!(wrap(DiffLayout::Unified, Wrapping::Enabled), Wrap::At(49));
        assert_eq!(wrap(DiffLayout::Unified, Wrapping::Disabled), Wrap::Off);
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

        assert_eq!(line.text().as_bytes(), b"--- a/file\n");
        assert_eq!(line.kind(), LineKind::FileHeader);
    }

    #[test]
    fn derives_compact_pane_visibility_from_terminal_size() {
        let split = PaneLayout::Split {
            file_list_width: 24,
            separator_padding: 1,
        };
        let cases = [
            ((19, 3), DiffLayout::Unified, PaneLayout::TooNarrow),
            ((20, 2), DiffLayout::Unified, PaneLayout::TooShort),
            ((27, 3), DiffLayout::Unified, PaneLayout::DiffOnly),
            ((28, 3), DiffLayout::Unified, split),
            ((28, 3), DiffLayout::Vertical, split),
            ((28, 6), DiffLayout::Stacked, PaneLayout::TooShort),
            ((28, 7), DiffLayout::Stacked, split),
        ];

        // architecture.md: the minimum layout shows one content line in each diff pane.
        for ((width, height), layout, expected) in cases {
            assert_eq!(
                pane_layout(width, height, ViewPreferences::initial(layout)),
                expected,
                "{width}×{height} {layout:?}"
            );
        }
    }

    #[test]
    fn gives_the_diff_the_whole_width_while_the_file_list_is_hidden() {
        let hidden = preferences(DiffLayout::Unified, Wrapping::Enabled, FileList::Hidden);

        assert_eq!(pane_layout(80, 3, hidden), PaneLayout::DiffOnly);
        assert_eq!(content_pane_width(80, hidden), 78);
        assert_eq!(Wrap::of(hidden, 80), Wrap::At(76));
    }

    #[test]
    fn derives_the_narrowest_content_pane_width_for_each_layout() {
        let width = |screen_width, layout| {
            content_pane_width(screen_width, ViewPreferences::initial(layout))
        };

        assert_eq!(width(80, DiffLayout::Unified), 51);
        assert_eq!(width(80, DiffLayout::Vertical), 24);
        assert_eq!(width(80, DiffLayout::Stacked), 51);
        assert_eq!(width(23, DiffLayout::Unified), 21);
        assert_eq!(width(23, DiffLayout::Vertical), 9);
    }

    #[test]
    fn leaves_each_pane_frame_out_of_its_content_height() {
        assert_eq!(content_pane_height(36, DiffLayout::Unified), 34);
        assert_eq!(content_pane_height(36, DiffLayout::Vertical), 34);
        assert_eq!(content_pane_height(36, DiffLayout::Stacked), 15);
        assert_eq!(content_pane_height(2, DiffLayout::Unified), 0);
        assert_eq!(content_pane_height(6, DiffLayout::Stacked), 0);
    }

    #[test]
    fn bounds_horizontal_movement_to_visible_content() {
        assert_eq!(maximum_horizontal_offset(40, 20), 20);
        assert_eq!(maximum_horizontal_offset(10, 20), 0);
        assert_eq!(maximum_horizontal_offset(40, 0), 0);
        assert_eq!(maximum_horizontal_offset(usize::MAX, 1), u16::MAX);
    }
}
