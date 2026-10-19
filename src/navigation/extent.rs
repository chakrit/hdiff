//! The rows one pane shape shows of one page at one context count and terminal-derived
//! wrap width: their screen lines, the widest row, and how a row keeps its place when
//! they are rebuilt. Rules: docs/spec/architecture.md#viewport-coordinates.

use crate::{
    compact::Count,
    effective_view::WrapWidth,
    geometry::{PaneShape, SplitDirection},
    line_breaks::wrapped_line_count,
    page_rows::{LineIndex, PageRows, PairIndex, SideBySideRow},
    preferences::{ContextRecords, DiffGranularity},
    prepared::{PageIndex, PreparedDocument, PreparedPage},
    screen::{Cells, MARKER_GUTTER_WIDTH, ScreenLines},
    side::BySide,
    styling::StyledRow,
};

/// What a page's rows are shown with: the pane shape, the context count, and the wrap
/// width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtentSettings {
    pub shape: PaneShape,
    pub context: ContextRecords,
    pub wrap: WrapWidth,
}

/// One screen line: wrapped line `wrapped_line`, counted from the row's first, of row
/// `row`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowLine<R> {
    pub row: R,
    pub wrapped_line: ScreenLines,
}

impl<R> RowLine<R> {
    /// The same screen line of the row `convert` makes of this one.
    pub fn map<T>(self, convert: impl FnOnce(R) -> T) -> RowLine<T> {
        RowLine {
            row: convert(self.row),
            wrapped_line: self.wrapped_line,
        }
    }
}

impl<T> RowLine<BySide<T>> {
    /// The same screen line of each side's row.
    pub fn split(self) -> BySide<RowLine<T>> {
        let wrapped_line = self.wrapped_line;

        self.row.map(|row| RowLine { row, wrapped_line })
    }
}

/// The rows of one page that one pane shape shows at one context count and wrap width,
/// their screen lines, and the widest row. A viewport's vertical offset indexes these
/// lines; the extent resolves its rows on its own page, so its indexes reach no other.
#[derive(Debug)]
pub struct ScrollExtent {
    page: PageIndex,
    context: ContextRecords,
    wrap: WrapWidth,
    rows: VisibleRows,
}

/// Screen lines from an offset, with the page's rows as the pane shape names them.
pub enum ShownLines<'a> {
    Single(PageLines<'a, LineIndex>),
    Split {
        direction: SplitDirection,
        lines: PageLines<'a, PairIndex>,
    },
}

/// Screen lines of an extent's rows, resolved on its page with one granularity's styles.
pub struct PageLines<'a, R> {
    page: &'a PreparedPage,
    granularity: DiffGranularity,
    lines: RowLines<'a, R>,
}

impl ScrollExtent {
    /// The rows of `page` of `document` that `settings` show.
    pub(super) fn of(
        document: &PreparedDocument,
        page: PageIndex,
        settings: ExtentSettings,
    ) -> Self {
        let rows = document.page(page).rows();
        let ExtentSettings {
            shape,
            context,
            wrap,
        } = settings;

        let visible = match shape {
            PaneShape::Single => VisibleRows::Single(
                rows.line_indices()
                    .filter(|index| rows.shows_line(*index, context))
                    .map(|index| measured_row(rows, index, index, None, wrap))
                    .collect(),
            ),
            PaneShape::Split(direction) => VisibleRows::Split {
                direction,
                pairs: rows
                    .pair_indices()
                    .filter(|index| rows.shows_pair(*index, context))
                    .map(|index| {
                        let (first, second) = rows.pair(index).split_first();
                        measured_row(rows, index, first, second, wrap)
                    })
                    .collect(),
            },
        };

        Self {
            page,
            context,
            wrap,
            rows: visible,
        }
    }

    pub fn line_count(&self) -> ScreenLines {
        match &self.rows {
            VisibleRows::Single(lines) => lines.line_count(),
            VisibleRows::Split { pairs, .. } => pairs.line_count(),
        }
    }

    /// Cells of the widest visible row, including the marker gutter.
    pub fn width(&self) -> Cells {
        match &self.rows {
            VisibleRows::Single(lines) => lines.width,
            VisibleRows::Split { pairs, .. } => pairs.width,
        }
    }

    /// The width the rows break at: the one drawing reads.
    pub fn wrap(&self) -> WrapWidth {
        self.wrap
    }

    /// How far these rows scroll horizontally in panes `content_width` cells wide, at the
    /// wrap width they were measured at.
    pub fn horizontal_range(&self, content_width: Cells) -> Cells {
        self.wrap.horizontal_range(self.width(), content_width)
    }

    /// Up to `limit` screen lines from line `offset`, with their rows from this extent's
    /// page of `document` in `granularity`'s styles.
    pub(super) fn shown<'a>(
        &'a self,
        document: &'a PreparedDocument,
        granularity: DiffGranularity,
        offset: ScreenLines,
        limit: ScreenLines,
    ) -> ShownLines<'a> {
        let page = document.page(self.page);

        match &self.rows {
            VisibleRows::Single(lines) => ShownLines::Single(PageLines {
                page,
                granularity,
                lines: lines.lines_from(offset, limit),
            }),
            VisibleRows::Split { direction, pairs } => ShownLines::Split {
                direction: *direction,
                lines: PageLines {
                    page,
                    granularity,
                    lines: pairs.lines_from(offset, limit),
                },
            },
        }
    }

    /// Shows `page` with these settings, rebuilding the rows unless it already is shown.
    pub(super) fn turn_to(&mut self, document: &PreparedDocument, page: PageIndex) {
        if page != self.page {
            *self = Self::of(document, page, self.settings());
        }
    }

    /// Shows this page with `settings`, rebuilding the rows unless they already are
    /// those.
    pub(super) fn reshow(&mut self, document: &PreparedDocument, settings: ExtentSettings) {
        if settings != self.settings() {
            *self = Self::of(document, self.page, settings);
        }
    }

    /// Shows this page with `settings`, and returns the screen line that now shows the
    /// row on screen line `offset`; none when the lines end above `offset`.
    pub(super) fn reanchor(
        &mut self,
        document: &PreparedDocument,
        settings: ExtentSettings,
        offset: ScreenLines,
    ) -> Option<ScreenLines> {
        let anchor = self.anchor_at(document, offset);

        self.reshow(document, settings);
        anchor.map(|anchor| self.offset_of(document, anchor))
    }

    /// What these rows are shown with. The rows' own shape holds the pane shape, so the
    /// split direction has one home.
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

    /// Screen line `offset` as a wrapped line of the row that holds it, named by the
    /// row's first line; none past the last line.
    fn anchor_at(
        &self,
        document: &PreparedDocument,
        offset: ScreenLines,
    ) -> Option<RowLine<LineIndex>> {
        match &self.rows {
            VisibleRows::Single(lines) => lines.line_at(offset),
            VisibleRows::Split { pairs, .. } => {
                let rows = document.page(self.page).rows();
                pairs
                    .line_at(offset)
                    .map(|line| line.map(|pair| rows.pair(pair).first()))
            }
        }
    }

    /// The screen line that shows `anchor`'s wrapped line, clamped to its row's lines; a
    /// hidden row maps to the first line of the first visible row after it.
    fn offset_of(&self, document: &PreparedDocument, anchor: RowLine<LineIndex>) -> ScreenLines {
        match &self.rows {
            VisibleRows::Single(lines) => lines.offset_of(anchor),
            VisibleRows::Split { pairs, .. } => {
                let rows = document.page(self.page).rows();
                pairs.offset_of(anchor.map(|line| rows.pair_of(line)))
            }
        }
    }
}

impl<'a> Iterator for PageLines<'a, LineIndex> {
    type Item = RowLine<StyledRow<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        let (page, granularity) = (self.page, self.granularity);

        self.lines
            .next()
            .map(|line| line.map(|row| page.row(row, granularity)))
    }
}

impl<'a> Iterator for PageLines<'a, PairIndex> {
    type Item = RowLine<SideBySideRow<StyledRow<'a>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let (page, granularity) = (self.page, self.granularity);

        self.lines.next().map(|line| {
            line.map(|pair| page.rows().pair(pair).map(|row| page.row(row, granularity)))
        })
    }
}

/// The visible rows: a single pane shows lines, and split panes show pairs.
#[derive(Debug)]
enum VisibleRows {
    Single(MeasuredRows<LineIndex>),
    Split {
        direction: SplitDirection,
        pairs: MeasuredRows<PairIndex>,
    },
}

/// Visible rows in display order, each with the line just past its last screen line, and
/// the cells of the widest row. A row and its line end are held together, so no row lacks
/// one.
#[derive(Debug)]
struct MeasuredRows<R> {
    rows: Box<[PlacedRow<R>]>,
    width: Cells,
}

/// A visible row and the screen line just past its last one.
#[derive(Debug)]
struct PlacedRow<R> {
    row: R,
    line_end: LineEnd,
}

impl<R: Copy + Ord> MeasuredRows<R> {
    fn line_count(&self) -> ScreenLines {
        self.rows
            .last()
            .map_or(ScreenLines::ZERO, |placed| placed.line_end.get())
    }

    /// Up to `limit` screen lines from line `offset`.
    fn lines_from(&self, offset: ScreenLines, limit: ScreenLines) -> RowLines<'_, R> {
        let position = self.position_at(offset);

        RowLines {
            measured: self,
            position,
            wrapped_line: offset.saturating_sub(self.line_start(position)),
            remaining: limit,
        }
    }

    /// Screen line `offset` as a wrapped line of the row that holds it; none past the
    /// last line.
    fn line_at(&self, offset: ScreenLines) -> Option<RowLine<R>> {
        let position = self.position_at(offset);
        let placed = self.rows.get(position)?;

        Some(RowLine {
            row: placed.row,
            wrapped_line: offset.saturating_sub(self.line_start(position)),
        })
    }

    /// The screen line that shows `anchor`'s wrapped line, clamped to its row's lines; a
    /// row not shown maps to the first line of the first shown row after it.
    fn offset_of(&self, anchor: RowLine<R>) -> ScreenLines {
        match self
            .rows
            .binary_search_by(|placed| placed.row.cmp(&anchor.row))
        {
            Ok(position) => {
                let start = self.line_start(position);
                let last_line = self.row_lines(position).saturating_sub(ScreenLines::ONE);
                start + anchor.wrapped_line.min(last_line)
            }
            Err(next_position) => self.line_start(next_position),
        }
    }

    /// Position of the row that screen line `offset` belongs to; the row count past the
    /// end.
    fn position_at(&self, offset: ScreenLines) -> usize {
        self.rows
            .partition_point(|placed| placed.line_end.get() <= offset)
    }

    fn line_start(&self, position: usize) -> ScreenLines {
        match position {
            0 => ScreenLines::ZERO,
            _ => self.rows[position - 1].line_end.get(),
        }
    }

    /// Screen lines the row at `position` takes.
    fn row_lines(&self, position: usize) -> ScreenLines {
        self.rows[position]
            .line_end
            .get()
            .saturating_sub(self.line_start(position))
    }
}

impl<R> FromIterator<MeasuredRow<R>> for MeasuredRows<R> {
    /// Places each row below the last, and keeps the widest width.
    fn from_iter<I: IntoIterator<Item = MeasuredRow<R>>>(measured: I) -> Self {
        let (rows, width) = measured.into_iter().fold(
            (Vec::new(), Cells::ZERO),
            |(mut rows, width): (Vec<PlacedRow<R>>, Cells), row| {
                let line_start = rows
                    .last()
                    .map_or(ScreenLines::ZERO, |placed| placed.line_end.get());
                rows.push(PlacedRow {
                    row: row.row,
                    line_end: LineEnd::of(line_start + row.height),
                });
                (rows, width.max(row.width))
            },
        );

        Self {
            rows: rows.into_boxed_slice(),
            width,
        }
    }
}

/// The screen line just past a visible row's last one, retained as compactly as a line
/// record's counts.
#[derive(Clone, Copy, Debug)]
struct LineEnd(Count);

impl LineEnd {
    fn of(lines: ScreenLines) -> Self {
        Self(Count::of(lines.get()))
    }

    fn get(self) -> ScreenLines {
        ScreenLines::new(self.0.get())
    }
}

/// Screen lines of an extent's rows, in display order.
struct RowLines<'a, R> {
    measured: &'a MeasuredRows<R>,
    /// Position of the row the next line belongs to.
    position: usize,
    wrapped_line: ScreenLines,
    remaining: ScreenLines,
}

impl<R: Copy + Ord> Iterator for RowLines<'_, R> {
    type Item = RowLine<R>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == ScreenLines::ZERO {
            return None;
        }
        let placed = self.measured.rows.get(self.position)?;

        let line = RowLine {
            row: placed.row,
            wrapped_line: self.wrapped_line,
        };
        self.remaining = self.remaining.saturating_sub(ScreenLines::ONE);
        self.wrapped_line += ScreenLines::ONE;

        if self.wrapped_line == self.measured.row_lines(self.position) {
            self.position += 1;
            self.wrapped_line = ScreenLines::ZERO;
        }
        Some(line)
    }
}

/// One visible row, and the cells and screen lines it takes.
struct MeasuredRow<R> {
    row: R,
    width: Cells,
    height: ScreenLines,
}

/// A row made of line `first`, and of line `second` when it pairs two: as wide as its
/// widest line and as tall as its tallest.
fn measured_row<R>(
    rows: &PageRows,
    row: R,
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
        row,
        width: payload_width + MARKER_GUTTER_WIDTH,
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

#[cfg(test)]
mod tests {
    use super::{ExtentSettings, RowLine, ScrollExtent, VisibleRows};
    use crate::{
        effective_view::WrapWidth,
        geometry::{PaneShape, SplitDirection},
        page_rows::{LineIndex, PageRows, PairIndex},
        parser::parse_patch,
        preferences::ContextRecords,
        prepared::{PageIndex, PreparedDocument},
        screen::{Cells, ScreenLines},
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
    ) -> ScrollExtent {
        let settings = ExtentSettings {
            shape,
            context: ContextRecords::new(context),
            wrap,
        };

        ScrollExtent::of(document, PageIndex::FIRST, settings)
    }

    fn first_rows(document: &PreparedDocument) -> &PageRows {
        document.page(PageIndex::FIRST).rows()
    }

    /// Each line as a reader sees it: its record marker, its payload, then a newline.
    fn texts(document: &PreparedDocument, lines: impl Iterator<Item = LineIndex>) -> Vec<String> {
        let rows = first_rows(document);

        lines
            .map(|index| rows.line(index))
            .map(|line| format!("{}{}\n", line.kind().marker(), line.payload()))
            .collect()
    }

    /// Up to `limit` screen lines of a single-pane extent from line `offset`.
    fn single_lines(extent: &ScrollExtent, offset: usize, limit: usize) -> Vec<RowLine<LineIndex>> {
        let VisibleRows::Single(lines) = &extent.rows else {
            panic!("a single pane shows lines");
        };
        lines
            .lines_from(ScreenLines::new(offset), ScreenLines::new(limit))
            .collect()
    }

    /// Up to `limit` screen lines of a split-pane extent from line `offset`.
    fn pair_lines(extent: &ScrollExtent, offset: usize, limit: usize) -> Vec<RowLine<PairIndex>> {
        let VisibleRows::Split { pairs, .. } = &extent.rows else {
            panic!("split panes show pairs");
        };
        pairs
            .lines_from(ScreenLines::new(offset), ScreenLines::new(limit))
            .collect()
    }

    /// The screen line that shows wrapped line `wrapped_line` of the row starting with
    /// line `line`.
    fn anchor_offset(
        document: &PreparedDocument,
        extent: &ScrollExtent,
        line: usize,
        wrapped_line: usize,
    ) -> ScreenLines {
        let anchor = RowLine {
            row: first_rows(document).line_index(line),
            wrapped_line: ScreenLines::new(wrapped_line),
        };

        extent.offset_of(document, anchor)
    }

    #[test]
    fn shows_context_rows_within_the_context_count_of_a_change_in_their_hunk() {
        let document = prepared(
            b"--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n c0\n c1\n c2\n-old\n+new\n c3\n c4\n@@ -20,2 +20,2 @@\n far0\n far1\n",
        );
        let shown = |context| {
            let extent = extent_of(&document, PaneShape::Single, context, WrapWidth::Off);
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

        let width = |shape, context| extent_of(&document, shape, context, WrapWidth::Off).width();

        assert_eq!(width(PaneShape::Single, 1), Cells::new(24));
        assert_eq!(width(VERTICAL, 1), Cells::new(24));
        assert_eq!(width(PaneShape::Single, 2), Cells::new(40));
    }

    #[test]
    fn maps_a_row_to_its_visible_offset_or_the_next_visible_row() {
        let document =
            prepared(b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n");
        let extent = extent_of(&document, PaneShape::Single, 1, WrapWidth::Off);
        let offset = |line| anchor_offset(&document, &extent, line, 0);

        // Visible rows: 0 (---), 1 (+++), 2 (@@), 5 (c2), 6 (-old), 7 (+new), 8 (c3).
        assert_eq!(offset(5), ScreenLines::new(3));
        assert_eq!(
            offset(3),
            ScreenLines::new(3),
            "hidden c0 maps to the next visible row"
        );
    }

    #[test]
    fn counts_the_wrapped_lines_of_each_visible_row() {
        let document = prepared(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-abcdefghijklmnopqrstuvwxyz0123456789abcdefghi\n+abc\n context\n",
        );
        let wrap = WrapWidth::at(20);
        let unified = extent_of(&document, PaneShape::Single, 3, wrap);
        let vertical = extent_of(&document, VERTICAL, 3, wrap);
        let rows = first_rows(&document);
        let line = |row, wrapped_line| RowLine {
            row: rows.line_index(row),
            wrapped_line: ScreenLines::new(wrapped_line),
        };
        let pair = |row, wrapped_line| RowLine {
            row: rows.pair_index(row),
            wrapped_line: ScreenLines::new(wrapped_line),
        };

        assert_eq!(
            unified.line_count(),
            ScreenLines::new(8),
            "the 45-cell deletion takes three lines"
        );
        assert_eq!(
            single_lines(&unified, 4, 3),
            [line(3, 1), line(3, 2), line(4, 0)]
        );
        assert_eq!(
            anchor_offset(&document, &unified, 3, 1),
            ScreenLines::new(4)
        );
        assert_eq!(
            anchor_offset(&document, &unified, 3, 9),
            ScreenLines::new(5),
            "clamped to the row's last line"
        );
        assert_eq!(
            anchor_offset(&document, &unified, 4, 0),
            ScreenLines::new(6)
        );
        assert_eq!(
            vertical.line_count(),
            ScreenLines::new(7),
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
        let mut extent = extent_of(&document, VERTICAL, 0, WrapWidth::Off);
        let narrow = extent.settings();
        let wide = ExtentSettings {
            context: ContextRecords::new(3),
            ..narrow
        };

        // Narrow pairs: ---, +++, @@, -old|+new. Wide adds c0, c1, and c2 before the
        // change.
        let widened = extent.reanchor(&document, wide, ScreenLines::new(3));
        let widened_settings = extent.settings();
        let narrowed = extent.reanchor(&document, narrow, ScreenLines::new(6));

        assert_eq!(widened_settings, wide, "the extent shows the new settings");
        assert_eq!(widened, Some(ScreenLines::new(6)));
        assert_eq!(narrowed, Some(ScreenLines::new(3)));
    }
}
