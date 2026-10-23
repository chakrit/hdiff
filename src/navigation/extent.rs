//! The rows one pane shape shows of one page at one context count and terminal-derived
//! wrap width: their screen lines, the widest row, and how a row keeps its place when
//! they are rebuilt. Rules: docs/spec/architecture.md#viewport-coordinates.

use std::iter;

use crate::{
    compact::Count,
    effective_view::{HorizontalFit, WrapWidth},
    geometry::{PaneShape, SplitDirection},
    line_breaks::wrapped_line_count,
    page_rows::{LineIndex, PageRows, PairIndex, SideBySideRow},
    preferences::{ContextRecords, DiffGranularity},
    prepared::{PageIndex, PreparedDocument, PreparedPage},
    screen::{Cells, MARKER_GUTTER_WIDTH, ScreenLines},
    styling::StyledRow,
};

/// What a page's rows are shown with: the pane shape, the context count, and the wrap
/// width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ExtentSettings {
    pub(super) shape: PaneShape,
    pub(super) context: ContextRecords,
    pub(super) wrap: WrapWidth,
}

/// The screen lines a pane asks for, named so the start and the count cannot swap.
pub(super) struct LineRequest {
    /// The first screen line asked for.
    pub(super) first: ScreenLines,
    /// At most this many screen lines.
    pub(super) count: ScreenLines,
}

/// One screen line: wrapped line `wrapped_line`, counted from the row's first, of row
/// `row`.
#[derive(Debug, PartialEq, Eq)]
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

/// The rows of one page that one pane shape shows at one context count and wrap width,
/// their screen lines, and the widest row. A viewport's vertical offset indexes these
/// lines; the extent resolves its rows on its own page, so its indexes reach no other.
pub struct ScrollExtent {
    page: PageIndex,
    context: ContextRecords,
    wrap: WrapWidth,
    visible: VisibleRows,
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
                    .map(|index| measured_row(rows, index, iter::once(index), wrap))
                    .collect(),
            ),
            PaneShape::Split(direction) => VisibleRows::Split {
                direction,
                pairs: rows
                    .pair_indices()
                    .filter(|index| rows.shows_pair(*index, context))
                    .map(|index| measured_row(rows, index, rows.pair(index).lines(), wrap))
                    .collect(),
            },
        };

        Self {
            page,
            context,
            wrap,
            visible,
        }
    }

    /// Screen lines the visible rows take.
    pub fn line_count(&self) -> ScreenLines {
        match &self.visible {
            VisibleRows::Single(lines) => lines.line_count(),
            VisibleRows::Split { pairs, .. } => pairs.line_count(),
        }
    }

    /// Cells of the widest visible row, including the marker gutter.
    fn width(&self) -> Cells {
        match &self.visible {
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
    pub(super) fn horizontal_range(&self, content_width: Cells) -> Cells {
        self.wrap.horizontal_range(HorizontalFit {
            rows_width: self.width(),
            content_width,
        })
    }

    /// The screen lines `request` asks for, with their rows from this extent's page of
    /// `document` in `granularity`'s styles.
    pub(super) fn shown<'a>(
        &'a self,
        document: &'a PreparedDocument,
        granularity: DiffGranularity,
        request: LineRequest,
    ) -> ShownLines<'a> {
        let page = document.page(self.page);

        match &self.visible {
            VisibleRows::Single(lines) => ShownLines::Single(PageLines {
                page,
                granularity,
                lines: lines.lines_from(request),
            }),
            VisibleRows::Split { direction, pairs } => ShownLines::Split {
                direction: *direction,
                lines: PageLines {
                    page,
                    granularity,
                    lines: pairs.lines_from(request),
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
        let shape = match &self.visible {
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
        match &self.visible {
            VisibleRows::Single(lines) => lines.line_at(offset),
            VisibleRows::Split { pairs, .. } => {
                let rows = document.page(self.page).rows();
                pairs
                    .line_at(offset)
                    .map(|line| line.map(|pair_index| rows.pair(pair_index).first()))
            }
        }
    }

    /// The screen line that shows `anchor`'s wrapped line, clamped to its row's lines; a
    /// row that is not visible maps to the first line of the first visible row after it.
    fn offset_of(&self, document: &PreparedDocument, anchor: RowLine<LineIndex>) -> ScreenLines {
        match &self.visible {
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
        let page = self.page;
        let granularity = self.granularity;

        self.lines
            .next()
            .map(|line| line.map(|line_index| page.row(line_index, granularity)))
    }
}

impl<'a> Iterator for PageLines<'a, PairIndex> {
    type Item = RowLine<SideBySideRow<StyledRow<'a>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let page = self.page;
        let granularity = self.granularity;

        self.lines.next().map(|line| {
            line.map(|pair_index| {
                page.rows()
                    .pair(pair_index)
                    .map(|line_index| page.row(line_index, granularity))
            })
        })
    }
}

/// The visible rows: a single pane shows lines, and split panes show pairs.
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
struct MeasuredRows<R> {
    rows: Box<[PlacedRow<R>]>,
    width: Cells,
}

/// A visible row and the screen line just past its last one.
struct PlacedRow<R> {
    row: R,
    line_end: LineEnd,
}

/// Screen lines `rows` take, placed one below the other: the line end of the last one,
/// or none for no rows.
fn lines_taken<R>(rows: &[PlacedRow<R>]) -> ScreenLines {
    rows.last()
        .map_or(ScreenLines::ZERO, |placed| placed.line_end.get())
}

impl<R: Copy + Ord> MeasuredRows<R> {
    /// Screen lines the rows take.
    fn line_count(&self) -> ScreenLines {
        lines_taken(&self.rows)
    }

    /// The screen lines `request` asks for.
    fn lines_from(&self, request: LineRequest) -> RowLines<'_, R> {
        let position = self.position_at(request.first);

        RowLines {
            measured: self,
            position,
            wrapped_line: request.first.saturating_sub(self.line_start(position)),
            remaining: request.count,
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
    /// row that is not visible maps to the first line of the first visible row after it.
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

    /// The first screen line of the row at `position`.
    fn line_start(&self, position: usize) -> ScreenLines {
        lines_taken(&self.rows[..position])
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
                let line_start = lines_taken(&rows);
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
#[derive(Clone, Copy)]
struct LineEnd(Count);

impl LineEnd {
    /// The end at `lines`; panics past `u32`, as `Count::of` does.
    fn of(lines: ScreenLines) -> Self {
        Self(Count::of(lines.get()))
    }

    fn get(self) -> ScreenLines {
        ScreenLines::new(self.0.get())
    }
}

/// Screen lines of an extent's rows, in display order.
struct RowLines<'a, R> {
    /// The rows the lines come from.
    measured: &'a MeasuredRows<R>,
    /// Position of the row the next line belongs to.
    position: usize,
    /// The next line's wrapped line within its row.
    wrapped_line: ScreenLines,
    /// Lines still to yield.
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

/// A row made of `lines`: as wide as its widest line and as tall as its tallest.
fn measured_row<R>(
    rows: &PageRows,
    row: R,
    lines: impl Iterator<Item = LineIndex>,
    wrap: WrapWidth,
) -> MeasuredRow<R> {
    let measure = |line| LineSize {
        payload_width: rows.payload_width(line),
        height: line_height(rows, line, wrap),
    };

    let size = lines.map(measure).fold(LineSize::NONE, LineSize::max);

    MeasuredRow {
        row,
        width: size.payload_width + MARKER_GUTTER_WIDTH,
        height: size.height,
    }
}

/// One line's payload cells and the screen lines it takes.
struct LineSize {
    payload_width: Cells,
    height: ScreenLines,
}

impl LineSize {
    /// The size of no line, which any line's size outgrows.
    const NONE: Self = Self {
        payload_width: Cells::ZERO,
        height: ScreenLines::ZERO,
    };

    /// As wide as the wider line and as tall as the taller.
    fn max(self, other: Self) -> Self {
        Self {
            payload_width: self.payload_width.max(other.payload_width),
            height: self.height.max(other.height),
        }
    }
}

/// Screen lines the line takes; only lines wider than the wrap width are measured.
fn line_height(rows: &PageRows, index: LineIndex, wrap: WrapWidth) -> ScreenLines {
    match wrap {
        WrapWidth::At(cells) => {
            let payload = rows.measured_text(index);
            if payload.cells() <= cells.get() {
                ScreenLines::ONE
            } else {
                wrapped_line_count(payload, cells)
            }
        }
        WrapWidth::Off | WrapWidth::NoPayload => ScreenLines::ONE,
    }
}

#[cfg(test)]
mod tests {
    use super::{ExtentSettings, LineRequest, RowLine, ScrollExtent, VisibleRows};
    use crate::{
        effective_view::WrapWidth,
        geometry::{PaneShape, SplitDirection},
        page_rows::{LineIndex, PageRows, PairIndex, RenderedLine},
        parser::parse_patch,
        preferences::ContextRecords,
        prepared::{PageIndex, PreparedDocument},
        screen::{Cells, ScreenLines},
    };

    const VERTICAL: PaneShape = PaneShape::Split(SplitDirection::Vertical);

    /// The wrap width at which [`WRAPPED_DELETION`] takes three lines.
    const DELETION_WRAP: WrapWidth = WrapWidth::at(20);

    /// A deletion of 45 payload cells, three lines at [`DELETION_WRAP`], as its line
    /// reads.
    const WRAPPED_DELETION: &str = "-abcdefghijklmnopqrstuvwxyz0123456789abcdefghi";

    /// [`WRAPPED_DELETION`], the addition it pairs with, and a context record.
    fn wrapped_deletion_patch() -> String {
        format!("--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n{WRAPPED_DELETION}\n+abc\n context\n")
    }

    /// One change between three context records before it and one after.
    const CHANGE_AMONG_CONTEXT: &[u8] =
        b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n";

    /// `patch` parsed and prepared.
    fn prepared(patch: &[u8]) -> PreparedDocument {
        PreparedDocument::prepare(&parse_patch(patch).expect("valid diff"))
    }

    /// The first page's extent in `shape` at `context` records of context, wrapped at
    /// `wrap`.
    fn extent_of(
        document: &PreparedDocument,
        shape: PaneShape,
        context: ContextRecords,
        wrap: WrapWidth,
    ) -> ScrollExtent {
        let settings = ExtentSettings {
            shape,
            context,
            wrap,
        };

        ScrollExtent::of(document, PageIndex::FIRST, settings)
    }

    /// The rows of the document's first page.
    fn first_rows(document: &PreparedDocument) -> &PageRows {
        document.page(PageIndex::FIRST).rows()
    }

    /// A line as a reader sees it: its record marker, then its payload.
    fn reading(line: &RenderedLine) -> String {
        format!("{}{}", line.kind().marker(), line.payload())
    }

    /// Each line as a reader sees it, then a newline.
    fn texts(document: &PreparedDocument, lines: impl Iterator<Item = LineIndex>) -> Vec<String> {
        let rows = first_rows(document);

        lines
            .map(|index| reading(rows.line(index)) + "\n")
            .collect()
    }

    /// The screen lines `request` asks for of a single-pane extent.
    fn single_lines(extent: &ScrollExtent, request: LineRequest) -> Vec<RowLine<LineIndex>> {
        let VisibleRows::Single(lines) = &extent.visible else {
            panic!("a single pane shows lines");
        };
        lines.lines_from(request).collect()
    }

    /// The screen lines `request` asks for of a split-pane extent.
    fn pair_lines(extent: &ScrollExtent, request: LineRequest) -> Vec<RowLine<PairIndex>> {
        let VisibleRows::Split { pairs, .. } = &extent.visible else {
            panic!("split panes show pairs");
        };
        pairs.lines_from(request).collect()
    }

    /// The first page's line that reads as `text`.
    fn line_reading(document: &PreparedDocument, text: &str) -> LineIndex {
        first_rows(document)
            .lines()
            .find(|(_, line)| reading(line) == text)
            .map(|(index, _)| index)
            .unwrap_or_else(|| panic!("a line reads {text:?}"))
    }

    /// A screen line naming wrapped line `wrapped_line` of the first page's line that
    /// reads as `text`.
    fn line_at(
        document: &PreparedDocument,
        text: &str,
        wrapped_line: ScreenLines,
    ) -> RowLine<LineIndex> {
        RowLine {
            row: line_reading(document, text),
            wrapped_line,
        }
    }

    #[test]
    fn shows_context_rows_within_the_context_count_of_a_change_in_their_hunk() {
        let document = prepared(
            b"--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n c0\n c1\n c2\n-old\n+new\n c3\n c4\n@@ -20,2 +20,2 @@\n far0\n far1\n",
        );
        let shown = |count| {
            let context = ContextRecords::new(count);
            let extent = extent_of(&document, PaneShape::Single, context, WrapWidth::Off);
            let all = LineRequest {
                first: ScreenLines::ZERO,
                count: ScreenLines::new(usize::MAX),
            };
            let lines = single_lines(&extent, all).into_iter().map(|line| line.row);
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
        const FAR_CONTEXT: &str = "far context that is hidden at one line";
        const ADDITION: &str = "\t界界界界界界界界界界";
        let document = prepared(
            format!("--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n {FAR_CONTEXT}\n near\n-old\n+{ADDITION}\n")
                .as_bytes(),
        );

        let widest = |shape, count| {
            extent_of(&document, shape, ContextRecords::new(count), WrapWidth::Off).width()
        };

        // docs/spec/architecture.md#view-model and #lossless-and-safe-rendering: a row
        // spans the marker column and payload spacer (2 cells) and its payload, whose tab
        // expands to the 2-column stop. The addition is 2 + 2 + ten two-cell characters
        // = 24 cells; the far context line is 2 + 38 characters = 40 cells.
        let addition = Cells::new(24);
        let far_context = Cells::new(40);

        assert_eq!(widest(PaneShape::Single, 1), addition);
        assert_eq!(widest(VERTICAL, 1), addition);
        assert_eq!(widest(PaneShape::Single, 2), far_context);
    }

    #[test]
    fn maps_a_row_to_its_visible_offset_or_the_next_visible_row() {
        let document = prepared(CHANGE_AMONG_CONTEXT);
        let context = ContextRecords::new(1);
        let extent = extent_of(&document, PaneShape::Single, context, WrapWidth::Off);
        let offset =
            |text| extent.offset_of(&document, line_at(&document, text, ScreenLines::ZERO));

        assert_eq!(
            offset(" c2"),
            ScreenLines::new(3),
            "c2 shows below the three header rows"
        );
        assert_eq!(
            offset(" c0"),
            ScreenLines::new(3),
            "hidden c0 maps to the next visible row"
        );
    }

    #[test]
    fn counts_and_shows_the_wrapped_lines_of_each_visible_row() {
        let document = prepared(wrapped_deletion_patch().as_bytes());
        let wrap = DELETION_WRAP;
        let context = ContextRecords::new(3);
        let unified = extent_of(&document, PaneShape::Single, context, wrap);
        let vertical = extent_of(&document, VERTICAL, context, wrap);
        let rows = first_rows(&document);
        let line = |text, wrapped_line| line_at(&document, text, wrapped_line);
        let pair = |text, wrapped_line| RowLine {
            row: rows.pair_of(line_reading(&document, text)),
            wrapped_line,
        };
        let [first, second, third] = [0, 1, 2].map(ScreenLines::new);
        let unified_from = unified.offset_of(&document, line(WRAPPED_DELETION, second));
        let vertical_from = vertical.offset_of(&document, line(WRAPPED_DELETION, third));

        assert_eq!(
            unified.line_count(),
            ScreenLines::new(8),
            "the 45-cell deletion takes three lines"
        );
        assert_eq!(
            single_lines(
                &unified,
                LineRequest {
                    first: unified_from,
                    count: ScreenLines::new(3),
                }
            ),
            [
                line(WRAPPED_DELETION, second),
                line(WRAPPED_DELETION, third),
                line("+abc", first),
            ]
        );
        assert_eq!(
            vertical.line_count(),
            ScreenLines::new(7),
            "the paired row takes its taller side's three lines"
        );
        assert_eq!(
            pair_lines(
                &vertical,
                LineRequest {
                    first: vertical_from,
                    count: ScreenLines::new(2),
                }
            ),
            [pair(WRAPPED_DELETION, third), pair(" context", first)]
        );
    }

    #[test]
    fn maps_an_anchor_to_its_wrapped_line_clamped_to_its_rows_lines() {
        let document = prepared(wrapped_deletion_patch().as_bytes());
        let context = ContextRecords::new(3);
        let unified = extent_of(&document, PaneShape::Single, context, DELETION_WRAP);
        let anchor_offset = |text, wrapped_line| {
            unified.offset_of(&document, line_at(&document, text, wrapped_line))
        };

        assert_eq!(
            anchor_offset(WRAPPED_DELETION, ScreenLines::new(1)),
            ScreenLines::new(4)
        );
        assert_eq!(
            anchor_offset(WRAPPED_DELETION, ScreenLines::new(9)),
            ScreenLines::new(5),
            "clamped to the row's last line"
        );
        assert_eq!(
            anchor_offset("+abc", ScreenLines::ZERO),
            ScreenLines::new(6)
        );
    }

    /// docs/spec/architecture.md#view-model: an anchored row keeps its place when its
    /// layout rebuilds at another context count.
    #[test]
    fn finds_an_anchored_pair_again_in_a_rebuilt_extent() {
        let document = prepared(CHANGE_AMONG_CONTEXT);
        let [narrow_context, wide_context] = [0, 3].map(ContextRecords::new);
        // Narrow pairs: ---, +++, @@, then -old|+new on line 3. Wide context adds c0, c1
        // and c2 before the change, moving it to line 6.
        let [narrow_offset, wide_offset] = [3, 6].map(ScreenLines::new);
        let mut extent = extent_of(&document, VERTICAL, narrow_context, WrapWidth::Off);
        let narrow = extent.settings();
        let wide = ExtentSettings {
            context: wide_context,
            ..narrow
        };

        let widened = extent.reanchor(&document, wide, narrow_offset);
        let widened_settings = extent.settings();
        let narrowed = extent.reanchor(&document, narrow, wide_offset);

        assert_eq!(widened_settings, wide, "the extent shows the new settings");
        assert_eq!(widened, Some(wide_offset));
        assert_eq!(narrowed, Some(narrow_offset));
    }
}
