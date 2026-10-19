//! The rows one pane shape shows of one page at one context count and terminal-derived
//! wrap width: their screen lines, the widest row, and how a row keeps its place when
//! they are rebuilt. Rules: docs/spec/architecture.md#viewport-coordinates.

use crate::{
    compact::Count,
    effective_view::WrapWidth,
    geometry::{PaneShape, SplitDirection},
    line_breaks::wrapped_line_count,
    page_rows::{LineIndex, PageRows, PairIndex},
    preferences::ContextRecords,
    prepared::{PageIndex, PreparedDocument},
    screen::{Cells, MARKER_GUTTER_WIDTH, ScreenLines},
};

/// What a page's rows are shown with: the pane shape, the context count, and the wrap
/// width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtentSettings {
    pub shape: PaneShape,
    pub context: ContextRecords,
    pub wrap: WrapWidth,
}

/// One screen line: wrapped line `wrapped_line`, counted from the row's first, of row `row`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowLine<R> {
    pub row: R,
    pub wrapped_line: ScreenLines,
}

impl<R> RowLine<R> {
    /// The same screen line of the row `row` becomes.
    pub fn map<T>(self, row: impl FnOnce(R) -> T) -> RowLine<T> {
        RowLine {
            row: row(self.row),
            wrapped_line: self.wrapped_line,
        }
    }
}

/// The rows of one page that one pane shape shows at one context count and wrap width,
/// their screen lines, and the widest row. A viewport's vertical offset indexes these
/// lines, and the extent's indexes reach only its page's rows. Contract:
/// docs/spec/architecture.md#viewport-coordinates.
#[derive(Debug, PartialEq, Eq)]
pub struct DisplayExtent {
    page: PageIndex,
    context: ContextRecords,
    wrap: WrapWidth,
    rows: VisibleRows,
    /// For each visible row, the line just past its last screen line.
    line_ends: Box<[Count]>,
    /// Cells of the widest visible row, including the marker gutter.
    width: Cells,
}

/// The visible rows in display order: a single pane shows lines, and split panes show
/// pairs.
#[derive(Debug, PartialEq, Eq)]
enum VisibleRows {
    Single(Box<[LineIndex]>),
    Split {
        direction: SplitDirection,
        pairs: Box<[PairIndex]>,
    },
}

/// Screen lines from an offset, with their rows as the pane shape names them.
pub enum ShownLines<'a> {
    Single(RowLines<'a, LineIndex>),
    Split {
        direction: SplitDirection,
        lines: RowLines<'a, PairIndex>,
    },
}

impl DisplayExtent {
    /// The rows of `page` of `document` that `settings` show.
    pub fn of(document: &PreparedDocument, page: PageIndex, settings: ExtentSettings) -> Self {
        let rows = document.page(page).rows();
        let ExtentSettings {
            shape,
            context,
            wrap,
        } = settings;

        let measured = match shape {
            PaneShape::Single => rows
                .line_indices()
                .filter(|index| rows.shows_line(*index, context))
                .map(|index| measured_row(rows, index, index, None, wrap))
                .fold(ExtentRows::default(), ExtentRows::with)
                .finish()
                .map_rows(VisibleRows::Single),
            PaneShape::Split(direction) => rows
                .pair_indices()
                .filter(|index| rows.shows_pair(*index, context))
                .map(|index| {
                    let (first, second) = rows.pair(index).split_first();
                    measured_row(rows, index, first, second, wrap)
                })
                .fold(ExtentRows::default(), ExtentRows::with)
                .finish()
                .map_rows(|pairs| VisibleRows::Split { direction, pairs }),
        };
        let MeasuredRows {
            rows: visible_rows,
            line_ends,
            width,
        } = measured;

        Self {
            page,
            context,
            wrap,
            rows: visible_rows,
            line_ends,
            width,
        }
    }

    /// The page these rows belong to.
    pub fn page(&self) -> PageIndex {
        self.page
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
            VisibleRows::Single(lines) => ShownLines::Single(self.lines_of(lines, offset, limit)),
            VisibleRows::Split { direction, pairs } => ShownLines::Split {
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

    /// What these rows are shown with.
    fn settings(&self) -> ExtentSettings {
        let shape = match &self.rows {
            VisibleRows::Single(_) => PaneShape::Single,
            VisibleRows::Split { direction, .. } => PaneShape::Split(*direction),
        };

        ExtentSettings {
            shape,
            context: self.context,
            wrap: self.wrap,
        }
    }

    /// The screen line `offset`, as a wrapped line of the row that starts with that line, if
    /// the lines reach that far.
    fn anchor_at(
        &self,
        document: &PreparedDocument,
        offset: ScreenLines,
    ) -> Option<RowLine<LineIndex>> {
        let position = self.position_at(offset);
        let line = match &self.rows {
            VisibleRows::Single(lines) => *lines.get(position)?,
            VisibleRows::Split { pairs, .. } => {
                let pair = *pairs.get(position)?;
                document.page(self.page).rows().pair(pair).first()
            }
        };

        Some(RowLine {
            row: line,
            wrapped_line: offset.saturating_sub(self.line_start(position)),
        })
    }

    /// The screen line that shows `anchor`'s wrapped line, clamped to its row's lines; a
    /// hidden row maps to the first line of the first visible row after it.
    fn offset_of(&self, document: &PreparedDocument, anchor: RowLine<LineIndex>) -> ScreenLines {
        let found = match &self.rows {
            VisibleRows::Single(lines) => lines.binary_search(&anchor.row),
            VisibleRows::Split { pairs, .. } => {
                let pair = document.page(self.page).rows().pair_of(anchor.row);
                pairs.binary_search(&pair)
            }
        };

        match found {
            Ok(position) => {
                let start = self.line_start(position);
                let last_line = self
                    .line_end(position)
                    .saturating_sub(start)
                    .saturating_sub(ScreenLines::ONE);
                start + anchor.wrapped_line.min(last_line)
            }
            Err(next_position) => self.line_start(next_position),
        }
    }

    fn lines_of<'a, R>(
        &'a self,
        rows: &'a [R],
        offset: ScreenLines,
        limit: ScreenLines,
    ) -> RowLines<'a, R> {
        let position = self.position_at(offset);

        RowLines {
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

/// Screen lines of an extent's rows, in display order.
pub struct RowLines<'a, R> {
    rows: &'a [R],
    extent: &'a DisplayExtent,
    /// Position of the row the next line belongs to.
    position: usize,
    wrapped_line: ScreenLines,
    remaining: ScreenLines,
}

impl<R: Copy> Iterator for RowLines<'_, R> {
    type Item = RowLine<R>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == ScreenLines::ZERO {
            return None;
        }
        let row = *self.rows.get(self.position)?;

        let line = RowLine {
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

/// One visible row, and the cells and screen lines it takes.
struct MeasuredRow<R> {
    index: R,
    width: Cells,
    height: ScreenLines,
}

/// A row made of line `first`, and of line `second` when it pairs two: as wide as its
/// widest line and as tall as its tallest.
fn measured_row<R>(
    rows: &PageRows,
    index: R,
    first: LineIndex,
    second: Option<LineIndex>,
    wrap: WrapWidth,
) -> MeasuredRow<R> {
    let measure = |line| (rows.payload_width(line), line_height(rows, line, wrap));
    let (first_width, first_height) = measure(first);
    let (payload_width, height) = match second {
        None => (first_width, first_height),
        Some(line) => {
            let (second_width, second_height) = measure(line);
            (
                first_width.max(second_width),
                first_height.max(second_height),
            )
        }
    };

    MeasuredRow {
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
    fn with(mut self, row: MeasuredRow<R>) -> Self {
        self.line_end = self.line_end + row.height;
        self.rows.push(row.index);
        self.line_ends.push(Count::of(self.line_end.get()));
        self.width = self.width.max(row.width);
        self
    }

    fn finish(self) -> MeasuredRows<Box<[R]>> {
        MeasuredRows {
            rows: self.rows.into_boxed_slice(),
            line_ends: self.line_ends.into_boxed_slice(),
            width: self.width,
        }
    }
}

/// An extent's finished rows, the line just past each row's last screen line, and the
/// cells of the widest row.
struct MeasuredRows<T> {
    rows: T,
    line_ends: Box<[Count]>,
    width: Cells,
}

impl<T> MeasuredRows<T> {
    fn map_rows<U>(self, rows: impl FnOnce(T) -> U) -> MeasuredRows<U> {
        MeasuredRows {
            rows: rows(self.rows),
            line_ends: self.line_ends,
            width: self.width,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DisplayExtent, ExtentSettings, RowLine, ShownLines};
    use crate::{
        effective_view::WrapWidth,
        geometry::{PaneShape, SplitDirection},
        page_rows::{LineIndex, PageRows, PairIndex},
        parser::parse_patch,
        preferences::ContextRecords,
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
        context: usize,
        wrap: WrapWidth,
    ) -> DisplayExtent {
        let settings = ExtentSettings {
            shape,
            context: ContextRecords::new(context),
            wrap,
        };

        DisplayExtent::of(document, PageIndex::FIRST, settings)
    }

    fn first_rows(document: &PreparedDocument) -> &PageRows {
        document.page(PageIndex::FIRST).rows()
    }

    /// Each line as a reader sees it: its record marker, its payload, then a newline.
    fn texts(document: &PreparedDocument, lines: impl Iterator<Item = LineIndex>) -> Vec<String> {
        let rows = first_rows(document);

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
    ) -> Vec<RowLine<LineIndex>> {
        let ShownLines::Single(lines) = extent.lines(screen_lines(offset), screen_lines(limit))
        else {
            panic!("a single pane shows lines");
        };
        lines.collect()
    }

    /// Up to `limit` screen lines of a split-pane extent from line `offset`.
    fn pair_lines(extent: &DisplayExtent, offset: usize, limit: usize) -> Vec<RowLine<PairIndex>> {
        let ShownLines::Split { lines, .. } =
            extent.lines(screen_lines(offset), screen_lines(limit))
        else {
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
        let anchor = RowLine {
            row: first_rows(document).line_index(line),
            wrapped_line: screen_lines(wrapped_line),
        };

        extent.offset_of(document, anchor)
    }

    fn wrap_at(cells: usize) -> WrapWidth {
        WrapWidth::At(NonZeroCells::new(Cells::new(cells)).expect("a nonzero test wrap width"))
    }

    fn screen_lines(count: usize) -> ScreenLines {
        ScreenLines::new(count)
    }

    #[test]
    fn shows_context_rows_within_the_context_count_of_a_change_in_their_hunk() {
        let document = prepared(
            b"--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n c0\n c1\n c2\n-old\n+new\n c3\n c4\n@@ -20,2 +20,2 @@\n far0\n far1\n",
        );
        let shown = |context| {
            let extent = extent_of(&document, PaneShape::Single, context, WrapWidth::Unbounded);
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

        let width =
            |shape, context| extent_of(&document, shape, context, WrapWidth::Unbounded).width();

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
        assert_eq!(offset(5), screen_lines(3));
        assert_eq!(
            offset(3),
            screen_lines(3),
            "hidden c0 maps to the next visible row"
        );
    }

    #[test]
    fn counts_the_wrapped_lines_of_each_visible_row() {
        let document = prepared(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-abcdefghijklmnopqrstuvwxyz0123456789abcdefghi\n+abc\n context\n",
        );
        let wrap = wrap_at(20);
        let unified = extent_of(&document, PaneShape::Single, 3, wrap);
        let vertical = extent_of(&document, VERTICAL, 3, wrap);
        let rows = first_rows(&document);
        let line = |row, wrapped_line| RowLine {
            row: rows.line_index(row),
            wrapped_line: screen_lines(wrapped_line),
        };
        let pair = |row, wrapped_line| RowLine {
            row: rows.pair_index(row),
            wrapped_line: screen_lines(wrapped_line),
        };

        assert_eq!(
            unified.line_count(),
            screen_lines(8),
            "the 45-cell deletion takes three lines"
        );
        assert_eq!(
            single_lines(&unified, 4, 3),
            [line(3, 1), line(3, 2), line(4, 0)]
        );
        assert_eq!(offset_of(&document, &unified, 3, 1), screen_lines(4));
        assert_eq!(
            offset_of(&document, &unified, 3, 9),
            screen_lines(5),
            "clamped to the row's last line"
        );
        assert_eq!(offset_of(&document, &unified, 4, 0), screen_lines(6));
        assert_eq!(
            unified.width(),
            Cells::new(22),
            "wrapped rows never scroll horizontally"
        );
        assert_eq!(
            vertical.line_count(),
            screen_lines(7),
            "the paired row takes its taller side's three lines"
        );
        assert_eq!(pair_lines(&vertical, 5, 2), [pair(3, 2), pair(4, 0)]);
    }

    /// docs/spec/architecture.md#view-model: an anchored row keeps its place when its
    /// layout rebuilds at another context count.
    #[test]
    fn finds_an_anchored_pair_again_in_a_rebuilt_extent() {
        let document =
            prepared(b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n");
        let mut extent = extent_of(&document, VERTICAL, 0, WrapWidth::Unbounded);
        let narrow = extent.settings();
        let wide = ExtentSettings {
            context: ContextRecords::new(3),
            ..narrow
        };

        // Narrow pairs: ---, +++, @@, -old|+new. Wide adds c0, c1, and c2 before the change.
        let widened = extent.reanchor(&document, wide, screen_lines(3));
        assert_eq!(extent.settings(), wide, "the extent shows the new settings");
        let narrowed = extent.reanchor(&document, narrow, screen_lines(6));

        assert_eq!(widened, Some(screen_lines(6)));
        assert_eq!(narrowed, Some(screen_lines(3)));
    }
}
