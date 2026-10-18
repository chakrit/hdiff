//! The rows one layout shows at one context count and terminal-derived wrap: their screen
//! lines, the widest row, and the anchors that find a row again across rebuilds. Rules:
//! `docs/spec/architecture.md` (Viewport coordinates).

use crate::{
    compact::{narrow, widen},
    line_breaks::wrapped_line_count,
    page_rows::{LineIndex, PageRows, PairIndex},
    preferences::{DiffLayout, Wrapping},
};

pub const MARKER_GUTTER_WIDTH: usize = 2;

/// A row named by the line it starts with, and one of its wrapped lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowAnchor {
    line: LineIndex,
    wrapped_line: usize,
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
    /// The wrap `wrapping` selects for panes whose narrowest content is `content_width`
    /// cells wide: both side-by-side panes wrap at the narrower pane's payload width.
    pub fn for_panes(wrapping: Wrapping, content_width: u16) -> Self {
        let payload_width = usize::from(content_width).saturating_sub(MARKER_GUTTER_WIDTH);

        match wrapping {
            Wrapping::Disabled => Self::Off,
            Wrapping::Enabled => Self::At(payload_width.max(1)),
        }
    }

    /// Cells of a payload `width` cells wide that the pane must scroll through.
    fn visible_width(self, width: usize) -> usize {
        match self {
            Self::Off => width,
            Self::At(cells) => width.min(cells),
        }
    }
}

struct VisibleRow<R> {
    index: R,
    width: usize,
    height: usize,
}

/// One screen line: line `line` of row `row`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenLine<R> {
    pub row: R,
    pub line: usize,
}

/// The rows one layout shows at one context count and wrap: unified rows are lines, and
/// side-by-side rows are pairs.
#[derive(Debug, PartialEq, Eq)]
pub enum DisplayExtent {
    Unified(ScrollExtent<LineIndex>),
    Vertical(ScrollExtent<PairIndex>),
    Stacked(ScrollExtent<PairIndex>),
}

impl DisplayExtent {
    /// The rows of `rows` that `display_layout` shows at `context_lines`, each broken into
    /// lines by `wrap`.
    pub fn of(
        rows: &PageRows,
        display_layout: DiffLayout,
        context_lines: usize,
        wrap: Wrap,
    ) -> Self {
        let pair_rows = || {
            ScrollExtent::from_rows(
                wrap,
                rows.pair_indices()
                    .filter(|index| rows.shows_pair(*index, context_lines))
                    .map(|index| visible_row(rows, index, rows.pair(index).lines(), wrap)),
            )
        };

        match display_layout {
            DiffLayout::Unified => Self::Unified(ScrollExtent::from_rows(
                wrap,
                rows.line_indices()
                    .filter(|index| rows.shows_line(*index, context_lines))
                    .map(|index| visible_row(rows, index, std::iter::once(index), wrap)),
            )),
            DiffLayout::Vertical => Self::Vertical(pair_rows()),
            DiffLayout::Stacked => Self::Stacked(pair_rows()),
        }
    }

    pub fn line_count(&self) -> usize {
        match self {
            Self::Unified(extent) => extent.line_count(),
            Self::Vertical(extent) | Self::Stacked(extent) => extent.line_count(),
        }
    }

    /// Terminal cells of the widest visible row, including the marker gutter.
    pub fn width(&self) -> usize {
        match self {
            Self::Unified(extent) => extent.width,
            Self::Vertical(extent) | Self::Stacked(extent) => extent.width,
        }
    }

    /// How the rows break into lines: the one wrap drawing and bounds both read.
    pub fn wrap(&self) -> Wrap {
        match self {
            Self::Unified(extent) => extent.wrap,
            Self::Vertical(extent) | Self::Stacked(extent) => extent.wrap,
        }
    }

    /// The row of `rows` on screen line `offset`, if the lines reach that far.
    pub fn anchor_at(&self, rows: &PageRows, offset: usize) -> Option<RowAnchor> {
        match self {
            Self::Unified(extent) => extent.line_at(offset).map(|screen_line| RowAnchor {
                line: screen_line.row,
                wrapped_line: screen_line.line,
            }),
            Self::Vertical(extent) | Self::Stacked(extent) => {
                extent.line_at(offset).map(|screen_line| RowAnchor {
                    line: rows.pair(screen_line.row).first(),
                    wrapped_line: screen_line.line,
                })
            }
        }
    }

    /// The screen line that shows `anchor` of `rows`, as [`ScrollExtent::offset_of`] finds
    /// it.
    pub fn offset_of(&self, rows: &PageRows, anchor: RowAnchor) -> usize {
        match self {
            Self::Unified(extent) => extent.offset_of(anchor.line, anchor.wrapped_line),
            Self::Vertical(extent) | Self::Stacked(extent) => {
                extent.offset_of(rows.pair_of(anchor.line), anchor.wrapped_line)
            }
        }
    }
}

/// A row made of `lines`: as wide as its widest line and as tall as its tallest.
fn visible_row<R>(
    rows: &PageRows,
    index: R,
    lines: impl Iterator<Item = LineIndex> + Clone,
    wrap: Wrap,
) -> VisibleRow<R> {
    let payload_width = lines
        .clone()
        .map(|line| rows.payload_width(line))
        .max()
        .unwrap_or(0);
    let height = lines
        .map(|line| line_height(rows, line, wrap))
        .max()
        .unwrap_or(1);

    VisibleRow {
        index,
        width: wrap.visible_width(payload_width) + MARKER_GUTTER_WIDTH,
        height,
    }
}

/// Screen lines the line takes; only lines wider than the wrap width are measured.
fn line_height(rows: &PageRows, index: LineIndex, wrap: Wrap) -> usize {
    let Wrap::At(cells) = wrap else {
        return 1;
    };
    let payload_width = rows.payload_width(index);
    if payload_width <= cells {
        return 1;
    }

    wrapped_line_count(rows.line(index).payload(), payload_width, cells)
}

/// The rows one layout shows at one context count and wrap, their screen lines, and the
/// widest row. A viewport's vertical offset indexes these lines. Contract:
/// `docs/spec/architecture.md` (Viewport coordinates).
#[derive(Debug, PartialEq, Eq)]
pub struct ScrollExtent<R> {
    /// Visible rows in display order.
    rows: Box<[R]>,
    /// For each visible row, the line just past its last screen line.
    line_ends: Box<[u32]>,
    /// Terminal cells of the widest visible row, including the marker gutter.
    width: usize,
    wrap: Wrap,
}

impl<R: Copy + Ord> ScrollExtent<R> {
    fn from_rows(wrap: Wrap, rows: impl Iterator<Item = VisibleRow<R>>) -> Self {
        let mut indices = Vec::new();
        let mut line_ends = Vec::new();
        let mut line_end = 0;
        let mut width = 0;

        for row in rows {
            line_end += row.height;
            indices.push(row.index);
            line_ends.push(narrow(line_end));
            width = width.max(row.width);
        }
        Self {
            rows: indices.into_boxed_slice(),
            line_ends: line_ends.into_boxed_slice(),
            width,
            wrap,
        }
    }

    pub fn line_count(&self) -> usize {
        self.line_ends.last().map_or(0, |end| widen(*end))
    }

    /// Up to `limit` screen lines from line `offset`.
    pub fn lines(&self, offset: usize, limit: usize) -> impl Iterator<Item = ScreenLine<R>> + '_ {
        let first = self.line_ends.partition_point(|end| widen(*end) <= offset);

        (first..self.rows.len())
            .flat_map(move |position| {
                let row = self.rows[position];
                let row_start = self.line_start(position);
                let row_end = widen(self.line_ends[position]);

                (row_start.max(offset)..row_end).map(move |line| ScreenLine {
                    row,
                    line: line - row_start,
                })
            })
            .take(limit)
    }

    fn line_at(&self, offset: usize) -> Option<ScreenLine<R>> {
        self.lines(offset, 1).next()
    }

    /// The offset of line `line` of row `index`, clamped to the row's lines; a hidden row
    /// maps to the first line of the first visible row after it.
    pub fn offset_of(&self, index: R, line: usize) -> usize {
        let position = self.rows.partition_point(|row| *row < index);
        let start = self.line_start(position);
        let shown = self.rows.get(position).is_some_and(|row| *row == index);

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

#[cfg(test)]
mod tests {
    use super::{DisplayExtent, ScreenLine, ScrollExtent, Wrap};
    use crate::{
        page_rows::{LineIndex, PageRows, PairIndex},
        parser::parse_unified_diff,
        preferences::{DiffLayout, Wrapping},
    };

    fn rows_of(patch: &[u8]) -> PageRows {
        let document = parse_unified_diff(patch).expect("valid diff");
        PageRows::of_file(&document.file_views().next().expect("first file"))
    }

    fn texts(rows: &PageRows, lines: impl Iterator<Item = LineIndex>) -> Vec<&[u8]> {
        lines
            .map(|line| rows.line(line).text().as_bytes())
            .collect()
    }

    fn unified(extent: DisplayExtent) -> ScrollExtent<LineIndex> {
        let DisplayExtent::Unified(extent) = extent else {
            panic!("the unified layout shows lines");
        };
        extent
    }

    #[test]
    fn shows_context_rows_within_the_context_count_of_a_change_in_their_hunk() {
        let view = rows_of(
            b"--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n c0\n c1\n c2\n-old\n+new\n c3\n c4\n@@ -20,2 +20,2 @@\n far0\n far1\n",
        );
        let shown = |context_lines| {
            let extent = unified(DisplayExtent::of(
                &view,
                DiffLayout::Unified,
                context_lines,
                Wrap::Off,
            ));
            let lines = extent.lines(0, usize::MAX).map(|line| line.row);
            String::from_utf8(texts(&view, lines).concat()).expect("UTF-8 fixture")
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
        let view = rows_of(
            "--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n far context that is hidden at one line\n near\n-old\n+\t界界界界界界界界界界\n"
                .as_bytes(),
        );

        let width = |display_layout, context_lines| {
            DisplayExtent::of(&view, display_layout, context_lines, Wrap::Off).width()
        };

        assert_eq!(width(DiffLayout::Unified, 1), 24);
        assert_eq!(width(DiffLayout::Vertical, 1), 24);
        assert_eq!(width(DiffLayout::Unified, 2), 40);
    }

    #[test]
    fn maps_a_row_to_its_visible_offset_or_the_next_visible_row() {
        let view =
            rows_of(b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n");
        let extent = unified(DisplayExtent::of(&view, DiffLayout::Unified, 1, Wrap::Off));

        // Visible rows: 0 (---), 1 (+++), 2 (@@), 5 (c2), 6 (-old), 7 (+new), 8 (c3).
        assert_eq!(extent.offset_of(LineIndex::new(5), 0), 3);
        assert_eq!(
            extent.offset_of(LineIndex::new(3), 0),
            3,
            "hidden c0 maps to the next visible row"
        );
        assert_eq!(
            extent.offset_of(LineIndex::new(9), 0),
            7,
            "past the end maps to the row count"
        );
    }

    #[test]
    fn counts_the_wrapped_lines_of_each_visible_row() {
        let view = rows_of(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-abcdefghijklmnopqrstuvwxyz0123456789abcdefghi\n+abc\n context\n",
        );
        let unified = unified(DisplayExtent::of(
            &view,
            DiffLayout::Unified,
            3,
            Wrap::At(20),
        ));
        let DisplayExtent::Vertical(vertical) =
            DisplayExtent::of(&view, DiffLayout::Vertical, 3, Wrap::At(20))
        else {
            panic!("the vertical layout shows pairs");
        };
        let line = |row, line| ScreenLine {
            row: LineIndex::new(row),
            line,
        };
        let pair = |row, line| ScreenLine {
            row: PairIndex::new(row),
            line,
        };

        assert_eq!(
            unified.line_count(),
            8,
            "the 45-cell deletion takes three lines"
        );
        assert_eq!(
            unified.lines(4, 3).collect::<Vec<_>>(),
            [line(3, 1), line(3, 2), line(4, 0)]
        );
        assert_eq!(unified.offset_of(LineIndex::new(3), 1), 4);
        assert_eq!(
            unified.offset_of(LineIndex::new(3), 9),
            5,
            "clamped to the row's last line"
        );
        assert_eq!(unified.offset_of(LineIndex::new(4), 0), 6);
        assert_eq!(unified.width, 22, "wrapped rows never scroll horizontally");
        assert_eq!(
            vertical.line_count(),
            7,
            "the paired row takes its taller side's three lines"
        );
        assert_eq!(
            vertical.lines(5, 2).collect::<Vec<_>>(),
            [pair(3, 2), pair(4, 0)]
        );
    }

    #[test]
    fn wraps_both_side_by_side_panes_at_the_narrower_payload_width() {
        assert_eq!(Wrap::for_panes(Wrapping::Enabled, 24), Wrap::At(22));
        assert_eq!(Wrap::for_panes(Wrapping::Enabled, 51), Wrap::At(49));
        assert_eq!(Wrap::for_panes(Wrapping::Disabled, 51), Wrap::Off);
        assert_eq!(Wrap::for_panes(Wrapping::Enabled, 78), Wrap::At(76));
    }

    /// architecture.md (View model): an anchored row keeps its place when its layout
    /// rebuilds at another context count.
    #[test]
    fn finds_an_anchored_pair_again_in_a_rebuilt_extent() {
        let view =
            rows_of(b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n");
        let narrow = DisplayExtent::of(&view, DiffLayout::Vertical, 0, Wrap::Off);
        let wide = DisplayExtent::of(&view, DiffLayout::Vertical, 3, Wrap::Off);

        // Narrow pairs: ---, +++, @@, -old|+new. Wide adds c0, c1, and c2 before the change.
        let change = narrow.anchor_at(&view, 3).expect("the change is on line 3");

        assert_eq!(wide.offset_of(&view, change), 6);
        assert_eq!(narrow.offset_of(&view, change), 3);
    }
}
