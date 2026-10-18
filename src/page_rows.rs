//! A page's retained rows: its rendered lines, their side-by-side pairs, and the facts
//! each line's visibility and width derive from. Rules: `docs/spec/architecture.md`
//! (Core model, Viewport coordinates).

use std::{
    iter::{self, Chain, Once},
    option,
};

use crate::{
    compact::Count,
    display_lines::{self, DisplayLine, LineKind},
    document::{FileView, RecordAddress, RecordKind},
    screen::Cells,
    side::{BySide, Side},
    source_text::SourceLine,
    tab_stops::{MeasuredPayload, MeasuredText},
};

/// The position of a rendered line among its page's lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineIndex(Count);

/// The position of a side-by-side row among its page's pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PairIndex(Count);

impl LineIndex {
    fn at(position: usize) -> Self {
        Self(Count::of(position))
    }

    fn get(self) -> usize {
        self.0.get()
    }

    #[cfg(test)]
    pub(crate) fn new(position: usize) -> Self {
        Self::at(position)
    }
}

impl PairIndex {
    fn at(position: usize) -> Self {
        Self(Count::of(position))
    }

    fn get(self) -> usize {
        self.0.get()
    }

    #[cfg(test)]
    pub(crate) fn new(position: usize) -> Self {
        Self::at(position)
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
        let (first, second) = self.split_first();

        iter::once(first).chain(second)
    }

    /// The line the row starts with, and the new line when it pairs two.
    pub(crate) fn split_first(self) -> (T, Option<T>) {
        match self {
            Self::Paired(lines) => (lines.old, Some(lines.new)),
            Self::Shared(line) | Self::OldOnly(line) | Self::NewOnly(line) => (line, None),
        }
    }

    /// The line the row starts with, which names the row across layouts.
    pub(crate) fn first(self) -> T {
        self.split_first().0
    }
}

/// A displayed line as page rows retain it: its kind and its measured payload.
#[derive(Debug, PartialEq, Eq)]
pub struct RenderedLine {
    payload: MeasuredText,
    kind: LineKind,
}

impl RenderedLine {
    fn new(line: DisplayLine<'_>) -> Self {
        Self {
            payload: MeasuredText::new(line.payload()),
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
    pub(crate) fn measured_payload(&self) -> MeasuredPayload<'_> {
        self.payload.as_payload()
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowVisibility {
    Always,
    /// A context row this many records from the nearest change in its hunk.
    NearChange(Count),
    /// A context row in a hunk without changes.
    Never,
}

impl PageRows {
    pub fn of_file(view: &FileView<'_>) -> Self {
        let lines = display_lines::of_text(view.leading)
            .chain(display_lines::of_file(view.file))
            .chain(display_lines::of_text(view.trailing));

        Self::new(lines)
    }

    /// The rows of a document that holds only surrounding text.
    pub fn of_text<'a>(lines: impl IntoIterator<Item = &'a SourceLine>) -> Self {
        Self::new(display_lines::of_text(lines))
    }

    fn new<'a>(lines: impl Iterator<Item = DisplayLine<'a>>) -> Self {
        let lines = lines.map(RenderedLine::new).collect::<Vec<_>>();
        let (pairs, pair_of_line) = side_by_side_rows(&lines);
        let visibility = LineMap(row_visibility(&lines).into_boxed_slice());

        Self {
            lines: lines.into_boxed_slice(),
            pairs: pairs.into_boxed_slice(),
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

    pub fn line(&self, index: LineIndex) -> &RenderedLine {
        &self.lines[index.get()]
    }

    pub fn line_indices(&self) -> impl Iterator<Item = LineIndex> + use<> {
        (0..self.lines.len()).map(LineIndex::at)
    }

    /// One value for each line, computed from the line and its index, in line order.
    pub fn map_lines<T>(&self, mut value: impl FnMut(LineIndex, &RenderedLine) -> T) -> LineMap<T> {
        let values = self.lines().map(|(index, line)| value(index, line));

        LineMap(values.collect())
    }

    /// The value `entries` gives each line it names, and `None` for every other line.
    ///
    /// Panics when `entries` names a line twice or names a line not in these rows.
    pub fn sparse_map<T>(
        &self,
        entries: impl IntoIterator<Item = (LineIndex, T)>,
    ) -> LineMap<Option<T>> {
        let mut values = self.lines.iter().map(|_| None).collect::<Box<[_]>>();
        for (index, value) in entries {
            let slot = &mut values[index.get()];
            assert!(slot.is_none(), "each line takes one value");
            *slot = Some(value);
        }

        LineMap(values)
    }

    pub fn pairs(&self) -> &[SideBySideRow<LineIndex>] {
        &self.pairs
    }

    pub fn pair(&self, index: PairIndex) -> SideBySideRow<LineIndex> {
        self.pairs[index.get()]
    }

    pub(crate) fn pair_indices(&self) -> impl Iterator<Item = PairIndex> + use<> {
        (0..self.pairs.len()).map(PairIndex::at)
    }

    /// The pair holding `line`.
    pub(crate) fn pair_of(&self, line: LineIndex) -> PairIndex {
        *self.pair_of_line.get(line)
    }

    /// Context rows show within `context_lines` records of a change in their own hunk;
    /// every other row always shows.
    pub(crate) fn shows_line(&self, index: LineIndex, context_lines: usize) -> bool {
        match self.visibility.get(index) {
            RowVisibility::Always => true,
            RowVisibility::NearChange(distance) => distance.get() <= context_lines,
            RowVisibility::Never => false,
        }
    }

    pub(crate) fn shows_pair(&self, index: PairIndex, context_lines: usize) -> bool {
        self.pair(index)
            .lines()
            .any(|line| self.shows_line(line, context_lines))
    }

    /// Terminal cells of the line's payload, without the marker gutter.
    pub(crate) fn payload_width(&self, index: LineIndex) -> Cells {
        self.measured_payload(index).cells()
    }

    /// The line's payload with the width measured when the rows were built.
    pub(crate) fn measured_payload(&self, index: LineIndex) -> MeasuredPayload<'_> {
        self.line(index).measured_payload()
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
}

/// The side-by-side rows of `rendered_lines`, and the pair holding each line. Each run of
/// unchanged lines is shared; each block of changed lines pairs its old lines with its new
/// lines in order.
fn side_by_side_rows(
    rendered_lines: &[RenderedLine],
) -> (Vec<SideBySideRow<LineIndex>>, LineMap<PairIndex>) {
    let sides = rendered_lines
        .iter()
        .enumerate()
        .map(|(position, line)| (LineIndex::at(position), line.kind().changed_side()))
        .collect::<Vec<_>>();
    let rows = sides
        .chunk_by(|(_, earlier), (_, later)| earlier.is_some() == later.is_some())
        .flat_map(block_rows)
        .collect::<Vec<_>>();

    let mut pair_of_line = rows
        .iter()
        .enumerate()
        .flat_map(|(pair, row)| row.lines().map(move |line| (line, PairIndex::at(pair))))
        .collect::<Vec<_>>();
    pair_of_line.sort_unstable_by_key(|(line, _)| *line);
    assert!(
        pair_of_line
            .iter()
            .map(|(line, _)| line.get())
            .eq(0..rendered_lines.len()),
        "every line is in exactly one pair"
    );

    let pairs = pair_of_line.into_iter().map(|(_, pair)| pair).collect();
    (rows, LineMap(pairs))
}

/// The rows of one block of lines that are all changed or all unchanged.
fn block_rows(block: &[(LineIndex, Option<Side>)]) -> Vec<SideBySideRow<LineIndex>> {
    let side_lines = |side| {
        block
            .iter()
            .filter(move |(_, changed)| *changed == Some(side))
            .map(|(line, _)| Some(*line))
            .chain(std::iter::repeat(None))
    };

    let [(_, first_side), ..] = block else {
        unreachable!("chunk_by yields only nonempty blocks");
    };

    match first_side {
        None => block
            .iter()
            .map(|(line, _)| SideBySideRow::Shared(*line))
            .collect(),
        Some(_) => side_lines(Side::Old)
            .zip(side_lines(Side::New))
            .map_while(|lines| match lines {
                (Some(old), Some(new)) => Some(SideBySideRow::Paired(BySide { old, new })),
                (Some(old), None) => Some(SideBySideRow::OldOnly(old)),
                (None, Some(new)) => Some(SideBySideRow::NewOnly(new)),
                (None, None) => None,
            })
            .collect(),
    }
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
) -> Vec<Option<Count>> {
    let mut last_change: Option<RecordAddress> = None;

    lines
        .map(|line| {
            let LineKind::Record { address, .. } = line.kind() else {
                return None;
            };
            if line.kind().changed_side().is_some() {
                last_change = Some(address);
            }

            last_change.and_then(|change| change.records_from(address))
        })
        .collect()
}

fn is_context_line(line: &RenderedLine) -> bool {
    line.kind().record_kind() == Some(RecordKind::Context)
}

#[cfg(test)]
mod tests {
    use super::{LineIndex, PageRows, RenderedLine, SideBySideRow};
    use crate::{display_lines::LineKind, parser::parse_patch};

    fn rows_of(patch: &[u8]) -> PageRows {
        let document = parse_patch(patch).expect("valid diff");
        PageRows::of_file(&document.file_views().next().expect("first file"))
    }

    /// The line as a reader sees it: its record marker, then its payload.
    fn shown(line: &RenderedLine) -> String {
        format!("{}{}", line.kind().marker(), line.payload())
    }

    fn shown_lines(rows: &PageRows, lines: impl Iterator<Item = LineIndex>) -> Vec<String> {
        lines.map(|line| shown(rows.line(line))).collect()
    }

    #[test]
    fn payloads_exclude_the_record_marker_and_line_ending() {
        let input =
            b"before\r\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\r\n+new\n\\ No newline at end of file";
        let document = parse_patch(input).expect("valid diff");
        let rows = PageRows::of_file(&document.file_views().next().expect("first file"));

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
        let view = PageRows::of_file(&document.file_views().nth(1).expect("second file"));
        assert_eq!(view.line(view.line_index(2)).kind(), LineKind::HunkHeader);
        assert_eq!(view.line(view.line_index(5)).kind(), LineKind::HunkHeader);
        assert_eq!(
            shown_lines(&view, view.line_indices()),
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
        let view = rows_of(patch.as_bytes());
        assert!(
            view.lines()
                .all(|(_, line)| matches!(line.kind(), LineKind::Metadata))
        );
        assert_eq!(
            shown_lines(&view, view.line_indices()),
            patch.lines().collect::<Vec<_>>()
        );
    }

    #[test]
    fn aligns_asymmetric_changed_blocks_with_blank_vertical_peers() {
        let view = rows_of(
            b"--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        );
        let text = |line| shown(view.line(line));

        assert_eq!(view.pairs().len(), 6);
        assert_eq!(view.lines().count(), 7);
        assert!(matches!(
            view.pairs()[3],
            SideBySideRow::Paired(lines)
                if text(lines.old) == "-old first" && text(lines.new) == "+new first"
        ));
        assert!(matches!(
            view.pairs()[4],
            SideBySideRow::OldOnly(old) if text(old) == "-old second"
        ));
    }
}
