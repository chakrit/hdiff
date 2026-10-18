use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{
    document::{RecordMap, Side},
    page_rows::{LineMap, PageRows},
    preferences::DiffGranularity,
    render::{LineKind, PayloadSpan, RenderedLine},
    syntax::{SyntaxClass, SyntaxSpan},
};

/// The styles of each `page_rows` line: the syntax spans of its record, looked up by
/// address, and its character detail when `details` gives it some.
pub fn row_styles(
    page_rows: &PageRows,
    syntax: &RecordMap<Vec<SyntaxSpan>>,
    details: &LineMap<Option<Box<[PayloadSpan]>>>,
) -> LineMap<RowStyles> {
    page_rows.map_lines(|index, line| {
        let text = line.payload();
        let spans: &[SyntaxSpan] = match line.kind() {
            LineKind::Record { address, .. } => syntax.get(address),
            LineKind::Text | LineKind::Metadata | LineKind::FileHeader | LineKind::HunkHeader => {
                &[]
            }
        };
        let character = details.get(index).as_deref();

        RowStyles {
            syntax: syntax_partition(text, spans),
            character: character.map(|spans| character_partition(text, spans)),
        }
    })
}

/// How a row is toned: a change to one side, a context or raw record, a hunk header, or
/// any other line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowTone {
    Changed(Side),
    Context,
    HunkHeader,
    Plain,
}

impl RowTone {
    fn of(kind: LineKind) -> Self {
        match kind {
            LineKind::Record { kind, .. } => {
                kind.changed_side().map_or(Self::Context, Self::Changed)
            }
            LineKind::HunkHeader => Self::HunkHeader,
            LineKind::Text | LineKind::Metadata | LineKind::FileHeader => Self::Plain,
        }
    }
}

#[derive(Clone, Copy)]
pub struct StyledRow<'a> {
    pub tone: RowTone,
    text: &'a str,
    partition: &'a Partition,
}

impl<'a> StyledRow<'a> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    Plain,
    Syntax(SyntaxClass),
    /// A changed span on a row with character detail.
    Detail,
    /// Text between the changed spans on a row with character detail.
    Unchanged,
}

/// A row's syntax partition, and its character-detail partition when it has detail.
#[derive(Debug, PartialEq, Eq)]
pub struct RowStyles {
    syntax: Partition,
    character: Option<Partition>,
}

impl RowStyles {
    /// `line` styled as `granularity` presents it; a row without character detail keeps
    /// its syntax styles.
    pub fn row<'a>(
        &'a self,
        line: &'a RenderedLine,
        granularity: DiffGranularity,
    ) -> StyledRow<'a> {
        let partition = match granularity {
            DiffGranularity::Line => &self.syntax,
            DiffGranularity::Character => self.character.as_ref().unwrap_or(&self.syntax),
        };

        StyledRow {
            tone: RowTone::of(line.kind()),
            text: line.payload(),
            partition,
        }
    }
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

fn character_partition(text: &str, details: &[PayloadSpan]) -> Partition {
    let annotations = details
        .iter()
        .map(|span| Annotation {
            span: *span,
            priority_length: span.len(),
            style: TextStyle::Detail,
        })
        .collect::<Vec<_>>();

    Partition::new(text, &annotations, TextStyle::Unchanged)
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
    /// Splits `text` at annotation edges; text no annotation covers takes `uncovered`.
    fn new(text: &str, annotations: &[Annotation], uncovered: TextStyle) -> Self {
        if annotations.is_empty() {
            return Self::Uniform(Boundary {
                end: text.len(),
                style: uncovered,
            });
        }

        let mut offsets = vec![0, text.len()];
        for annotation in annotations {
            let span = annotation.span;
            assert!(text.is_char_boundary(span.start()));
            assert!(text.is_char_boundary(span.end()));
            offsets.extend([span.start(), span.end()]);
        }
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
    use super::{RowTone, StyledRow, TextStyle, character_partition, syntax_partition};
    use crate::{
        document::Side,
        render::PayloadSpan,
        syntax::{SyntaxClass, SyntaxSpan},
    };

    #[test]
    fn chooses_specific_captures_and_later_ties_without_losing_outer_styles() {
        let text = "αcall!";
        let syntax = [
            SyntaxSpan {
                span: PayloadSpan::new(0, 7),
                capture_length: 7,
                class: SyntaxClass::String,
            },
            SyntaxSpan {
                span: PayloadSpan::new(2, 6),
                capture_length: 4,
                class: SyntaxClass::Property,
            },
            SyntaxSpan {
                span: PayloadSpan::new(2, 6),
                capture_length: 4,
                class: SyntaxClass::Function,
            },
        ];
        let partition = syntax_partition(text, &syntax);
        let row = StyledRow {
            tone: RowTone::Context,
            text,
            partition: &partition,
        };

        // architecture.md (Technology direction): overlap precedence applies styles
        // without changing source text.
        assert_eq!(
            row.segments().collect::<Vec<_>>(),
            [
                ("α", TextStyle::Syntax(SyntaxClass::String)),
                ("call", TextStyle::Syntax(SyntaxClass::Function)),
                ("!", TextStyle::Syntax(SyntaxClass::String)),
            ]
        );
    }

    #[test]
    fn detail_presentation_suppresses_syntax_and_marks_unchanged_gaps() {
        let text = "αcall!";
        let details = [PayloadSpan::new(2, 6)];
        let partition = character_partition(text, &details);
        let row = StyledRow {
            tone: RowTone::Changed(Side::New),
            text,
            partition: &partition,
        };

        assert_eq!(
            row.segments().collect::<Vec<_>>(),
            [
                ("α", TextStyle::Unchanged),
                ("call", TextStyle::Detail),
                ("!", TextStyle::Unchanged),
            ]
        );
    }

    #[test]
    fn capture_precedence_retains_specificity_after_multiline_clipping() {
        let text = "word";
        let syntax = [
            SyntaxSpan {
                span: PayloadSpan::new(0, 4),
                capture_length: 4,
                class: SyntaxClass::Keyword,
            },
            SyntaxSpan {
                span: PayloadSpan::new(0, 4),
                capture_length: 20,
                class: SyntaxClass::Comment,
            },
        ];
        let partition = syntax_partition(text, &syntax);
        let row = StyledRow {
            tone: RowTone::Context,
            text,
            partition: &partition,
        };

        assert_eq!(
            row.segments().collect::<Vec<_>>(),
            [("word", TextStyle::Syntax(SyntaxClass::Keyword)),]
        );
    }

    #[test]
    fn text_without_annotations_takes_the_uncovered_style() {
        let partition = character_partition("same", &[]);
        let row = StyledRow {
            tone: RowTone::Changed(Side::Old),
            text: "same",
            partition: &partition,
        };

        assert_eq!(
            row.segments().collect::<Vec<_>>(),
            [("same", TextStyle::Unchanged)]
        );
    }
}
