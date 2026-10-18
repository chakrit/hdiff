//! A page's retained rows: its rendered lines, their side-by-side pairs, and the scroll
//! extents each layout shows. Rules: `docs/spec/architecture.md` (Core model, Viewport
//! coordinates).

use crate::{
    document::{FileView, RecordAddress, RecordKind, Side},
    line_breaks::wrapped_line_count,
    preferences::{DiffLayout, Wrapping},
    render::{LineKind, RenderedLine, render_file_lines, render_text_lines},
    source_text::SourceLine,
    tab_stops::expanded_width,
};

pub const MARKER_GUTTER_WIDTH: usize = 2;

/// The position of a rendered line among its layout's lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineIndex(u32);

/// The position of a side-by-side row among its layout's pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PairIndex(u32);

impl LineIndex {
    pub fn get(self) -> usize {
        widen(self.0)
    }
}

impl PairIndex {
    fn get(self) -> usize {
        widen(self.0)
    }
}

/// One side-by-side row: a line both panes show, a changed pair, or a one-sided change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideBySideRow<T> {
    Shared(T),
    Paired { before: T, after: T },
    BeforeOnly(T),
    AfterOnly(T),
}

impl<T> SideBySideRow<T> {
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> SideBySideRow<U> {
        match self {
            Self::Shared(line) => SideBySideRow::Shared(f(line)),
            Self::Paired { before, after } => SideBySideRow::Paired {
                before: f(before),
                after: f(after),
            },
            Self::BeforeOnly(line) => SideBySideRow::BeforeOnly(f(line)),
            Self::AfterOnly(line) => SideBySideRow::AfterOnly(f(line)),
        }
    }

    /// The row's lines in display order: one, or the before line then the after line.
    fn lines(self) -> impl Iterator<Item = T> + Clone
    where
        T: Clone,
    {
        let (first, second) = match self {
            Self::Paired { before, after } => (before, Some(after)),
            Self::Shared(line) | Self::BeforeOnly(line) | Self::AfterOnly(line) => (line, None),
        };

        std::iter::once(first).chain(second)
    }

    /// The line the row starts with, which names the row across layouts.
    fn first(self) -> T {
        match self {
            Self::Paired { before, .. } => before,
            Self::Shared(line) | Self::BeforeOnly(line) | Self::AfterOnly(line) => line,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    lines: Box<[RenderedLine]>,
    pairs: Box<[SideBySideRow<LineIndex>]>,
    /// The pair holding each line.
    pair_of_line: Box<[PairIndex]>,
    line_metrics: Box<[LineMetrics]>,
}

/// Per-line facts derived once so bounds and visibility never rescan the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineMetrics {
    visibility: RowVisibility,
    /// Terminal cells of the line's payload, without the marker gutter.
    payload_width: u32,
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

impl Layout {
    fn new(lines: Vec<RenderedLine>) -> Self {
        let (pairs, pair_of_line) = side_by_side_rows(&lines);
        let line_metrics = row_visibility(&lines)
            .into_iter()
            .zip(&lines)
            .map(|(visibility, line)| LineMetrics {
                visibility,
                payload_width: u32::try_from(expanded_width(line.payload()))
                    .expect("rendered line widths fit in u32"),
            })
            .collect();

        Layout {
            lines: lines.into_boxed_slice(),
            pairs: pairs.into_boxed_slice(),
            pair_of_line,
            line_metrics,
        }
    }

    pub fn lines(&self) -> &[RenderedLine] {
        &self.lines
    }

    pub fn line(&self, index: LineIndex) -> &RenderedLine {
        &self.lines[index.get()]
    }

    /// The index of the line at `position`.
    ///
    /// Panics when `position` is not below the line count.
    pub fn line_index(&self, position: usize) -> LineIndex {
        assert!(
            position < self.lines.len(),
            "line {position} is in the layout"
        );

        LineIndex(narrow(position))
    }

    pub fn line_indices(&self) -> impl Iterator<Item = LineIndex> + use<> {
        (0..narrow(self.lines.len())).map(LineIndex)
    }

    pub fn pairs(&self) -> &[SideBySideRow<LineIndex>] {
        &self.pairs
    }

    pub fn pair(&self, index: PairIndex) -> SideBySideRow<LineIndex> {
        self.pairs[index.get()]
    }

    /// The rows `display_layout` shows at `context_lines`, each broken into lines by
    /// `wrap`.
    pub fn scroll_extent(
        &self,
        display_layout: DiffLayout,
        context_lines: usize,
        wrap: Wrap,
    ) -> DisplayExtent {
        let pair_rows = || {
            ScrollExtent::from_rows(
                wrap,
                (0..narrow(self.pairs.len()))
                    .map(PairIndex)
                    .filter(|index| self.shows_pair(*index, context_lines))
                    .map(|index| self.visible_row(index, self.pair(index).lines(), wrap)),
            )
        };

        match display_layout {
            DiffLayout::Unified => DisplayExtent::Unified(ScrollExtent::from_rows(
                wrap,
                self.line_indices()
                    .filter(|index| self.shows_line(*index, context_lines))
                    .map(|index| self.visible_row(index, std::iter::once(index), wrap)),
            )),
            DiffLayout::Vertical => DisplayExtent::Vertical(pair_rows()),
            DiffLayout::Stacked => DisplayExtent::Stacked(pair_rows()),
        }
    }

    /// The row on screen line `offset` of `extent`, if the lines reach that far.
    pub fn anchor_at(&self, extent: &DisplayExtent, offset: usize) -> Option<RowAnchor> {
        match extent {
            DisplayExtent::Unified(extent) => extent.line_at(offset).map(|screen_line| RowAnchor {
                line: screen_line.row,
                wrapped_line: screen_line.line,
            }),
            DisplayExtent::Vertical(extent) | DisplayExtent::Stacked(extent) => {
                extent.line_at(offset).map(|screen_line| RowAnchor {
                    line: self.pair(screen_line.row).first(),
                    wrapped_line: screen_line.line,
                })
            }
        }
    }

    /// The screen line of `extent` that shows `anchor`, as [`ScrollExtent::offset_of`]
    /// finds it.
    pub fn offset_of(&self, extent: &DisplayExtent, anchor: RowAnchor) -> usize {
        match extent {
            DisplayExtent::Unified(extent) => extent.offset_of(anchor.line, anchor.wrapped_line),
            DisplayExtent::Vertical(extent) | DisplayExtent::Stacked(extent) => {
                let pair = self.pair_of_line[anchor.line.get()];
                extent.offset_of(pair, anchor.wrapped_line)
            }
        }
    }

    /// Context rows show within `context_lines` records of a change in their own hunk;
    /// every other row always shows.
    fn shows_line(&self, index: LineIndex, context_lines: usize) -> bool {
        match self.line_metrics[index.get()].visibility {
            RowVisibility::Always => true,
            RowVisibility::NearChange(distance) => {
                usize::try_from(distance).is_ok_and(|distance| distance <= context_lines)
            }
            RowVisibility::Never => false,
        }
    }

    fn shows_pair(&self, index: PairIndex, context_lines: usize) -> bool {
        self.pair(index)
            .lines()
            .any(|line| self.shows_line(line, context_lines))
    }

    /// A row made of `lines`: as wide as its widest line and as tall as its tallest.
    fn visible_row<R>(
        &self,
        index: R,
        lines: impl Iterator<Item = LineIndex> + Clone,
        wrap: Wrap,
    ) -> VisibleRow<R> {
        let payload_width = lines
            .clone()
            .map(|line| self.payload_width(line))
            .max()
            .unwrap_or(0);
        let height = lines
            .map(|line| self.line_height(line, wrap))
            .max()
            .unwrap_or(1);

        VisibleRow {
            index,
            width: wrap.visible_width(payload_width) + MARKER_GUTTER_WIDTH,
            height,
        }
    }

    fn payload_width(&self, index: LineIndex) -> usize {
        widen(self.line_metrics[index.get()].payload_width)
    }

    /// Screen lines the line takes; only lines wider than the wrap width are measured.
    fn line_height(&self, index: LineIndex, wrap: Wrap) -> usize {
        let Wrap::At(cells) = wrap else {
            return 1;
        };
        let payload_width = self.payload_width(index);
        if payload_width <= cells {
            return 1;
        }

        wrapped_line_count(self.line(index).payload(), payload_width, cells)
    }
}

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

fn widen(value: u32) -> usize {
    usize::try_from(value).expect("u32 values fit in usize")
}

fn narrow(value: usize) -> u32 {
    u32::try_from(value).expect("layout positions fit in u32")
}

pub fn file_layout(view: &FileView<'_>) -> Layout {
    let lines = render_text_lines(view.leading)
        .into_iter()
        .chain(render_file_lines(view.file))
        .chain(render_text_lines(view.trailing))
        .collect::<Vec<_>>();

    Layout::new(lines)
}

/// The layout of a document that holds only surrounding text.
pub fn text_layout<'a>(lines: impl IntoIterator<Item = &'a SourceLine>) -> Layout {
    Layout::new(render_text_lines(lines))
}

/// The side-by-side rows of `rendered_lines`, and the pair holding each line.
fn side_by_side_rows(
    rendered_lines: &[RenderedLine],
) -> (Vec<SideBySideRow<LineIndex>>, Box<[PairIndex]>) {
    let mut rows = Vec::new();
    let mut index = 0;

    while index < rendered_lines.len() {
        if rendered_lines[index].kind().changed_side().is_none() {
            rows.push(SideBySideRow::Shared(LineIndex(narrow(index))));
            index += 1;
            continue;
        }

        let mut before = Vec::new();
        let mut after = Vec::new();
        while let Some(side) = rendered_lines
            .get(index)
            .and_then(|line| line.kind().changed_side())
        {
            let line = LineIndex(narrow(index));
            match side {
                Side::Old => before.push(line),
                Side::New => after.push(line),
            }
            index += 1;
        }

        let mut before = before.into_iter();
        let mut after = after.into_iter();
        loop {
            let row = match (before.next(), after.next()) {
                (Some(before), Some(after)) => SideBySideRow::Paired { before, after },
                (Some(before), None) => SideBySideRow::BeforeOnly(before),
                (None, Some(after)) => SideBySideRow::AfterOnly(after),
                (None, None) => break,
            };
            rows.push(row);
        }
    }

    let mut pair_of_line = vec![PairIndex(0); rendered_lines.len()];
    for (pair, row) in rows.iter().enumerate() {
        for line in row.lines() {
            pair_of_line[line.get()] = PairIndex(narrow(pair));
        }
    }
    (rows, pair_of_line.into_boxed_slice())
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
        DisplayExtent, Layout, LineIndex, PairIndex, ScreenLine, ScrollExtent, SideBySideRow, Wrap,
        file_layout,
    };
    use crate::{
        parser::parse_unified_diff,
        preferences::{DiffLayout, Wrapping},
        render::LineKind,
    };

    fn layout_of(patch: &[u8]) -> Layout {
        let document = parse_unified_diff(patch).expect("valid diff");
        file_layout(&document.file_views().next().expect("first file"))
    }

    fn texts(layout: &Layout, lines: impl Iterator<Item = LineIndex>) -> Vec<&[u8]> {
        lines
            .map(|line| layout.line(line).text().as_bytes())
            .collect()
    }

    fn unified(extent: DisplayExtent) -> ScrollExtent<LineIndex> {
        let DisplayExtent::Unified(extent) = extent else {
            panic!("the unified layout shows lines");
        };
        extent
    }

    #[test]
    fn lays_out_a_selected_file() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-old\n+new\n@@ -1 +1 @@\n-old-again\n+new-again\n",
        )
        .expect("valid multi-file diff");
        let view = file_layout(&document.file_views().nth(1).expect("second file"));
        assert_eq!(view.lines()[2].kind(), LineKind::HunkHeader);
        assert_eq!(view.lines()[5].kind(), LineKind::HunkHeader);
        assert_eq!(
            texts(&view, view.line_indices()),
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
        let view = layout_of(include_bytes!("../tests/fixtures/git-rename-only.patch"));
        assert!(
            view.lines()
                .iter()
                .all(|line| matches!(line.kind(), LineKind::Metadata))
        );
        assert_eq!(
            texts(&view, view.line_indices()),
            include_bytes!("../tests/fixtures/git-rename-only.patch")
                .split_inclusive(|byte| *byte == b'\n')
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn aligns_asymmetric_changed_blocks_with_blank_vertical_peers() {
        let view = layout_of(
            b"--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        );
        let text = |line| view.line(line).text().as_bytes();

        assert_eq!(view.pairs().len(), 6);
        assert_eq!(view.lines().len(), 7);
        assert!(matches!(
            view.pairs()[3],
            SideBySideRow::Paired {
                before,
                after,
            } if text(before) == b"-old first\n" && text(after) == b"+new first\n"
        ));
        assert!(matches!(
            view.pairs()[4],
            SideBySideRow::BeforeOnly(before) if text(before) == b"-old second\n"
        ));
    }

    #[test]
    fn shows_context_rows_within_the_context_count_of_a_change_in_their_hunk() {
        let view = layout_of(
            b"--- a/file\n+++ b/file\n@@ -1,6 +1,6 @@\n c0\n c1\n c2\n-old\n+new\n c3\n c4\n@@ -20,2 +20,2 @@\n far0\n far1\n",
        );
        let shown = |context_lines| {
            let extent = unified(view.scroll_extent(DiffLayout::Unified, context_lines, Wrap::Off));
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
        let view = layout_of(
            "--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n far context that is hidden at one line\n near\n-old\n+\t界界界界界界界界界界\n"
                .as_bytes(),
        );

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
        let view =
            layout_of(b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n");
        let extent = unified(view.scroll_extent(DiffLayout::Unified, 1, Wrap::Off));

        // Visible rows: 0 (---), 1 (+++), 2 (@@), 5 (c2), 6 (-old), 7 (+new), 8 (c3).
        assert_eq!(extent.offset_of(LineIndex(5), 0), 3);
        assert_eq!(
            extent.offset_of(LineIndex(3), 0),
            3,
            "hidden c0 maps to the next visible row"
        );
        assert_eq!(
            extent.offset_of(LineIndex(9), 0),
            7,
            "past the end maps to the row count"
        );
    }

    #[test]
    fn counts_the_wrapped_lines_of_each_visible_row() {
        let view = layout_of(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-abcdefghijklmnopqrstuvwxyz0123456789abcdefghi\n+abc\n context\n",
        );
        let unified = unified(view.scroll_extent(DiffLayout::Unified, 3, Wrap::At(20)));
        let DisplayExtent::Vertical(vertical) =
            view.scroll_extent(DiffLayout::Vertical, 3, Wrap::At(20))
        else {
            panic!("the vertical layout shows pairs");
        };
        let line = |row, line| ScreenLine {
            row: LineIndex(row),
            line,
        };
        let pair = |row, line| ScreenLine {
            row: PairIndex(row),
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
        assert_eq!(unified.offset_of(LineIndex(3), 1), 4);
        assert_eq!(
            unified.offset_of(LineIndex(3), 9),
            5,
            "clamped to the row's last line"
        );
        assert_eq!(unified.offset_of(LineIndex(4), 0), 6);
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

    #[test]
    fn side_by_side_rows_reference_owned_lines() {
        let view = layout_of(b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n");
        let SideBySideRow::Shared(source_index) = view.pairs()[0] else {
            panic!("file header is a shared row");
        };

        let line = view.line(source_index);

        assert_eq!(line.text().as_bytes(), b"--- a/file\n");
        assert_eq!(line.kind(), LineKind::FileHeader);
    }

    /// architecture.md (View model): an anchored row keeps its place when its layout
    /// rebuilds at another context count.
    #[test]
    fn finds_an_anchored_pair_again_in_a_rebuilt_extent() {
        let view =
            layout_of(b"--- a/file\n+++ b/file\n@@ -1,5 +1,5 @@\n c0\n c1\n c2\n-old\n+new\n c3\n");
        let narrow = view.scroll_extent(DiffLayout::Vertical, 0, Wrap::Off);
        let wide = view.scroll_extent(DiffLayout::Vertical, 3, Wrap::Off);

        // Narrow pairs: ---, +++, @@, -old|+new. Wide adds c0, c1, and c2 before the change.
        let change = view.anchor_at(&narrow, 3).expect("the change is on line 3");

        assert_eq!(view.offset_of(&wide, change), 6);
        assert_eq!(view.offset_of(&narrow, change), 3);
    }
}
