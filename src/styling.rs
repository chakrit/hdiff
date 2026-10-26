//! Row tones and the styled segments of each row's payload. Rules:
//! docs/spec/architecture.md#palette-and-character-detail and
//! docs/spec/architecture.md#technology-direction.

use std::slice;

use crate::{
    detail::{ChangedDetail, character_details},
    display_lines::{DisplayLine, LineKind, RecordPart},
    document::FileView,
    measurement::{Observer, Stage},
    page_rows::{LineIndex, LineMap, PageRows},
    preferences::DiffGranularity,
    side::Side,
    source_text::SourceLine,
    span::PayloadSpan,
    syntax::{DisplayScan, SyntaxClass},
};

/// A page's rows kept with the styles of each line, so a line can only be styled with
/// its own styles.
#[derive(Debug, PartialEq, Eq)]
pub struct StyledRows {
    rows: PageRows,
    styles: LineMap<RowStyles>,
}

impl StyledRows {
    /// The rows of one file occurrence, each line styled with its syntax and its
    /// character detail.
    pub fn of_file_view(view: FileView<'_>, observer: &mut impl Observer) -> Self {
        let rows = observer.time(Stage::Layout, || PageRows::of_file_view(view));

        Self::styled(rows, &mut DisplayScan::of_file(view.file), observer)
    }

    /// The rows of `lines`, part of a page whose syntax `scan` reads in display order:
    /// each part after the first starts at a hunk header, so parts appended in order
    /// style the page as it would be styled whole.
    pub fn of_lines<'a>(
        lines: impl Iterator<Item = DisplayLine<'a>>,
        scan: &mut DisplayScan,
        observer: &mut impl Observer,
    ) -> Self {
        let rows = observer.time(Stage::Layout, || PageRows::of_lines(lines));

        Self::styled(rows, scan, observer)
    }

    /// Appends `later`, the next part of the page.
    pub fn append(&mut self, later: Self) {
        self.rows.append(later.rows);
        self.styles.append(later.styles);
    }

    /// The rows of surrounding text, which has no syntax; its shared rows carry no
    /// character detail.
    pub fn of_text<'a>(
        lines: impl IntoIterator<Item = &'a SourceLine>,
        observer: &mut impl Observer,
    ) -> Self {
        let rows = observer.time(Stage::Layout, || PageRows::of_text(lines));

        Self::styled(rows, &mut DisplayScan::textual(), observer)
    }

    /// The styles of each line of `rows`: the syntax `scan` reads from the lines in
    /// display order, and the character detail derived here from these same rows.
    fn styled(rows: PageRows, scan: &mut DisplayScan, observer: &mut impl Observer) -> Self {
        let details = observer.time(Stage::Detail, || character_details(&rows));
        let mut scratch = PartitionScratch::default();

        let styles = observer.measure(Stage::Styling, |styling| {
            rows.map_lines(|index, line| {
                let text = line.payload();
                let mut syntax = scratch.start(text, TextStyle::Plain);
                scan.line(line.kind(), text, styling, |span, class| {
                    syntax.push(span, TextStyle::Syntax(class));
                });
                let syntax = syntax.finish();
                let character = details
                    .get(index)
                    .as_ref()
                    .map(|changed| character_partition(&mut scratch, text, changed));

                RowStyles { syntax, character }
            })
        });

        Self { rows, styles }
    }

    pub fn rows(&self) -> &PageRows {
        &self.rows
    }

    /// Line `index` styled as `granularity` presents it; a row without character detail
    /// keeps its syntax styles.
    pub fn row(&self, index: LineIndex, granularity: DiffGranularity) -> StyledRow<'_> {
        let line = self.rows.line(index);
        let styles = self.styles.get(index);
        let partition = match granularity {
            DiffGranularity::Line => &styles.syntax,
            DiffGranularity::Character => styles.character.as_ref().unwrap_or(&styles.syntax),
        };

        StyledRow {
            tone: RowTone::of(line.kind()),
            text: line.payload(),
            partition,
        }
    }
}

/// How a row is toned: a change to one side, an unchanged record (context or a no-newline
/// marker), a hunk header, or any other line.
#[derive(Clone, Copy)]
pub enum RowTone {
    Changed(Side),
    UnchangedRecord,
    HunkHeader,
    /// The row reporting a broken section's failure.
    Failure,
    Plain,
}

impl RowTone {
    /// The tone of a line of `line_kind`: a record payload's change, if any; a no-newline
    /// marker is unchanged whatever its record changed.
    fn of(line_kind: LineKind) -> Self {
        match line_kind {
            LineKind::Record {
                kind: record_kind,
                part: RecordPart::Payload,
                ..
            } => record_kind
                .changed_side()
                .map_or(Self::UnchangedRecord, Self::Changed),
            LineKind::Record {
                part: RecordPart::NoNewlineMarker,
                ..
            } => Self::UnchangedRecord,
            LineKind::HunkHeader => Self::HunkHeader,
            LineKind::Failure => Self::Failure,
            LineKind::Text | LineKind::Metadata | LineKind::FileHeader => Self::Plain,
        }
    }
}

/// One row ready to draw: its tone, its sanitized text, and the styled segments of that
/// text.
#[derive(Clone, Copy)]
pub struct StyledRow<'a> {
    tone: RowTone,
    text: &'a str,
    partition: &'a Partition,
}

impl<'a> StyledRow<'a> {
    pub fn tone(&self) -> RowTone {
        self.tone
    }

    /// The row's text as styled segments in order, without empty ones.
    pub fn segments(&self) -> impl Iterator<Item = StyledText<'a, TextStyle>> + use<'a> {
        let text = self.text;

        self.partition
            .boundaries()
            .iter()
            .scan(0, move |start, boundary| {
                let segment = &text[*start..boundary.end];
                *start = boundary.end;
                Some(StyledText {
                    text: segment,
                    style: boundary.style,
                })
            })
            .filter(|segment| !segment.text.is_empty())
    }
}

/// A piece of text drawn in one style: a text style while rows are styled, a terminal
/// style once they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyledText<'a, S> {
    pub text: &'a str,
    pub style: S,
}

/// How a payload segment is drawn: as plain text, as a syntax class, or as character
/// detail of a line that changes one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    Plain,
    Syntax(SyntaxClass),
    Detail(Side, DetailStyle),
}

/// The two kinds of text on a row with character detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailStyle {
    /// A span that differs from the row's peer.
    Changed,
    /// Text between the changed spans.
    Unchanged,
}

/// A row's syntax partition, and its character-detail partition when it has detail.
#[derive(Debug, PartialEq, Eq)]
struct RowStyles {
    syntax: Partition,
    character: Option<Partition>,
}

/// `text` split by its character `detail`: changed spans, and the unchanged text between
/// them.
fn character_partition(
    scratch: &mut PartitionScratch,
    text: &str,
    detail: &ChangedDetail,
) -> Partition {
    let side = detail.side();
    let mut partition = scratch.start(text, TextStyle::Detail(side, DetailStyle::Unchanged));
    for span in detail.spans() {
        partition.push(*span, TextStyle::Detail(side, DetailStyle::Changed));
    }

    partition.finish()
}

/// A row's text split into styled segments: one style for all of it, or the end and
/// style of each segment in order.
#[derive(Debug, PartialEq, Eq)]
enum Partition {
    Uniform(Boundary),
    Split(Box<[Boundary]>),
}

/// Where a styled segment of a payload ends, and the style it is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boundary {
    pub end: usize,
    pub style: TextStyle,
}

impl Partition {
    /// Each segment's end and style, in order.
    fn boundaries(&self) -> &[Boundary] {
        match self {
            Self::Uniform(boundary) => slice::from_ref(boundary),
            Self::Split(boundaries) => boundaries,
        }
    }
}

/// Storage reused across rows, so building a partition allocates only its result.
#[derive(Default)]
struct PartitionScratch(Vec<Boundary>);

impl PartitionScratch {
    /// A partition of `text` to build from its spans; text no span covers takes
    /// `uncovered`.
    fn start(&mut self, text: &str, uncovered: TextStyle) -> PartitionBuild<'_> {
        self.0.clear();

        PartitionBuild {
            boundaries: &mut self.0,
            text_length: text.len(),
            uncovered,
            covered: 0,
        }
    }
}

/// A partition being built from spans that come in order and never overlap.
struct PartitionBuild<'s> {
    boundaries: &'s mut Vec<Boundary>,
    text_length: usize,
    uncovered: TextStyle,
    /// Where the last span ended.
    covered: usize,
}

impl PartitionBuild<'_> {
    /// Adds `span` drawn in `style`, after the text before it.
    ///
    /// Panics when `span` starts before the last span ended.
    fn push(&mut self, span: PayloadSpan, style: TextStyle) {
        assert!(
            span.start() >= self.covered,
            "partition spans come in order and never overlap"
        );

        self.extend(span.start(), self.uncovered);
        self.extend(span.end(), style);
        self.covered = span.end();
    }

    /// The partition, with the text after the last span uncovered.
    fn finish(mut self) -> Partition {
        self.extend(self.text_length, self.uncovered);

        match self.boundaries.as_slice() {
            [] => Partition::Uniform(Boundary {
                end: self.text_length,
                style: self.uncovered,
            }),
            [only] => Partition::Uniform(*only),
            all => Partition::Split(Box::from(all)),
        }
    }

    /// Draws the text up to `end` in `style`, joining the segment before it when that is
    /// drawn the same way.
    fn extend(&mut self, end: usize, style: TextStyle) {
        let start = self.boundaries.last().map_or(0, |boundary| boundary.end);
        if end == start {
            return;
        }

        match self.boundaries.last_mut() {
            Some(previous) if previous.style == style => previous.end = end,
            Some(_) | None => self.boundaries.push(Boundary { end, style }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DetailStyle, Partition, PartitionScratch, RowTone, StyledRow, StyledRows, StyledText,
        TextStyle, character_partition,
    };
    use crate::{
        detail::character_details,
        display_lines::{self, LineKind},
        measurement::Unobserved,
        page_rows::PageRows,
        parser::parse_patch,
        side::Side,
        span::PayloadSpan,
        syntax::{DisplayScan, SyntaxClass},
    };

    fn segments<'a>(text: &'a str, partition: &'a Partition) -> Vec<StyledText<'a, TextStyle>> {
        let row = StyledRow {
            tone: RowTone::Plain,
            text,
            partition,
        };
        row.segments().collect()
    }

    fn styled(text: &str, style: TextStyle) -> StyledText<'_, TextStyle> {
        StyledText { text, style }
    }

    /// docs/spec/architecture.md#lossless-and-safe-rendering: spans style text without
    /// changing it, and adjacent spans drawn alike form one segment.
    #[test]
    fn splits_text_at_ordered_spans_and_joins_adjacent_alike_segments() {
        let text = "αcall(x)!";
        let call = text.find("call").expect("the text holds call");
        let x = text.find('x').expect("the text holds x");
        let keyword = TextStyle::Syntax(SyntaxClass::Keyword);
        let mut scratch = PartitionScratch::default();

        let mut build = scratch.start(text, TextStyle::Plain);
        build.push(PayloadSpan::within(text, call..call + 2), keyword);
        build.push(PayloadSpan::within(text, call + 2..call + 4), keyword);
        build.push(
            PayloadSpan::within(text, x..x + 1),
            TextStyle::Syntax(SyntaxClass::Variable),
        );
        let partition = build.finish();

        assert_eq!(
            segments(text, &partition),
            [
                styled("α", TextStyle::Plain),
                styled("call", keyword),
                styled("(", TextStyle::Plain),
                styled("x", TextStyle::Syntax(SyntaxClass::Variable)),
                styled(")!", TextStyle::Plain),
            ]
        );
    }

    #[test]
    fn marks_changed_graphemes_and_the_unchanged_gaps_between_them() {
        let document = parse_patch("--- a/f\n+++ b/f\n@@ -1 +1 @@\n-αcall!\n+αcell!\n".as_bytes())
            .expect("valid diff");
        let views = document.file_views().collect::<Vec<_>>();
        let [view] = views.as_slice() else {
            panic!("the patch holds one file, not {}", views.len());
        };
        let rows = PageRows::of_file_view(*view);
        let details = character_details(&rows);
        let (addition, _) = rows
            .lines()
            .find(|(_, line)| line.kind().changed_side() == Some(Side::New))
            .expect("the patch adds a line");
        let detail = details
            .get(addition)
            .as_ref()
            .expect("a changed pair has detail");
        let text = rows.line(addition).payload();

        let partition = character_partition(&mut PartitionScratch::default(), text, detail);

        let new_detail = |style| TextStyle::Detail(Side::New, style);
        assert_eq!(
            segments(text, &partition),
            [
                styled("αc", new_detail(DetailStyle::Unchanged)),
                styled("e", new_detail(DetailStyle::Changed)),
                styled("ll!", new_detail(DetailStyle::Unchanged)),
            ]
        );
    }

    #[test]
    fn text_without_spans_takes_the_uncovered_style() {
        let partition = PartitionScratch::default()
            .start("same", TextStyle::Plain)
            .finish();

        assert_eq!(
            segments("same", &partition),
            [styled("same", TextStyle::Plain)]
        );
    }

    /// docs/spec/performance.md#preparation-boundary: a page that fills in one hunk at a
    /// time is the page its whole file prepares.
    #[test]
    fn a_page_built_hunk_by_hunk_is_the_page_built_whole() {
        let patch = "intro\n--- a/a.go\n+++ b/a.go\n@@ -1,2 +1,2 @@\n x := 1\n-/* old\n+// new\n@@ -9,2 +9,3 @@\n y\n-z */ w\n+v\n+u\n";
        let document = parse_patch(patch.as_bytes()).expect("valid diff");
        let views = document.file_views().collect::<Vec<_>>();
        let lines = display_lines::of_file_view(views[0]).collect::<Vec<_>>();
        let second_hunk = lines
            .iter()
            .rposition(|line| line.kind == LineKind::HunkHeader)
            .expect("two hunks");

        let whole = StyledRows::of_file_view(views[0], &mut Unobserved);
        let mut scan = DisplayScan::of_file(views[0].file);
        let mut parts = StyledRows::of_lines(
            lines[..second_hunk].iter().copied(),
            &mut scan,
            &mut Unobserved,
        );
        parts.append(StyledRows::of_lines(
            lines[second_hunk..].iter().copied(),
            &mut scan,
            &mut Unobserved,
        ));

        assert_eq!(parts, whole);
    }
}
