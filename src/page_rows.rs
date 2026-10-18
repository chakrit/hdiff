//! A page's retained rows: its rendered lines, their side-by-side pairs, and the facts
//! each line's visibility and width derive from. Rules: `docs/spec/architecture.md`
//! (Core model, Viewport coordinates).

use crate::{
    document::{FileView, RecordAddress, RecordKind, Side},
    render::{LineKind, RenderedLine, render_file_lines, render_text_lines},
    source_text::SourceLine,
    tab_stops::expanded_width,
};

/// The position of a rendered line among its page's lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineIndex(u32);

/// The position of a side-by-side row among its page's pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PairIndex(u32);

impl LineIndex {
    pub fn get(self) -> usize {
        widen(self.0)
    }

    #[cfg(test)]
    pub(crate) fn new(position: usize) -> Self {
        Self(narrow(position))
    }
}

impl PairIndex {
    fn get(self) -> usize {
        widen(self.0)
    }

    #[cfg(test)]
    pub(crate) fn new(position: usize) -> Self {
        Self(narrow(position))
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
    pub(crate) fn lines(self) -> impl Iterator<Item = T> + Clone
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
    pub(crate) fn first(self) -> T {
        match self {
            Self::Paired { before, .. } => before,
            Self::Shared(line) | Self::BeforeOnly(line) | Self::AfterOnly(line) => line,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PageRows {
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

impl PageRows {
    pub fn of_file(view: &FileView<'_>) -> Self {
        let lines = render_text_lines(view.leading)
            .into_iter()
            .chain(render_file_lines(view.file))
            .chain(render_text_lines(view.trailing))
            .collect::<Vec<_>>();

        Self::new(lines)
    }

    /// The rows of a document that holds only surrounding text.
    pub fn of_text<'a>(lines: impl IntoIterator<Item = &'a SourceLine>) -> Self {
        Self::new(render_text_lines(lines))
    }

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

        Self {
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
            "line {position} is in the page rows"
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

    pub(crate) fn pair_indices(&self) -> impl Iterator<Item = PairIndex> + use<> {
        (0..narrow(self.pairs.len())).map(PairIndex)
    }

    /// The pair holding `line`.
    pub(crate) fn pair_of(&self, line: LineIndex) -> PairIndex {
        self.pair_of_line[line.get()]
    }

    /// Context rows show within `context_lines` records of a change in their own hunk;
    /// every other row always shows.
    pub(crate) fn shows_line(&self, index: LineIndex, context_lines: usize) -> bool {
        match self.line_metrics[index.get()].visibility {
            RowVisibility::Always => true,
            RowVisibility::NearChange(distance) => {
                usize::try_from(distance).is_ok_and(|distance| distance <= context_lines)
            }
            RowVisibility::Never => false,
        }
    }

    pub(crate) fn shows_pair(&self, index: PairIndex, context_lines: usize) -> bool {
        self.pair(index)
            .lines()
            .any(|line| self.shows_line(line, context_lines))
    }

    /// Terminal cells of the line's payload, without the marker gutter.
    pub(crate) fn payload_width(&self, index: LineIndex) -> usize {
        widen(self.line_metrics[index.get()].payload_width)
    }
}

pub(crate) fn widen(value: u32) -> usize {
    usize::try_from(value).expect("u32 values fit in usize")
}

pub(crate) fn narrow(value: usize) -> u32 {
    u32::try_from(value).expect("row positions fit in u32")
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
    use super::{LineIndex, PageRows, SideBySideRow};
    use crate::{parser::parse_unified_diff, render::LineKind};

    fn rows_of(patch: &[u8]) -> PageRows {
        let document = parse_unified_diff(patch).expect("valid diff");
        PageRows::of_file(&document.file_views().next().expect("first file"))
    }

    fn texts(rows: &PageRows, lines: impl Iterator<Item = LineIndex>) -> Vec<&[u8]> {
        lines
            .map(|line| rows.line(line).text().as_bytes())
            .collect()
    }

    #[test]
    fn lays_out_a_selected_file() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-old\n+new\n@@ -1 +1 @@\n-old-again\n+new-again\n",
        )
        .expect("valid multi-file diff");
        let view = PageRows::of_file(&document.file_views().nth(1).expect("second file"));
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
        let view = rows_of(include_bytes!("../tests/fixtures/git-rename-only.patch"));
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
        let view = rows_of(
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
    fn side_by_side_rows_reference_owned_lines() {
        let view = rows_of(b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n");
        let SideBySideRow::Shared(source_index) = view.pairs()[0] else {
            panic!("file header is a shared row");
        };

        let line = view.line(source_index);

        assert_eq!(line.text().as_bytes(), b"--- a/file\n");
        assert_eq!(line.kind(), LineKind::FileHeader);
    }
}
