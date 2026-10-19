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
    Unmatched { side: Side, line: T },
}

impl<T> SideBySideRow<T> {
    /// The row with `f` applied to each line it holds.
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> SideBySideRow<U> {
        match self {
            Self::Shared(line) => SideBySideRow::Shared(f(line)),
            Self::Paired(lines) => SideBySideRow::Paired(lines.map(f)),
            Self::Unmatched { side, line } => SideBySideRow::Unmatched {
                side,
                line: f(line),
            },
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
            Self::Shared(line) | Self::Unmatched { line, .. } => SplitLines {
                first: line,
                second: None,
            },
        }
    }

    /// The line the row starts with, which names the row across layouts.
    pub(crate) fn first(self) -> T {
        self.split_first().first
    }

    /// The changed line each side holds in this row; a shared row changes neither side.
    pub(crate) fn changed_lines(self) -> BySide<Option<T>> {
        match self {
            Self::Shared(_) => BySide::default(),
            Self::Paired(lines) => lines.map(Some),
            Self::Unmatched { side, line } => BySide::only(side, line),
        }
    }

    /// The line both panes show, when the row changes neither side.
    fn shared_line(self) -> Option<T> {
        match self {
            Self::Shared(line) => Some(line),
            Self::Paired(_) | Self::Unmatched { .. } => None,
        }
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
        Self::of_lines(display_lines::of_file_view(view))
    }

    /// The rows of a document that holds only surrounding text.
    pub fn of_text<'a>(lines: impl IntoIterator<Item = &'a SourceLine>) -> Self {
        Self::of_lines(display_lines::of_text(lines))
    }

    /// The rows of `lines`, with their side-by-side layout and each line's visibility
    /// derived once.
    fn of_lines<'a>(lines: impl Iterator<Item = DisplayLine<'a>>) -> Self {
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

    /// Lines that show a record's payload; a no-newline marker is part of its record.
    pub fn record_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| line.kind().payload_record().is_some())
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
        let placed_values = self
            .pairs
            .iter()
            .flat_map(|row| values_on_lines(*row, values_of_pair(*row)));

        LineMap(in_line_order(placed_values))
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
    let shared = row
        .shared_line()
        .map(|line| LineValue { line, value: None });
    let changed = row
        .changed_lines()
        .zip(values)
        .into_iter()
        .filter_map(|(line, value)| line.map(|line| LineValue { line, value }));

    shared.into_iter().chain(changed)
}

/// The values of `placed`, one per line, in line order. Lines list a changed block's old
/// lines before its new lines, while a row holds the block's *i*th old and *i*th new
/// lines together, so values placed row by row are not in line order.
fn in_line_order<T>(placed: impl IntoIterator<Item = LineValue<T>>) -> Box<[T]> {
    let mut placed = placed.into_iter().collect::<Vec<_>>();
    placed.sort_by_key(|line_value| line_value.line);

    placed
        .into_iter()
        .map(|line_value| line_value.value)
        .collect()
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
        let blocks = Block::all_of(rendered_lines)
            .iter()
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

/// A changed line with the side it changes.
struct LineSide {
    line: LineIndex,
    side: Side,
}

/// A maximal run of consecutive lines: all unchanged, or all changed with their sides.
enum Block {
    Unchanged(Vec<LineIndex>),
    Changed(Vec<LineSide>),
}

impl Block {
    /// The blocks of `rendered_lines`, in line order. Each line joins the block before
    /// it when that block is of its kind, and opens a new block otherwise.
    fn all_of(rendered_lines: &[RenderedLine]) -> Vec<Self> {
        let mut blocks = Vec::<Self>::new();
        for (position, rendered_line) in rendered_lines.iter().enumerate() {
            let line = LineIndex::at(position);
            match (blocks.last_mut(), rendered_line.kind().changed_side()) {
                (Some(Self::Unchanged(lines)), None) => lines.push(line),
                (Some(Self::Changed(lines)), Some(side)) => lines.push(LineSide { line, side }),
                (None | Some(Self::Changed(_)), None) => blocks.push(Self::Unchanged(vec![line])),
                (None | Some(Self::Unchanged(_)), Some(side)) => {
                    blocks.push(Self::Changed(vec![LineSide { line, side }]));
                }
            }
        }

        blocks
    }
}

/// The rows of one block, and the pair holding each of its lines, in line order.
struct BlockRows {
    pairs: Vec<SideBySideRow<LineIndex>>,
    pair_of_line: Vec<PairIndex>,
}

impl BlockRows {
    /// The rows of `block`, the first of which is the page's pair `first_pair`.
    fn of(block: &Block, first_pair: PairIndex) -> Self {
        match block {
            Block::Unchanged(lines) => Self {
                pairs: lines.iter().copied().map(SideBySideRow::Shared).collect(),
                pair_of_line: (0..lines.len())
                    .map(|offset| first_pair.offset(offset))
                    .collect(),
            },
            Block::Changed(lines) => Self::changed(lines, first_pair),
        }
    }

    /// A changed block's *i*th old line and *i*th new line share row *i*. Each side's
    /// lines keep their order, so a no-newline marker sits right after its record in its
    /// side's column, and every line of the block takes a row.
    fn changed(block: &[LineSide], first_pair: PairIndex) -> Self {
        let column = |side| {
            block
                .iter()
                .filter(|line_side| line_side.side == side)
                .map(|line_side| line_side.line)
                .collect::<Vec<_>>()
        };
        let columns = BySide::of(column);

        let paired_count = columns.as_ref().merge(|old, new| old.len().min(new.len()));
        let paired = columns.as_ref().merge(|old, new| {
            old.iter()
                .zip(new)
                .map(|(&old, &new)| SideBySideRow::Paired(BySide { old, new }))
        });
        let unmatched = columns.as_ref().map_sides(|side, column| {
            column[paired_count..]
                .iter()
                .map(move |&line| SideBySideRow::Unmatched { side, line })
        });
        let pairs = paired
            .chain(unmatched.into_iter().flatten())
            .collect::<Vec<_>>();

        let placed_pairs = pairs.iter().enumerate().flat_map(|(offset, row)| {
            row.lines().map(move |line| LineValue {
                line,
                value: first_pair.offset(offset),
            })
        });
        let pair_of_line = in_line_order(placed_pairs).into_vec();

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
            let LineKind::Record { address, kind, .. } = line.kind() else {
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
    matches!(line.kind().record_kind(), Some(RecordKind::Context(_)))
}

#[cfg(test)]
mod tests {
    use super::{PageRows, RenderedLine, SideBySideRow};
    use crate::{
        display_lines::{LineKind, RecordPart},
        parser::parse_patch,
        preferences::ContextRecords,
        side::{BySide, Side},
    };

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

    /// Every row of `rows` that holds changed lines, as a reader sees its lines.
    fn changed_pairs(rows: &PageRows) -> Vec<SideBySideRow<String>> {
        rows.pair_indices()
            .map(|index| rows.pair(index))
            .filter(|row| !matches!(row, SideBySideRow::Shared(_)))
            .map(|row| row.map(|line| shown(rows.line(line))))
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
                SideBySideRow::Unmatched {
                    side: Side::Old,
                    line: "-old second".to_owned(),
                },
                SideBySideRow::Shared(" context".to_owned()),
            ]
        );
    }

    /// docs/spec/architecture.md#view-model: a changed block renders as aligned
    /// before/after pairs; a no-newline marker belongs to the record it follows, so it
    /// stays in its record's block and on its record's side.
    #[test]
    fn pairs_an_end_of_file_edit_with_each_side_s_no_newline_marker() {
        let marker = "\\ No newline at end of file";
        let rows = rows_of(
            format!("--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n{marker}\n+new\n{marker}\n")
                .as_bytes(),
        );

        assert_eq!(
            changed_pairs(&rows),
            [
                SideBySideRow::Paired(BySide {
                    old: "-old".to_owned(),
                    new: "+new".to_owned(),
                }),
                SideBySideRow::Paired(BySide {
                    old: marker.to_owned(),
                    new: marker.to_owned(),
                }),
            ]
        );
    }

    /// docs/spec/architecture.md#core-model: a no-newline marker shows on its record's
    /// side, right after its record, even when the other side of its changed block holds
    /// more records.
    #[test]
    fn keeps_a_no_newline_marker_right_after_its_record_in_an_uneven_block() {
        let marker = "\\ No newline at end of file";
        let rows = rows_of(
            format!("--- a/f\n+++ b/f\n@@ -1,2 +1,3 @@\n-a\n-b\n{marker}\n+c\n+d\n+e\n").as_bytes(),
        );

        assert_eq!(
            changed_pairs(&rows),
            [
                SideBySideRow::Paired(BySide {
                    old: "-a".to_owned(),
                    new: "+c".to_owned(),
                }),
                SideBySideRow::Paired(BySide {
                    old: "-b".to_owned(),
                    new: "+d".to_owned(),
                }),
                SideBySideRow::Paired(BySide {
                    old: marker.to_owned(),
                    new: "+e".to_owned(),
                }),
            ]
        );
    }

    /// docs/spec/architecture.md#view-model: context records show within the
    /// context-record count of a change. docs/spec/architecture.md#core-model: a
    /// no-newline marker shows exactly when the record it follows does.
    #[test]
    fn hides_a_no_newline_marker_with_the_context_record_it_follows() {
        let rows = rows_of(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-a\n+b\n last\n\\ No newline at end of file\n",
        );
        let address_of = |kind| match kind {
            LineKind::Record { address, .. } => Some(address),
            LineKind::Text | LineKind::Metadata | LineKind::FileHeader | LineKind::HunkHeader => {
                None
            }
        };
        let (marker, marker_kind) = rows
            .lines()
            .map(|(index, line)| (index, line.kind()))
            .find(|(_, kind)| {
                matches!(
                    kind,
                    LineKind::Record {
                        part: RecordPart::NoNewlineMarker,
                        ..
                    }
                )
            })
            .expect("the hunk ends with a no-newline marker");
        let record = rows
            .lines()
            .find(|(index, line)| {
                *index != marker && address_of(line.kind()) == address_of(marker_kind)
            })
            .map(|(index, _)| index)
            .expect("the marker's record shares its address");

        for records in [0, 1] {
            let context = ContextRecords::new(records);
            let record_shown = rows.shows_line(record, context);

            assert_eq!(
                rows.shows_line(marker, context),
                record_shown,
                "context {records}"
            );
        }
    }
}
