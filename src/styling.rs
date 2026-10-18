//! Row tones and the styled segments of each row's payload. Rules:
//! `docs/spec/architecture.md` (View model, Technology direction).

use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{
    detail::ChangedDetail,
    display_lines::LineKind,
    document::RecordMap,
    page_rows::{LineIndex, LineMap, PageRows},
    preferences::DiffGranularity,
    side::Side,
    span::PayloadSpan,
    syntax::{SyntaxClass, SyntaxSpan},
};

/// A page's rows kept with the styles of each line, so a line can only be styled with
/// its own styles.
#[derive(Debug, PartialEq, Eq)]
pub struct StyledRows {
    rows: PageRows,
    styles: LineMap<RowStyles>,
}

impl StyledRows {
    /// The styles of each line of `rows`: the syntax spans of its record, looked up by
    /// address, and its character detail when `details` gives it some.
    pub fn new(
        rows: PageRows,
        syntax: &RecordMap<Vec<SyntaxSpan>>,
        details: &LineMap<Option<ChangedDetail>>,
    ) -> Self {
        let styles = rows.map_lines(|index, line| {
            let text = line.payload();
            let spans: &[SyntaxSpan] = match line.kind() {
                LineKind::Record { address, .. } => syntax.get(address),
                LineKind::Text
                | LineKind::Metadata
                | LineKind::FileHeader
                | LineKind::HunkHeader => &[],
            };
            let character = details.get(index).as_ref();

            RowStyles {
                syntax: syntax_partition(text, spans),
                character: character.map(|detail| character_partition(text, detail)),
            }
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

/// How a row is toned: a change to one side, an unchanged record (context or raw), a hunk
/// header, or any other line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowTone {
    Changed(Side),
    UnchangedRecord,
    HunkHeader,
    Plain,
}

impl RowTone {
    fn of(kind: LineKind) -> Self {
        match kind {
            LineKind::Record { kind, .. } => kind
                .changed_side()
                .map_or(Self::UnchangedRecord, Self::Changed),
            LineKind::HunkHeader => Self::HunkHeader,
            LineKind::Text | LineKind::Metadata | LineKind::FileHeader => Self::Plain,
        }
    }
}

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

    pub fn segments(&self) -> impl Iterator<Item = (&'a str, TextStyle)> + use<'a> {
        let text = self.text;

        self.partition
            .boundaries()
            .iter()
            .scan(0, move |start, boundary| {
                let segment = &text[*start..boundary.end];
                *start = boundary.end;
                Some((segment, boundary.style))
            })
            .filter(|(segment, _)| !segment.is_empty())
    }
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

fn syntax_partition(text: &str, syntax: &[SyntaxSpan]) -> Partition {
    let annotations = syntax
        .iter()
        .map(|span| Annotation {
            span: span.span,
            priority_length: span.capture_length,
            style: TextStyle::Syntax(span.class),
        })
        .collect::<Vec<_>>();

    Partition::new(text, &annotations, TextStyle::Plain)
}

fn character_partition(text: &str, detail: &ChangedDetail) -> Partition {
    let side = detail.side();
    let annotations = detail
        .spans()
        .iter()
        .map(|span| Annotation {
            span: *span,
            priority_length: span.len(),
            style: TextStyle::Detail(side, DetailStyle::Changed),
        })
        .collect::<Vec<_>>();

    Partition::new(
        text,
        &annotations,
        TextStyle::Detail(side, DetailStyle::Unchanged),
    )
}

struct Annotation {
    span: PayloadSpan,
    priority_length: usize,
    style: TextStyle,
}

/// A row's text split into styled segments: one style for all of it, or the end and
/// style of each segment in order.
#[derive(Debug, PartialEq, Eq)]
enum Partition {
    Uniform(Boundary),
    Split(Box<[Boundary]>),
}

#[derive(Debug, PartialEq, Eq)]
struct Boundary {
    end: usize,
    style: TextStyle,
}

impl Partition {
    /// Splits `text` at annotation edges, which are spans of `text`; text no annotation
    /// covers takes `uncovered`.
    fn new(text: &str, annotations: &[Annotation], uncovered: TextStyle) -> Self {
        if annotations.is_empty() {
            return Self::Uniform(Boundary {
                end: text.len(),
                style: uncovered,
            });
        }

        let mut offsets = annotations
            .iter()
            .flat_map(|annotation| [annotation.span.start(), annotation.span.end()])
            .chain([0, text.len()])
            .collect::<Vec<_>>();
        offsets.sort_unstable();
        offsets.dedup();

        let mut starts = (0..annotations.len()).collect::<Vec<_>>();
        starts.sort_unstable_by_key(|index| annotations[*index].span.start());
        let mut starts = starts.into_iter().peekable();
        let mut active = BinaryHeap::new();
        let mut boundaries: Vec<Boundary> = Vec::new();

        for interval in offsets.windows(2) {
            let [start, end] = [interval[0], interval[1]];
            while let Some(index) =
                starts.next_if(|index| annotations[*index].span.start() <= start)
            {
                let annotation = &annotations[index];
                active.push((Reverse(annotation.priority_length), index));
            }
            while let Some((_, index)) = active.peek()
                && annotations[*index].span.end() <= start
            {
                active.pop();
            }
            // architecture.md: shortest capture wins; later encounters break ties.
            let style = active
                .peek()
                .map_or(uncovered, |(_, index)| annotations[*index].style);
            if let Some(previous) = boundaries.last_mut()
                && previous.style == style
            {
                previous.end = end;
            } else {
                boundaries.push(Boundary { end, style });
            }
        }

        Self::Split(boundaries.into_boxed_slice())
    }

    fn boundaries(&self) -> &[Boundary] {
        match self {
            Self::Uniform(boundary) => std::slice::from_ref(boundary),
            Self::Split(boundaries) => boundaries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DetailStyle, Partition, RowTone, StyledRow, TextStyle, character_partition,
        syntax_partition,
    };
    use crate::{
        detail::character_details,
        page_rows::PageRows,
        parser::parse_patch,
        side::Side,
        span::PayloadSpan,
        syntax::{SyntaxClass, SyntaxSpan},
    };

    fn segments<'a>(text: &'a str, partition: &'a Partition) -> Vec<(&'a str, TextStyle)> {
        let row = StyledRow {
            tone: RowTone::Plain,
            text,
            partition,
        };
        row.segments().collect()
    }

    fn syntax(
        text: &str,
        start: usize,
        end: usize,
        length: usize,
        class: SyntaxClass,
    ) -> SyntaxSpan {
        SyntaxSpan {
            span: PayloadSpan::within(text, start, end),
            capture_length: length,
            class,
        }
    }

    #[test]
    fn chooses_specific_captures_and_later_ties_without_losing_outer_styles() {
        let text = "αcall!";
        let spans = [
            syntax(text, 0, 7, 7, SyntaxClass::String),
            syntax(text, 2, 6, 4, SyntaxClass::Property),
            syntax(text, 2, 6, 4, SyntaxClass::Function),
        ];
        let partition = syntax_partition(text, &spans);

        // architecture.md (Technology direction): overlap precedence applies styles
        // without changing source text.
        assert_eq!(
            segments(text, &partition),
            [
                ("α", TextStyle::Syntax(SyntaxClass::String)),
                ("call", TextStyle::Syntax(SyntaxClass::Function)),
                ("!", TextStyle::Syntax(SyntaxClass::String)),
            ]
        );
    }

    #[test]
    fn detail_presentation_suppresses_syntax_and_marks_unchanged_gaps() {
        let document = parse_patch("--- a/f\n+++ b/f\n@@ -1 +1 @@\n-αcall!\n+αcell!\n".as_bytes())
            .expect("valid diff");
        let rows = PageRows::of_file(&document.file_views().next().expect("first file"));
        let details = character_details(&rows);
        let addition = rows.line_index(4);
        let detail = details
            .get(addition)
            .as_ref()
            .expect("a changed pair has detail");
        let text = rows.line(addition).payload();

        let partition = character_partition(text, detail);

        let detail = |style| TextStyle::Detail(Side::New, style);
        assert_eq!(
            segments(text, &partition),
            [
                ("αc", detail(DetailStyle::Unchanged)),
                ("e", detail(DetailStyle::Changed)),
                ("ll!", detail(DetailStyle::Unchanged)),
            ]
        );
    }

    #[test]
    fn capture_precedence_retains_specificity_after_multiline_clipping() {
        let text = "word";
        let spans = [
            syntax(text, 0, 4, 4, SyntaxClass::Keyword),
            syntax(text, 0, 4, 20, SyntaxClass::Comment),
        ];
        let partition = syntax_partition(text, &spans);

        assert_eq!(
            segments(text, &partition),
            [("word", TextStyle::Syntax(SyntaxClass::Keyword))]
        );
    }

    #[test]
    fn text_without_annotations_takes_the_uncovered_style() {
        let partition = syntax_partition("same", &[]);

        assert_eq!(segments("same", &partition), [("same", TextStyle::Plain)]);
    }
}
