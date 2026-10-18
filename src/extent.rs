//! The rows one pane shape shows of one page at one context count and terminal-derived wrap
//! width: their screen lines, the widest row, and how a row keeps its place when they are
//! rebuilt. Rules: `docs/spec/architecture.md` (Viewport coordinates).

use crate::{
    compact::Count,
    effective_view::WrapWidth,
    geometry::{PaneShape, SplitDirection},
    line_breaks::wrapped_line_count,
    page_rows::{LineIndex, PageRows, PairIndex},
    prepared::{PageIndex, PreparedDocument},
    screen::{Cells, MARKER_GUTTER_WIDTH, ScreenLines},
};

/// What a page's rows are shown with: the pane shape, the context count, and the wrap
/// width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtentSettings {
    pub shape: PaneShape,
    pub context_lines: usize,
    pub wrap: WrapWidth,
}

/// One screen line: wrapped line `wrapped_line`, counted from the row's first, of row `row`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenLine<R> {
    pub row: R,
    pub wrapped_line: ScreenLines,
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

/// The rows of one page that one pane shape shows at one context count and wrap width,
/// their screen lines, and the widest row. A viewport's vertical offset indexes these
/// lines, and the extent's indexes reach only its page's rows. Contract:
/// `docs/spec/architecture.md` (Viewport coordinates).
#[derive(Debug, PartialEq, Eq)]
pub struct DisplayExtent {
    page: PageIndex,
    context_lines: usize,
    wrap: WrapWidth,
    rows: PaneRows,
    /// For each visible row, the line just past its last screen line.
    line_ends: Box<[Count]>,
    /// Cells of the widest visible row, including the marker gutter.
    width: Cells,
}

/// The visible rows in display order: a single pane shows lines, and split panes show
/// pairs.
#[derive(Debug, PartialEq, Eq)]
enum PaneRows {
    Single(Box<[LineIndex]>),
    Split {
        direction: SplitDirection,
        pairs: Box<[PairIndex]>,
    },
}

/// Screen lines from an offset, with their rows as the pane shape names them.
pub enum ShownLines<'a> {
    Single(Lines<'a, LineIndex>),
    Split {
        direction: SplitDirection,
        lines: Lines<'a, PairIndex>,
    },
}

impl DisplayExtent {
    /// The rows of `page` of `document` that `settings` show.
    pub fn of(document: &PreparedDocument, page: PageIndex, settings: ExtentSettings) -> Self {
        let rows = document.page(page).rows();
        let ExtentSettings {
            shape,
            context_lines,
            wrap,
        } = settings;

        let (pane_rows, ends) = match shape {
            PaneShape::Single => {
                let (lines, ends) = rows
                    .line_indices()
                    .filter(|index| rows.shows_line(*index, context_lines))
                    .map(|index| visible_row(rows, index, index, None, wrap))
                    .fold(ExtentRows::default(), ExtentRows::with)
                    .finish();
                (PaneRows::Single(lines), ends)
            }
            PaneShape::Split(direction) => {
                let (pairs, ends) = rows
                    .pair_indices()
                    .filter(|index| rows.shows_pair(*index, context_lines))
                    .map(|index| {
                        let (first, second) = rows.pair(index).split_first();
                        visible_row(rows, index, first, second, wrap)
                    })
                    .fold(ExtentRows::default(), ExtentRows::with)
                    .finish();
                (PaneRows::Split { direction, pairs }, ends)
            }
        };

        Self {
            page,
            context_lines,
            wrap,
            rows: pane_rows,
            line_ends: ends.line_ends,
            width: ends.width,
        }
    }

    /// The page these rows belong to.
    pub fn page(&self) -> PageIndex {
        self.page
    }

    /// What these rows are shown with.
    pub fn settings(&self) -> ExtentSettings {
        let shape = match &self.rows {
            PaneRows::Single(_) => PaneShape::Single,
            PaneRows::Split { direction, .. } => PaneShape::Split(*direction),
        };

        ExtentSettings {
            shape,
            context_lines: self.context_lines,
            wrap: self.wrap,
        }
    }

    pub fn line_count(&self) -> ScreenLines {
        self.line_ends
            .last()
            .map_or(ScreenLines::ZERO, |end| ScreenLines::new(end.get()))
    }

    /// Cells of the widest visible row, including the marker gutter.
    pub fn width(&self) -> Cells {
        self.width
    }

    /// The width the rows break at: the one drawing reads.
    pub fn wrap(&self) -> WrapWidth {
        self.wrap
    }

    /// Up to `limit` screen lines from line `offset`.
    pub fn lines(&self, offset: ScreenLines, limit: ScreenLines) -> ShownLines<'_> {
        match &self.rows {
            PaneRows::Single(lines) => ShownLines::Single(self.lines_of(lines, offset, limit)),
            PaneRows::Split { direction, pairs } => ShownLines::Split {
                direction: *direction,
                lines: self.lines_of(pairs, offset, limit),
            },
        }
    }

    /// Shows `page` with `settings`, rebuilding the rows unless they already are those.
    pub fn turn_to(
        &mut self,
        document: &PreparedDocument,
        page: PageIndex,
        settings: ExtentSettings,
    ) {
        if page != self.page || settings != self.settings() {
            *self = Self::of(document, page, settings);
        }
    }

    /// Shows this page with `settings`, rebuilding the rows unless they already are those.
    pub fn reshow(&mut self, document: &PreparedDocument, settings: ExtentSettings) {
        self.turn_to(document, self.page, settings);
    }

    /// Shows this page with `settings`, and returns the screen line that now shows the row
    /// on screen line `offset`; none when the lines end above `offset`.
    pub fn reanchor(
        &mut self,
        document: &PreparedDocument,
        settings: ExtentSettings,
        offset: ScreenLines,
    ) -> Option<ScreenLines> {
        let anchor = self.anchor_at(document, offset);

        self.reshow(document, settings);
        anchor.map(|anchor| self.offset_of(document, anchor))
    }

    /// The row on screen line `offset`, if the lines reach that far.
    fn anchor_at(&self, document: &PreparedDocument, offset: ScreenLines) -> Option<RowAnchor> {
        let position = self.position_at(offset);
        let line = match &self.rows {
            PaneRows::Single(lines) => *lines.get(position)?,
            PaneRows::Split { pairs, .. } => {
                let pair = *pairs.get(position)?;
                document.page(self.page).rows().pair(pair).first()
            }
        };

        Some(RowAnchor {
            line,
            wrapped_line: offset.saturating_sub(self.line_start(position)),
        })
    }

    /// The screen line that shows `anchor`'s wrapped line, clamped to its row's lines; a
    /// hidden row maps to the first line of the first visible row after it.
    fn offset_of(&self, document: &PreparedDocument, anchor: RowAnchor) -> ScreenLines {
        let (position, shown) = match &self.rows {
            PaneRows::Single(lines) => position_of(lines, anchor.line),
            PaneRows::Split { pairs, .. } => {
                let pair = document.page(self.page).rows().pair_of(anchor.line);
                position_of(pairs, pair)
            }
        };
        let start = self.line_start(position);

        match shown {
            true => {
                let last_line = self
                    .line_end(position)
                    .saturating_sub(start)
                    .saturating_sub(ScreenLines::ONE);
                start + anchor.wrapped_line.min(last_line)
            }
            false => start,
        }
    }

    fn lines_of<'a, R>(
        &'a self,
        rows: &'a [R],
        offset: ScreenLines,
        limit: ScreenLines,
    ) -> Lines<'a, R> {
        let position = self.position_at(offset);

        Lines {
            rows,
            extent: self,
            position,
            wrapped_line: offset.saturating_sub(self.line_start(position)),
            remaining: limit,
        }
    }

    /// Position of the row that screen line `offset` belongs to; the row count past the end.
    fn position_at(&self, offset: ScreenLines) -> usize {
        self.line_ends
            .partition_point(|end| ScreenLines::new(end.get()) <= offset)
    }

    fn line_start(&self, position: usize) -> ScreenLines {
        match position {
            0 => ScreenLines::ZERO,
            _ => self.line_end(position - 1),
        }
    }

    fn line_end(&self, position: usize) -> ScreenLines {
        ScreenLines::new(self.line_ends[position].get())
    }
}

/// A row of the extent's page, named by the line it starts with, and one of its wrapped
/// lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RowAnchor {
    line: LineIndex,
    wrapped_line: ScreenLines,
}

/// Screen lines of an extent's rows, in display order.
pub struct Lines<'a, R> {
    rows: &'a [R],
    extent: &'a DisplayExtent,
    /// Position of the row the next line belongs to.
    position: usize,
    wrapped_line: ScreenLines,
    remaining: ScreenLines,
}

impl<R: Copy> Iterator for Lines<'_, R> {
    type Item = ScreenLine<R>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == ScreenLines::ZERO {
            return None;
        }
        let row = *self.rows.get(self.position)?;

        let line = ScreenLine {
            row,
            wrapped_line: self.wrapped_line,
        };
        self.remaining = self.remaining.saturating_sub(ScreenLines::ONE);
        self.wrapped_line = self.wrapped_line + ScreenLines::ONE;

        let row_lines = self
            .extent
            .line_end(self.position)
            .saturating_sub(self.extent.line_start(self.position));
        if self.wrapped_line == row_lines {
            self.position += 1;
            self.wrapped_line = ScreenLines::ZERO;
        }
        Some(line)
    }
}

/// Where `index` sits among the sorted `rows`, and whether it is there; a missing row sits
/// where it would be.
fn position_of<R: Ord>(rows: &[R], index: R) -> (usize, bool) {
    let position = rows.partition_point(|row| *row < index);

    (position, rows.get(position) == Some(&index))
}

struct VisibleRow<R> {
    index: R,
    width: Cells,
    height: ScreenLines,
}

/// A row made of line `first`, and of line `second` when it pairs two: as wide as its
/// widest line and as tall as its tallest.
fn visible_row<R>(
    rows: &PageRows,
    index: R,
    first: LineIndex,
    second: Option<LineIndex>,
    wrap: WrapWidth,
) -> VisibleRow<R> {
    let measure = |line| (rows.payload_width(line), line_height(rows, line, wrap));
    let (payload_width, height) = second.map(measure).into_iter().fold(
        measure(first),
        |(width, height), (other_width, other_height)| {
            (width.max(other_width), height.max(other_height))
        },
    );

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
    let payload = rows.measured_text(index);
    if payload.cells() <= cells.get() {
        return ScreenLines::ONE;
    }

    wrapped_line_count(payload, cells)
}

/// Visible rows gathered in display order, with their running line ends and widest width.
struct ExtentRows<R> {
    rows: Vec<R>,
    line_ends: Vec<Count>,
    line_end: ScreenLines,
    width: Cells,
}

impl<R> Default for ExtentRows<R> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            line_ends: Vec::new(),
            line_end: ScreenLines::ZERO,
            width: Cells::ZERO,
        }
    }
}

impl<R> ExtentRows<R> {
    fn with(mut self, row: VisibleRow<R>) -> Self {
        self.line_end = self.line_end + row.height;
        self.rows.push(row.index);
        self.line_ends.push(Count::of(self.line_end.get()));
        self.width = self.width.max(row.width);
        self
    }

    fn finish(self) -> (Box<[R]>, LineEnds) {
        let ends = LineEnds {
            line_ends: self.line_ends.into_boxed_slice(),
            width: self.width,
        };

        (self.rows.into_boxed_slice(), ends)
    }
}

/// The finished line ends of an extent's rows, and its widest row.
struct LineEnds {
    line_ends: Box<[Count]>,
    width: Cells,
}

#[cfg(test)]
mod tests {
    use super::{DisplayExtent, ExtentSettings, RowAnchor, ScreenLine, ShownLines};
    use crate::{
        effective_view::WrapWidth,
        geometry::{PaneShape, SplitDirection},
        page_rows::{LineIndex, PairIndex},
        parser::parse_patch,
        prepared::{PageIndex, PreparedDocument},
        screen::{Cells, NonZeroCells, ScreenLines},
    };

    const VERTICAL: PaneShape = PaneShape::Split(SplitDirection::Vertical);

    fn prepared(patch: &[u8]) -> PreparedDocument {
        PreparedDocument::prepare(&parse_patch(patch).expect("valid diff"))
    }

    fn extent_of(
        document: &PreparedDocument,
        shape: PaneShape,
        context_lines: usize,
        wrap: WrapWidth,
    ) -> DisplayExtent {
        let settings = ExtentSettings {
            shape,
            context_lines,
            wrap,
        };

        DisplayExtent::of(document, PageIndex::FIRST, settings)
    }

    /// Each line as a reader sees it: its record marker, its payload, then a newline.
    fn texts(document: &PreparedDocument, lines: impl Iterator<Item = LineIndex>) -> Vec<String> {
        let rows = document.page(PageIndex::FIRST).rows();

        lines
            .map(|line| rows.line(line))
            .map(|line| format!("{}{}\n", line.kind().marker(), line.payload()))
            .collect()
    }

    /// Up to `limit` screen lines of a single-pane extent from line `offset`.
    fn single_lines(
        extent: &DisplayExtent,
        offset: usize,
        limit: usize,
    ) -> Vec<ScreenLine<LineIndex>> {
        let ShownLines::Single(lines) = extent.lines(lines(offset), lines(limit)) else {
            panic!("a single pane shows lines");
        };
        lines.collect()
    }

    /// Up to `limit` screen lines of a split-pane extent from line `offset`.
    fn pair_lines(
        extent: &DisplayExtent,
        offset: usize,
        limit: usize,
    ) -> Vec<ScreenLine<PairIndex>> {
        let ShownLines::Split { lines, .. } = extent.lines(lines(offset), lines(limit)) else {
            panic!("split panes show pairs");
        };
        lines.collect()
    }

    /// The screen line that shows wrapped line `wrapped_line` of the row starting with line
    /// `line`.
    fn offset_of(
        document: &PreparedDocument,
        extent: &DisplayExtent,
        line: usize,
        wrapped_line: usize,
    ) -> ScreenLines {
        let anchor = RowAnchor {
            line: LineIndex::new(line),
            wrapped_line: lines(wrapped_line),
        };

        extent.offset_of(document, anchor)
    }

    fn wrap_at(cells: usize) -> WrapWidth {
        WrapWidth::At(NonZeroCells::new(Cells::new(cells)).expect("a nonzero test wrap width"))
    }

    fn lines(count: usize) -> ScreenLines {
        ScreenLines::new(count)
    }

    #[test]
    fn shows_context_rows_within_the_context_count_of_a_change_in_their_hunk() {
        let document = prepared(
            b"--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n c0\n c1\n c2\n-old\n+new\n c3\n c4\n@@ -20,2 +20,2 @@\n far0\n far1\n",
        );
        let shown = |context_lines| {
            let extent = extent_of(
                &document,
                PaneShape::Single,
                context_lines,
                WrapWidth::Unbounded,
            );
            let lines = single_lines(&extent, 0, usize::MAX)
                .into_iter()
                .map(|line| line.row);
            texts(&document, lines).concat()
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
        let document = prepared(
            "--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n far context that is hidden at one line\n near\n-old\n+\t界界界界界界界界界界\n"
                .as_bytes(),
        );

        let width = |shape, context_lines| {
            extent_of(&document, shape, context_lines, WrapWidth::Unbounded).width()
        };

        assert_eq!(width(PaneShape::Single, 1), Cells::new(24));
        assert_eq!(width(VERTICAL, 1), Cells::new(24));
        assert_eq!(width(PaneShape::Single, 2), Cells::new(40));
    }

    #[test]
    fn maps_a_row_to_its_visible_offset_or_the_next_visible_row() {
        let document =
            prepared(b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n");
        let extent = extent_of(&document, PaneShape::Single, 1, WrapWidth::Unbounded);
        let offset = |line| offset_of(&document, &extent, line, 0);

        // Visible rows: 0 (---), 1 (+++), 2 (@@), 5 (c2), 6 (-old), 7 (+new), 8 (c3).
        assert_eq!(offset(5), lines(3));
        assert_eq!(
            offset(3),
            lines(3),
            "hidden c0 maps to the next visible row"
        );
        assert_eq!(offset(9), lines(7), "past the end maps to the row count");
    }

    #[test]
    fn counts_the_wrapped_lines_of_each_visible_row() {
        let document = prepared(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-abcdefghijklmnopqrstuvwxyz0123456789abcdefghi\n+abc\n context\n",
        );
        let wrap = wrap_at(20);
        let unified = extent_of(&document, PaneShape::Single, 3, wrap);
        let vertical = extent_of(&document, VERTICAL, 3, wrap);
        let line = |row, wrapped_line| ScreenLine {
            row: LineIndex::new(row),
            wrapped_line: lines(wrapped_line),
        };
        let pair = |row, wrapped_line| ScreenLine {
            row: PairIndex::new(row),
            wrapped_line: lines(wrapped_line),
        };

        assert_eq!(
            unified.line_count(),
            lines(8),
            "the 45-cell deletion takes three lines"
        );
        assert_eq!(
            single_lines(&unified, 4, 3),
            [line(3, 1), line(3, 2), line(4, 0)]
        );
        assert_eq!(offset_of(&document, &unified, 3, 1), lines(4));
        assert_eq!(
            offset_of(&document, &unified, 3, 9),
            lines(5),
            "clamped to the row's last line"
        );
        assert_eq!(offset_of(&document, &unified, 4, 0), lines(6));
        assert_eq!(
            unified.width(),
            Cells::new(22),
            "wrapped rows never scroll horizontally"
        );
        assert_eq!(
            vertical.line_count(),
            lines(7),
            "the paired row takes its taller side's three lines"
        );
        assert_eq!(pair_lines(&vertical, 5, 2), [pair(3, 2), pair(4, 0)]);
    }

    /// architecture.md (View model): an anchored row keeps its place when its layout
    /// rebuilds at another context count.
    #[test]
    fn finds_an_anchored_pair_again_in_a_rebuilt_extent() {
        let document =
            prepared(b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n");
        let mut extent = extent_of(&document, VERTICAL, 0, WrapWidth::Unbounded);
        let narrow = extent.settings();
        let wide = ExtentSettings {
            context_lines: 3,
            ..narrow
        };

        // Narrow pairs: ---, +++, @@, -old|+new. Wide adds c0, c1, and c2 before the change.
        let widened = extent.reanchor(&document, wide, lines(3));
        assert_eq!(extent.settings(), wide, "the extent shows the new settings");
        let narrowed = extent.reanchor(&document, narrow, lines(6));

        assert_eq!(widened, Some(lines(6)));
        assert_eq!(narrowed, Some(lines(3)));
    }
}
