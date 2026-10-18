//! The rows one pane shape shows at one context count and terminal-derived wrap width: their
//! screen lines, the widest row, and the anchors that find a row again across rebuilds.
//! Rules: `docs/spec/architecture.md` (Viewport coordinates).

use crate::{
    compact::Count,
    geometry::{PaneShape, SplitDirection},
    line_breaks::wrapped_line_count,
    page_rows::{LineIndex, PageRows, PairIndex},
    preferences::Wrapping,
    screen::{Cells, MARKER_GUTTER_WIDTH, NonZeroCells, ScreenLines},
};

/// A row named by the line it starts with, and one of its wrapped lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowAnchor {
    line: LineIndex,
    wrapped_line: usize,
}

/// The width at which rows break into screen lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapWidth {
    /// Every row is one line, scrolled horizontally.
    Unbounded,
    /// Payloads wrap at this many cells.
    At(NonZeroCells),
}

impl WrapWidth {
    /// The wrap width `wrapping` selects for panes whose narrowest content is
    /// `content_width` wide: both side-by-side panes wrap at the narrower pane's payload
    /// width. A pane with no room for payload wraps one cell per line.
    pub fn for_panes(wrapping: Wrapping, content_width: Cells) -> Self {
        let payload_width = content_width.saturating_sub(MARKER_GUTTER_WIDTH);
        let wrap_width = NonZeroCells::new(payload_width).unwrap_or(NonZeroCells::MIN);

        match wrapping {
            Wrapping::Disabled => Self::Unbounded,
            Wrapping::Enabled => Self::At(wrap_width),
        }
    }

    /// Cells of a payload `width` cells wide that the pane must scroll through.
    fn visible_width(self, width: Cells) -> Cells {
        match self {
            Self::Unbounded => width,
            Self::At(cells) => width.min(cells.get()),
        }
    }
}

struct VisibleRow<R> {
    index: R,
    width: Cells,
    height: ScreenLines,
}

/// One screen line: wrapped line `wrapped_line` of row `row`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenLine<R> {
    pub row: R,
    pub wrapped_line: usize,
}

impl<R> ScreenLine<R> {
    /// The same screen line of the row `row` becomes.
    pub fn map<T>(self, row: impl FnOnce(R) -> T) -> ScreenLine<T> {
        ScreenLine {
            row: row(self.row),
            wrapped_line: self.wrapped_line,
        }
    }
}

/// The rows one pane shape shows at one context count and wrap width: a single pane shows
/// lines, and split panes show pairs.
#[derive(Debug, PartialEq, Eq)]
pub enum DisplayExtent {
    Single(ScrollExtent<LineIndex>),
    Split {
        direction: SplitDirection,
        extent: ScrollExtent<PairIndex>,
    },
}

impl DisplayExtent {
    /// The rows of `rows` that `shape` shows at `context_lines`, each broken into lines at
    /// `wrap`.
    pub fn of(rows: &PageRows, shape: PaneShape, context_lines: usize, wrap: WrapWidth) -> Self {
        match shape {
            PaneShape::Single => Self::Single(ScrollExtent::from_rows(
                wrap,
                rows.line_indices()
                    .filter(|index| rows.shows_line(*index, context_lines))
                    .map(|index| visible_row(rows, index, std::iter::once(index), wrap)),
            )),
            PaneShape::Split(direction) => Self::Split {
                direction,
                extent: ScrollExtent::from_rows(
                    wrap,
                    rows.pair_indices()
                        .filter(|index| rows.shows_pair(*index, context_lines))
                        .map(|index| visible_row(rows, index, rows.pair(index).lines(), wrap)),
                ),
            },
        }
    }

    pub fn line_count(&self) -> ScreenLines {
        match self {
            Self::Single(extent) => extent.line_count(),
            Self::Split { extent, .. } => extent.line_count(),
        }
    }

    /// Cells of the widest visible row, including the marker gutter.
    pub fn width(&self) -> Cells {
        match self {
            Self::Single(extent) => extent.width,
            Self::Split { extent, .. } => extent.width,
        }
    }

    /// The width the rows break at: the one drawing and bounds both read.
    pub fn wrap(&self) -> WrapWidth {
        match self {
            Self::Single(extent) => extent.wrap,
            Self::Split { extent, .. } => extent.wrap,
        }
    }

    /// The row of `rows` on screen line `offset`, if the lines reach that far.
    pub fn anchor_at(&self, rows: &PageRows, offset: ScreenLines) -> Option<RowAnchor> {
        match self {
            Self::Single(extent) => extent.line_at(offset).map(|screen_line| RowAnchor {
                line: screen_line.row,
                wrapped_line: screen_line.wrapped_line,
            }),
            Self::Split { extent, .. } => extent.line_at(offset).map(|screen_line| RowAnchor {
                line: rows.pair(screen_line.row).first(),
                wrapped_line: screen_line.wrapped_line,
            }),
        }
    }

    /// The screen line that shows `anchor` of `rows`, as [`ScrollExtent::offset_of`] finds
    /// it.
    pub fn offset_of(&self, rows: &PageRows, anchor: RowAnchor) -> ScreenLines {
        match self {
            Self::Single(extent) => extent.offset_of(anchor.line, anchor.wrapped_line),
            Self::Split { extent, .. } => {
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
    wrap: WrapWidth,
) -> VisibleRow<R> {
    let payload_width = lines
        .clone()
        .map(|line| rows.payload_width(line))
        .max()
        .unwrap_or(Cells::ZERO);
    let height = lines
        .map(|line| line_height(rows, line, wrap))
        .max()
        .unwrap_or(ScreenLines::ONE);

    VisibleRow {
        index,
        width: wrap.visible_width(payload_width) + MARKER_GUTTER_WIDTH,
        height,
    }
}

/// Screen lines the line takes; only lines wider than the wrap width are measured.
fn line_height(rows: &PageRows, index: LineIndex, wrap: WrapWidth) -> ScreenLines {
    let WrapWidth::At(cells) = wrap else {
        return ScreenLines::ONE;
    };
    let payload = rows.measured_payload(index);
    if payload.cells() <= cells.get() {
        return ScreenLines::ONE;
    }

    wrapped_line_count(payload, cells)
}

/// The rows one pane shape shows at one context count and wrap width, their screen lines,
/// and the widest row. A viewport's vertical offset indexes these lines. Contract:
/// `docs/spec/architecture.md` (Viewport coordinates).
#[derive(Debug, PartialEq, Eq)]
pub struct ScrollExtent<R> {
    /// Visible rows in display order.
    rows: Box<[R]>,
    /// For each visible row, the line just past its last screen line.
    line_ends: Box<[Count]>,
    /// Cells of the widest visible row, including the marker gutter.
    width: Cells,
    wrap: WrapWidth,
}

impl<R: Copy + Ord> ScrollExtent<R> {
    fn from_rows(wrap: WrapWidth, rows: impl Iterator<Item = VisibleRow<R>>) -> Self {
        let mut indices = Vec::new();
        let mut line_ends = Vec::new();
        let mut line_end = 0;
        let mut width = Cells::ZERO;

        for row in rows {
            line_end += row.height.get();
            indices.push(row.index);
            line_ends.push(Count::of(line_end));
            width = width.max(row.width);
        }
        Self {
            rows: indices.into_boxed_slice(),
            line_ends: line_ends.into_boxed_slice(),
            width,
            wrap,
        }
    }

    pub fn line_count(&self) -> ScreenLines {
        ScreenLines::new(self.line_ends.last().map_or(0, |end| end.get()))
    }

    /// Up to `limit` screen lines from line `offset`.
    pub fn lines(
        &self,
        offset: ScreenLines,
        limit: ScreenLines,
    ) -> impl Iterator<Item = ScreenLine<R>> + '_ {
        let offset = offset.get();
        let first = self.line_ends.partition_point(|end| end.get() <= offset);

        (first..self.rows.len())
            .flat_map(move |position| {
                let row = self.rows[position];
                let row_start = self.line_start(position);
                let row_end = self.line_ends[position].get();

                (row_start.max(offset)..row_end).map(move |line| ScreenLine {
                    row,
                    wrapped_line: line - row_start,
                })
            })
            .take(limit.get())
    }

    fn line_at(&self, offset: ScreenLines) -> Option<ScreenLine<R>> {
        self.lines(offset, ScreenLines::new(1)).next()
    }

    /// The offset of wrapped line `wrapped_line` of row `index`, clamped to the row's
    /// lines; a hidden row maps to the first line of the first visible row after it.
    pub fn offset_of(&self, index: R, wrapped_line: usize) -> ScreenLines {
        let position = self.rows.partition_point(|row| *row < index);
        let start = self.line_start(position);
        let shown = self.rows.get(position).is_some_and(|row| *row == index);

        let offset = match shown {
            true => start + wrapped_line.min(self.line_ends[position].get() - start - 1),
            false => start,
        };
        ScreenLines::new(offset)
    }

    fn line_start(&self, position: usize) -> usize {
        match position {
            0 => 0,
            _ => self.line_ends[position - 1].get(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DisplayExtent, ScreenLine, ScrollExtent, WrapWidth};
    use crate::{
        geometry::{PaneShape, SplitDirection},
        page_rows::{LineIndex, PageRows, PairIndex},
        parser::parse_patch,
        preferences::Wrapping,
        screen::{Cells, NonZeroCells, ScreenLines},
    };

    const SIDE_BY_SIDE: PaneShape = PaneShape::Split(SplitDirection::SideBySide);

    fn rows_of(patch: &[u8]) -> PageRows {
        let document = parse_patch(patch).expect("valid diff");
        PageRows::of_file(&document.file_views().next().expect("first file"))
    }

    fn texts(rows: &PageRows, lines: impl Iterator<Item = LineIndex>) -> Vec<&[u8]> {
        lines
            .map(|line| rows.line(line).text().as_bytes())
            .collect()
    }

    fn single(extent: DisplayExtent) -> ScrollExtent<LineIndex> {
        let DisplayExtent::Single(extent) = extent else {
            panic!("a single pane shows lines");
        };
        extent
    }

    fn wrap_at(cells: usize) -> WrapWidth {
        WrapWidth::At(NonZeroCells::new(Cells::new(cells)).expect("a nonzero test wrap width"))
    }

    fn lines(count: usize) -> ScreenLines {
        ScreenLines::new(count)
    }

    #[test]
    fn shows_context_rows_within_the_context_count_of_a_change_in_their_hunk() {
        let view = rows_of(
            b"--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n c0\n c1\n c2\n-old\n+new\n c3\n c4\n@@ -20,2 +20,2 @@\n far0\n far1\n",
        );
        let shown = |context_lines| {
            let extent = single(DisplayExtent::of(
                &view,
                PaneShape::Single,
                context_lines,
                WrapWidth::Unbounded,
            ));
            let lines = extent
                .lines(ScreenLines::ZERO, lines(usize::MAX))
                .map(|line| line.row);
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

        let width = |shape, context_lines| {
            DisplayExtent::of(&view, shape, context_lines, WrapWidth::Unbounded).width()
        };

        assert_eq!(width(PaneShape::Single, 1), Cells::new(24));
        assert_eq!(width(SIDE_BY_SIDE, 1), Cells::new(24));
        assert_eq!(width(PaneShape::Single, 2), Cells::new(40));
    }

    #[test]
    fn maps_a_row_to_its_visible_offset_or_the_next_visible_row() {
        let view =
            rows_of(b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n");
        let extent = single(DisplayExtent::of(
            &view,
            PaneShape::Single,
            1,
            WrapWidth::Unbounded,
        ));

        // Visible rows: 0 (---), 1 (+++), 2 (@@), 5 (c2), 6 (-old), 7 (+new), 8 (c3).
        assert_eq!(extent.offset_of(LineIndex::new(5), 0), lines(3));
        assert_eq!(
            extent.offset_of(LineIndex::new(3), 0),
            lines(3),
            "hidden c0 maps to the next visible row"
        );
        assert_eq!(
            extent.offset_of(LineIndex::new(9), 0),
            lines(7),
            "past the end maps to the row count"
        );
    }

    #[test]
    fn counts_the_wrapped_lines_of_each_visible_row() {
        let view = rows_of(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-abcdefghijklmnopqrstuvwxyz0123456789abcdefghi\n+abc\n context\n",
        );
        let wrap = wrap_at(20);
        let unified = single(DisplayExtent::of(&view, PaneShape::Single, 3, wrap));
        let DisplayExtent::Split {
            extent: vertical, ..
        } = DisplayExtent::of(&view, SIDE_BY_SIDE, 3, wrap)
        else {
            panic!("split panes show pairs");
        };
        let line = |row, wrapped_line| ScreenLine {
            row: LineIndex::new(row),
            wrapped_line,
        };
        let pair = |row, wrapped_line| ScreenLine {
            row: PairIndex::new(row),
            wrapped_line,
        };

        assert_eq!(
            unified.line_count(),
            lines(8),
            "the 45-cell deletion takes three lines"
        );
        assert_eq!(
            unified.lines(lines(4), lines(3)).collect::<Vec<_>>(),
            [line(3, 1), line(3, 2), line(4, 0)]
        );
        assert_eq!(unified.offset_of(LineIndex::new(3), 1), lines(4));
        assert_eq!(
            unified.offset_of(LineIndex::new(3), 9),
            lines(5),
            "clamped to the row's last line"
        );
        assert_eq!(unified.offset_of(LineIndex::new(4), 0), lines(6));
        assert_eq!(
            unified.width,
            Cells::new(22),
            "wrapped rows never scroll horizontally"
        );
        assert_eq!(
            vertical.line_count(),
            lines(7),
            "the paired row takes its taller side's three lines"
        );
        assert_eq!(
            vertical.lines(lines(5), lines(2)).collect::<Vec<_>>(),
            [pair(3, 2), pair(4, 0)]
        );
    }

    #[test]
    fn wraps_both_side_by_side_panes_at_the_narrower_payload_width() {
        let wrap = |wrapping, width| WrapWidth::for_panes(wrapping, Cells::new(width));

        assert_eq!(wrap(Wrapping::Enabled, 24), wrap_at(22));
        assert_eq!(wrap(Wrapping::Enabled, 51), wrap_at(49));
        assert_eq!(wrap(Wrapping::Disabled, 51), WrapWidth::Unbounded);
        assert_eq!(wrap(Wrapping::Enabled, 78), wrap_at(76));
    }

    /// architecture.md (View model): an anchored row keeps its place when its layout
    /// rebuilds at another context count.
    #[test]
    fn finds_an_anchored_pair_again_in_a_rebuilt_extent() {
        let view =
            rows_of(b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n");
        let narrow = DisplayExtent::of(&view, SIDE_BY_SIDE, 0, WrapWidth::Unbounded);
        let wide = DisplayExtent::of(&view, SIDE_BY_SIDE, 3, WrapWidth::Unbounded);

        // Narrow pairs: ---, +++, @@, -old|+new. Wide adds c0, c1, and c2 before the change.
        let change = narrow
            .anchor_at(&view, lines(3))
            .expect("the change is on line 3");

        assert_eq!(wide.offset_of(&view, change), lines(6));
        assert_eq!(narrow.offset_of(&view, change), lines(3));
    }
}
