//! A page's retained rows: its rendered lines, their side-by-side pairs, and the facts
//! each line's visibility and width derive from. Rules: `docs/spec/architecture.md`
//! (Core model, Viewport coordinates).

use crate::{
    compact::Count,
    display_lines::{self, DisplayLine, LineEnding, LineKind},
    document::{BySide, FileView, RecordAddress, RecordKind, Side},
    screen::Cells,
    source_text::SourceLine,
    tab_stops::MeasuredPayload,
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
    pub(crate) fn lines(self) -> impl Iterator<Item = T> + Clone
    where
        T: Clone,
    {
        let (first, second) = match self {
            Self::Paired(lines) => (lines.old, Some(lines.new)),
            Self::Shared(line) | Self::OldOnly(line) | Self::NewOnly(line) => (line, None),
        };

        std::iter::once(first).chain(second)
    }

    /// The line the row starts with, which names the row across layouts.
    pub(crate) fn first(self) -> T {
        match self {
            Self::Paired(lines) => lines.old,
            Self::Shared(line) | Self::OldOnly(line) | Self::NewOnly(line) => line,
        }
    }
}

/// A displayed line as page rows retain it: its kind and its whole text.
#[derive(Debug, PartialEq, Eq)]
pub struct RenderedLine {
    text: Box<str>,
    kind: LineKind,
}

impl RenderedLine {
    fn new(line: DisplayLine<'_>) -> Self {
        let mut text = String::new();
        line.write_text(&mut text);

        Self {
            text: text.into_boxed_str(),
            kind: line.kind,
        }
    }

    pub fn kind(&self) -> LineKind {
        self.kind
    }

    /// The whole line as finite output writes it.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The text between the record marker and the line ending.
    pub fn payload(&self) -> &str {
        let start = self.kind.marker().len();
        let end = self.text.len() - LineEnding::of(self.text.as_bytes()).text().len();

        &self.text[start..end]
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
    line_metrics: LineMap<LineMetrics>,
}

/// Per-line facts derived once so bounds and visibility never rescan the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineMetrics {
    visibility: RowVisibility,
    /// Terminal cells of the line's payload, without the marker gutter.
    payload_width: Count,
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
        let line_metrics = row_visibility(&lines)
            .into_iter()
            .zip(&lines)
            .map(|(visibility, line)| LineMetrics {
                visibility,
                payload_width: Count::of(MeasuredPayload::new(line.payload()).cells().get()),
            })
            .collect();

        Self {
            lines: lines.into_boxed_slice(),
            pairs: pairs.into_boxed_slice(),
            pair_of_line,
            line_metrics: LineMap(line_metrics),
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
        match self.line_metrics.get(index).visibility {
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
        Cells::new(self.line_metrics.get(index).payload_width.get())
    }

    /// The line's payload with the width measured when the rows were built.
    pub(crate) fn measured_payload(&self, index: LineIndex) -> MeasuredPayload<'_> {
        MeasuredPayload::measured(self.line(index).payload(), self.payload_width(index))
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

    match block.first() {
        Some((_, None)) => block
            .iter()
            .map(|(line, _)| SideBySideRow::Shared(*line))
            .collect(),
        Some((_, Some(_))) => side_lines(Side::Old)
            .zip(side_lines(Side::New))
            .map_while(|lines| match lines {
                (Some(old), Some(new)) => Some(SideBySideRow::Paired(BySide { old, new })),
                (Some(old), None) => Some(SideBySideRow::OldOnly(old)),
                (None, Some(new)) => Some(SideBySideRow::NewOnly(new)),
                (None, None) => None,
            })
            .collect(),
        None => Vec::new(),
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
    use super::{LineIndex, PageRows, SideBySideRow};
    use crate::{display_lines::LineKind, parser::parse_patch};

    fn rows_of(patch: &[u8]) -> PageRows {
        let document = parse_patch(patch).expect("valid diff");
        PageRows::of_file(&document.file_views().next().expect("first file"))
    }

    fn texts(rows: &PageRows, lines: impl Iterator<Item = LineIndex>) -> Vec<&[u8]> {
        lines
            .map(|line| rows.line(line).text().as_bytes())
            .collect()
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
        let view = rows_of(include_bytes!("../tests/fixtures/git-rename-only.patch"));
        assert!(
            view.lines()
                .all(|(_, line)| matches!(line.kind(), LineKind::Metadata))
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
        let view = rows_of(
            b"--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        );
        let text = |line| view.line(line).text().as_bytes();

        assert_eq!(view.pairs().len(), 6);
        assert_eq!(view.lines().count(), 7);
        assert!(matches!(
            view.pairs()[3],
            SideBySideRow::Paired(lines)
                if text(lines.old) == b"-old first\n" && text(lines.new) == b"+new first\n"
        ));
        assert!(matches!(
            view.pairs()[4],
            SideBySideRow::OldOnly(old) if text(old) == b"-old second\n"
        ));
    }
}
