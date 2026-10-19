//! A page's retained rows: its rendered lines, their side-by-side pairs, and the facts
//! each line's visibility and width derive from. Rules:
//! docs/spec/architecture.md#core-model and
//! docs/spec/architecture.md#viewport-coordinates.

use std::{
    iter::{self, Chain, Once},
    marker::PhantomData,
    option,
};

use crate::{
    compact::Count,
    display_lines::{self, DisplayLine, LineKind},
    document::{FileView, RecordAddress, RecordKind},
    preferences::ContextRecords,
    screen::Cells,
    side::{BySide, Side},
    source_text::SourceLine,
    tab_stops::{MeasuredText, MeasuredTextRef},
};

/// The position of an item in its page's position space `Of`. The space keeps a line
/// position from indexing pairs, and a pair position from indexing lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PagePosition<Of>(Count, PhantomData<Of>);

/// The position space of a page's rendered lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LineSpace {}

/// The position space of a page's side-by-side rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PairSpace {}

/// The position of a rendered line among its page's lines.
pub type LineIndex = PagePosition<LineSpace>;

/// The position of a side-by-side row among its page's pairs.
pub type PairIndex = PagePosition<PairSpace>;

impl<Of> PagePosition<Of> {
    fn at(position: usize) -> Self {
        Self(Count::of(position), PhantomData)
    }

    fn get(self) -> usize {
        self.0.get()
    }

    /// The position `count` items after this one.
    fn offset(self, count: usize) -> Self {
        Self::at(self.get() + count)
    }
}

/// One side-by-side row: a line both panes show, a changed pair, or a change to one side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideBySideRow<T> {
    Shared(T),
    Paired(BySide<T>),
    OldOnly(T),
    NewOnly(T),
}

impl<T> SideBySideRow<T> {
    /// The row with `f` applied to each line it holds.
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> SideBySideRow<U> {
        match self {
            Self::Shared(line) => SideBySideRow::Shared(f(line)),
            Self::Paired(lines) => SideBySideRow::Paired(lines.map(f)),
            Self::OldOnly(line) => SideBySideRow::OldOnly(f(line)),
            Self::NewOnly(line) => SideBySideRow::NewOnly(f(line)),
        }
    }

    /// The row's lines in display order: one, or the old line then the new line.
    pub(crate) fn lines(self) -> Chain<Once<T>, option::IntoIter<T>> {
        let SplitLines { first, second } = self.split_first();

        iter::once(first).chain(second)
    }

    /// The line the row starts with, and the new line when it pairs two.
    pub(crate) fn split_first(self) -> SplitLines<T> {
        match self {
            Self::Paired(lines) => SplitLines {
                first: lines.old,
                second: Some(lines.new),
            },
            Self::Shared(line) | Self::OldOnly(line) | Self::NewOnly(line) => SplitLines {
                first: line,
                second: None,
            },
        }
    }

    /// The line the row starts with, which names the row across layouts.
    pub(crate) fn first(self) -> T {
        self.split_first().first
    }
}

/// A side-by-side row's lines: the one it starts with, and the new line when it pairs
/// two.
pub(crate) struct SplitLines<T> {
    pub first: T,
    pub second: Option<T>,
}

/// A displayed line as page rows retain it: its kind and its measured payload.
#[derive(Debug, PartialEq, Eq)]
pub struct RenderedLine {
    payload: MeasuredText,
    kind: LineKind,
}

impl RenderedLine {
    fn of(line: DisplayLine<'_>) -> Self {
        Self {
            payload: MeasuredText::of_text(line.payload()),
            kind: line.kind,
        }
    }

    pub fn kind(&self) -> LineKind {
        self.kind
    }

    /// The text between the record marker and the line ending.
    pub fn payload(&self) -> &str {
        self.payload.text()
    }

    /// The payload with the cells it covers once its tabs are expanded.
    pub(crate) fn measured_text(&self) -> MeasuredTextRef<'_> {
        self.payload.borrowed()
    }
}

/// One value for each line of a page's rows, looked up by [`LineIndex`]; only those rows
/// build one.
#[derive(Debug, PartialEq, Eq)]
pub struct LineMap<T>(Box<[T]>);

impl<T> LineMap<T> {
    /// The value of line `index`.
    ///
    /// Panics when `index` names no line of the rows this map was built from.
    pub fn get(&self, index: LineIndex) -> &T {
        &self.0[index.get()]
    }
}

/// A page's retained rows: its rendered lines, their side-by-side pairs, the pair
/// holding each line, and each line's visibility.
#[derive(Debug, PartialEq, Eq)]
pub struct PageRows {
    lines: Box<[RenderedLine]>,
    pairs: Box<[SideBySideRow<LineIndex>]>,
    /// The pair holding each line.
    pair_of_line: LineMap<PairIndex>,
    /// Derived once so visibility never rescans the file.
    visibility: LineMap<RowVisibility>,
}

/// When a rendered line shows.
#[derive(Debug, PartialEq, Eq)]
enum RowVisibility {
    Always,
    /// A context row this many records from the nearest change in its hunk.
    NearChange(Count),
    /// A context row in a hunk without changes.
    Never,
}

impl PageRows {
    /// The rows of a file page: the file's lines with the text around it.
    pub fn of_file_view(view: FileView<'_>) -> Self {
        Self::new(display_lines::of_file_view(view))
    }

    /// The rows of a document that holds only surrounding text.
    pub fn of_text<'a>(lines: impl IntoIterator<Item = &'a SourceLine>) -> Self {
        Self::new(display_lines::of_text(lines))
    }

    /// The rows of `lines`, with their side-by-side layout and each line's visibility
    /// derived once.
    fn new<'a>(lines: impl Iterator<Item = DisplayLine<'a>>) -> Self {
        let lines = lines.map(RenderedLine::of).collect::<Vec<_>>();
        let SideBySideLayout {
            pairs,
            pair_of_line,
        } = SideBySideLayout::of(&lines);
        let visibility = LineMap(row_visibility(&lines).into_boxed_slice());

        Self {
            lines: lines.into_boxed_slice(),
            pairs,
            pair_of_line,
            visibility,
        }
    }

    /// Each line with its index, in line order.
    pub fn lines(&self) -> impl Iterator<Item = (LineIndex, &RenderedLine)> {
        self.line_indices().zip(&self.lines)
    }

    /// Lines that show a record.
    pub fn record_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| line.kind().record_kind().is_some())
            .count()
    }

    /// The line `index`.
    pub fn line(&self, index: LineIndex) -> &RenderedLine {
        &self.lines[index.get()]
    }

    /// Every line's index, in line order.
    pub fn line_indices(&self) -> impl Iterator<Item = LineIndex> + use<> {
        (0..self.lines.len()).map(LineIndex::at)
    }

    /// One value for each line, computed from the line and its index, in line order.
    pub fn map_lines<T>(&self, mut value: impl FnMut(LineIndex, &RenderedLine) -> T) -> LineMap<T> {
        let values = self.lines().map(|(index, line)| value(index, line));

        LineMap(values.collect())
    }

    /// One value for each line, computed per side-by-side row: each pair's per-side
    /// values go to the lines it holds, so a changed line takes its side's value and a
    /// shared line takes none.
    pub fn line_values_by_pair<T>(
        &self,
        mut values_of_pair: impl FnMut(SideBySideRow<LineIndex>) -> BySide<Option<T>>,
    ) -> LineMap<Option<T>> {
        let mut placed_values = self
            .pairs
            .iter()
            .flat_map(|row| values_on_lines(*row, values_of_pair(*row)))
            .collect::<Vec<_>>();
        // A changed block lists its old lines before its new lines, so pair order is not
        // line order.
        placed_values.sort_by_key(|line_value| line_value.line);

        let values = placed_values.into_iter().map(|line_value| line_value.value);

        LineMap(values.collect())
    }

    /// The side-by-side row `index`.
    pub fn pair(&self, index: PairIndex) -> SideBySideRow<LineIndex> {
        self.pairs[index.get()]
    }

    /// Every side-by-side row's index, in pair order.
    pub(crate) fn pair_indices(&self) -> impl Iterator<Item = PairIndex> + use<> {
        (0..self.pairs.len()).map(PairIndex::at)
    }

    /// The pair holding `line`.
    pub(crate) fn pair_of(&self, line: LineIndex) -> PairIndex {
        *self.pair_of_line.get(line)
    }

    /// Context rows show within `context` records of a change in their own hunk; every
    /// other row always shows.
    pub(crate) fn shows_line(&self, index: LineIndex, context: ContextRecords) -> bool {
        match self.visibility.get(index) {
            RowVisibility::Always => true,
            RowVisibility::NearChange(distance) => context.admits(*distance),
            RowVisibility::Never => false,
        }
    }

    /// Whether any line of pair `index` shows within `context` records of a change.
    pub(crate) fn shows_pair(&self, index: PairIndex, context: ContextRecords) -> bool {
        self.pair(index)
            .lines()
            .any(|line| self.shows_line(line, context))
    }

    /// Terminal cells of the line's payload, without the marker gutter.
    pub(crate) fn payload_width(&self, index: LineIndex) -> Cells {
        self.measured_text(index).cells()
    }

    /// The line's payload with the width measured when the rows were built.
    pub(crate) fn measured_text(&self, index: LineIndex) -> MeasuredTextRef<'_> {
        self.line(index).measured_text()
    }
}

#[cfg(test)]
impl PageRows {
    /// The index of the line at `position`.
    ///
    /// Panics when `position` is not below the line count.
    pub fn line_index(&self, position: usize) -> LineIndex {
        assert!(
            position < self.lines.len(),
            "line {position} is in the page rows"
        );

        LineIndex::at(position)
    }

    /// The index of the side-by-side row at `position`.
    ///
    /// Panics when `position` is not below the pair count.
    pub fn pair_index(&self, position: usize) -> PairIndex {
        assert!(
            position < self.pairs.len(),
            "pair {position} is in the page rows"
        );

        PairIndex::at(position)
    }
}

/// A value placed on the line it belongs to.
struct LineValue<T> {
    line: LineIndex,
    value: T,
}

/// Each line `row` holds, with the value of the side it shows; a shared line shows no
/// side.
fn values_on_lines<T>(
    row: SideBySideRow<LineIndex>,
    values: BySide<Option<T>>,
) -> impl Iterator<Item = LineValue<Option<T>>> {
    let on_line = |line, value| Some(LineValue { line, value });
    let placed_values = match row {
        SideBySideRow::Shared(line) => [on_line(line, None), None],
        SideBySideRow::Paired(lines) => [
            on_line(lines.old, values.old),
            on_line(lines.new, values.new),
        ],
        SideBySideRow::OldOnly(line) => [on_line(line, values.old), None],
        SideBySideRow::NewOnly(line) => [on_line(line, values.new), None],
    };

    placed_values.into_iter().flatten()
}

/// A page's side-by-side rows and the pair holding each of its lines.
struct SideBySideLayout {
    pairs: Box<[SideBySideRow<LineIndex>]>,
    pair_of_line: LineMap<PairIndex>,
}

impl SideBySideLayout {
    /// The layout of `rendered_lines`. Each run of unchanged lines is shared; each block
    /// of changed lines pairs its old lines with its new lines in order.
    fn of(rendered_lines: &[RenderedLine]) -> Self {
        let sides = rendered_lines
            .iter()
            .enumerate()
            .map(|(position, line)| LineSide {
                line: LineIndex::at(position),
                side: line.kind().changed_side(),
            })
            .collect::<Vec<_>>();
        let blocks = sides
            .chunk_by(|earlier, later| earlier.side.is_some() == later.side.is_some())
            .scan(PairIndex::at(0), |first_pair, block| {
                let block_rows = BlockRows::of(block, *first_pair);
                *first_pair = first_pair.offset(block_rows.pairs.len());
                Some(block_rows)
            })
            .collect::<Vec<_>>();

        let pair_of_line = blocks
            .iter()
            .flat_map(|block| block.pair_of_line.iter().copied())
            .collect();
        let pairs = blocks.into_iter().flat_map(|block| block.pairs).collect();

        Self {
            pairs,
            pair_of_line: LineMap(pair_of_line),
        }
    }
}

/// A line with the side it changes: `Option<Side>` before a block is known to be changed,
/// `Side` within a changed block.
struct LineSide<S> {
    line: LineIndex,
    side: S,
}

/// The rows of one block of lines that are all changed or all unchanged, and the pair
/// holding each of its lines, in line order.
struct BlockRows {
    pairs: Vec<SideBySideRow<LineIndex>>,
    pair_of_line: Vec<PairIndex>,
}

impl BlockRows {
    /// The rows of `block`, the first of which is the page's pair `first_pair`.
    fn of(block: &[LineSide<Option<Side>>], first_pair: PairIndex) -> Self {
        // A block is uniform, so its lines all carry a side exactly when it is changed.
        let changed = block
            .iter()
            .map(|line_side| {
                line_side.side.map(|side| LineSide {
                    line: line_side.line,
                    side,
                })
            })
            .collect::<Option<Vec<_>>>();

        match changed {
            None => Self {
                pairs: block
                    .iter()
                    .map(|line_side| SideBySideRow::Shared(line_side.line))
                    .collect(),
                pair_of_line: (0..block.len())
                    .map(|offset| first_pair.offset(offset))
                    .collect(),
            },
            Some(changed) => Self::changed(&changed, first_pair),
        }
    }

    /// A changed block pairs its *i*th old line with its *i*th new line in row *i*.
    fn changed(block: &[LineSide<Side>], first_pair: PairIndex) -> Self {
        let side_lines = |side| {
            block
                .iter()
                .filter(move |line_side| line_side.side == side)
                .map(|line_side| Some(line_side.line))
                .chain(iter::repeat(None))
        };
        let pairs = side_lines(Side::Old)
            .zip(side_lines(Side::New))
            .map_while(|lines| match lines {
                (Some(old), Some(new)) => Some(SideBySideRow::Paired(BySide { old, new })),
                (Some(old), None) => Some(SideBySideRow::OldOnly(old)),
                (None, Some(new)) => Some(SideBySideRow::NewOnly(new)),
                (None, None) => None,
            })
            .collect();

        let starts = BySide {
            old: first_pair,
            new: first_pair,
        };
        let pair_of_line = block
            .iter()
            .scan(starts, |next_pair, line_side| {
                let pair = *next_pair.get(line_side.side);
                *next_pair.get_mut(line_side.side) = pair.offset(1);
                Some(pair)
            })
            .collect();

        Self {
            pairs,
            pair_of_line,
        }
    }
}

/// When each of `rendered_lines` shows: a context line by its record distance to the
/// nearest change in its hunk, every other line always.
fn row_visibility(rendered_lines: &[RenderedLine]) -> Vec<RowVisibility> {
    let distances_before = distances_from_previous_change(rendered_lines.iter());
    let mut distances_after = distances_from_previous_change(rendered_lines.iter().rev());
    distances_after.reverse();

    rendered_lines
        .iter()
        .zip(distances_before.into_iter().zip(distances_after))
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
) -> Vec<Option<Count>> {
    lines
        .scan(None, |last_change: &mut Option<RecordAddress>, line| {
            let LineKind::Record { address, kind } = line.kind() else {
                return Some(None);
            };
            if kind.changed_side().is_some() {
                *last_change = Some(address);
            }

            Some(last_change.and_then(|change| change.records_from(address)))
        })
        .collect()
}

/// Whether `line` shows an unchanged record of a hunk.
fn is_context_line(line: &RenderedLine) -> bool {
    line.kind().record_kind() == Some(RecordKind::Context)
}

#[cfg(test)]
mod tests {
    use super::{PageRows, RenderedLine, SideBySideRow};
    use crate::{display_lines::LineKind, parser::parse_patch, side::BySide};

    /// The rows of `patch`'s first file.
    fn rows_of(patch: &[u8]) -> PageRows {
        let document = parse_patch(patch).expect("valid diff");
        PageRows::of_file_view(document.file_views().next().expect("first file"))
    }

    /// The line as a reader sees it: its record marker, then its payload.
    fn shown(line: &RenderedLine) -> String {
        format!("{}{}", line.kind().marker(), line.payload())
    }

    /// Every line of `rows` as a reader sees it, in line order.
    fn shown_lines(rows: &PageRows) -> Vec<String> {
        rows.line_indices()
            .map(|line| shown(rows.line(line)))
            .collect()
    }

    #[test]
    fn payloads_exclude_the_record_marker_and_line_ending() {
        let rows = rows_of(
            b"before\r\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\r\n+new\n\\ No newline at end of file",
        );

        let payloads = rows
            .lines()
            .map(|(_, line)| line.payload())
            .collect::<Vec<_>>();

        assert_eq!(
            payloads,
            [
                "before",
                "--- a/file",
                "+++ b/file",
                "@@ -1 +1 @@",
                "old",
                "new",
                "\\ No newline at end of file"
            ]
        );
    }

    #[test]
    fn lays_out_one_file_page() {
        let document = parse_patch(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-old\n+new\n@@ -1 +1 @@\n-old-again\n+new-again\n",
        )
        .expect("valid multi-file diff");
        let rows = PageRows::of_file_view(document.file_views().nth(1).expect("second file"));

        let shown = shown_lines(&rows);
        let hunk_headers = rows
            .lines()
            .map(|(_, line)| line.kind() == LineKind::HunkHeader)
            .collect::<Vec<_>>();
        let hunk_header_texts = shown
            .iter()
            .map(|text| text.starts_with("@@"))
            .collect::<Vec<_>>();

        assert_eq!(hunk_headers, hunk_header_texts, "lines: {shown:?}");
        assert_eq!(
            shown,
            [
                "--- a/second",
                "+++ b/second",
                "@@ -1 +1 @@",
                "-old",
                "+new",
                "@@ -1 +1 @@",
                "-old-again",
                "+new-again",
            ]
        );
    }

    #[test]
    fn lays_out_a_metadata_only_file() {
        let patch = include_str!("../tests/fixtures/git-rename-only.patch");
        let rows = rows_of(patch.as_bytes());

        let kinds = rows
            .lines()
            .map(|(_, line)| line.kind())
            .collect::<Vec<_>>();
        assert!(
            kinds.iter().all(|kind| *kind == LineKind::Metadata),
            "{kinds:?}"
        );
        assert_eq!(shown_lines(&rows), patch.lines().collect::<Vec<_>>());
    }

    #[test]
    fn aligns_asymmetric_changed_blocks_with_blank_vertical_peers() {
        let rows = rows_of(
            b"--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        );
        let text = |line| shown(rows.line(line));
        let pairs = rows
            .pair_indices()
            .map(|index| rows.pair(index).map(text))
            .collect::<Vec<_>>();

        assert_eq!(rows.lines().count(), 7);
        assert_eq!(
            pairs,
            [
                SideBySideRow::Shared("--- a/file".to_owned()),
                SideBySideRow::Shared("+++ b/file".to_owned()),
                SideBySideRow::Shared("@@ -1,3 +1,2 @@".to_owned()),
                SideBySideRow::Paired(BySide {
                    old: "-old first".to_owned(),
                    new: "+new first".to_owned(),
                }),
                SideBySideRow::OldOnly("-old second".to_owned()),
                SideBySideRow::Shared(" context".to_owned()),
            ]
        );
    }
}
