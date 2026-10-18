use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{
    detail::RowDetail,
    document::{RecordKind, RecordMap},
    layout::Layout,
    preferences::DiffGranularity,
    render::{LineKind, PayloadSpan, RenderedLine},
    syntax::{SyntaxClass, SyntaxSpan},
};

/// The styles of each `layout` row: syntax spans of its record, looked up by address, and
/// the character detail of each row `details` names.
pub fn row_styles(
    layout: &Layout,
    syntax: &RecordMap<Vec<SyntaxSpan>>,
    details: Vec<RowDetail>,
) -> Box<[RowStyles]> {
    let mut rows = layout
        .lines()
        .iter()
        .map(|line| {
            let spans: &[SyntaxSpan] = match line.kind() {
                LineKind::Record { address, .. } => syntax.get(address),
                LineKind::Text
                | LineKind::Metadata
                | LineKind::FileHeader
                | LineKind::HunkHeader => &[],
            };
            RowStyles::syntax(line.payload(), spans)
        })
        .collect::<Vec<_>>();

    for detail in details {
        let text = layout.line(detail.row).payload();
        rows[detail.row.get()].character = Some(character_partition(text, &detail.spans));
    }
    rows.into_boxed_slice()
}

/// What a row's styling distinguishes: the record kind, a hunk header, or any other line.
#[derive(Clone, Copy)]
pub enum RowKind {
    Record(RecordKind),
    HunkHeader,
    Metadata,
}

impl RowKind {
    fn of(kind: LineKind) -> Self {
        match kind {
            LineKind::Record { kind, .. } => Self::Record(kind),
            LineKind::HunkHeader => Self::HunkHeader,
            LineKind::Text | LineKind::Metadata | LineKind::FileHeader => Self::Metadata,
        }
    }
}

#[derive(Clone, Copy)]
pub struct StyledRow<'a> {
    pub kind: RowKind,
    text: &'a str,
    partition: &'a Partition,
}

impl<'a> StyledRow<'a> {
    pub fn segments(&self) -> impl Iterator<Item = (&'a str, TextStyle)> + use<'a> {
        let mut remaining = self.text;
        let mut offset = 0;
        let mut boundaries = self.partition.0.iter();
        std::iter::from_fn(move || {
            let Some(boundary) = boundaries.next() else {
                return (!remaining.is_empty())
                    .then(|| (std::mem::take(&mut remaining), TextStyle::Plain));
            };
            let (text, rest) = remaining.split_at(boundary.end - offset);
            remaining = rest;
            offset = boundary.end;
            Some((text, boundary.style))
        })
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
    fn syntax(text: &str, syntax: &[SyntaxSpan]) -> Self {
        let annotations = syntax
            .iter()
            .map(|span| Annotation {
                span: span.span,
                priority_length: span.capture_length,
                style: TextStyle::Syntax(span.class),
            })
            .collect::<Vec<_>>();

        Self {
            syntax: Partition::new(text, &annotations, TextStyle::Plain),
            character: None,
        }
    }

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
            kind: RowKind::of(line.kind()),
            text: line.payload(),
            partition,
        }
    }
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

#[derive(Debug, PartialEq, Eq)]
struct Partition(Box<[Boundary]>);

#[derive(Debug, PartialEq, Eq)]
struct Boundary {
    end: usize,
    style: TextStyle,
}

impl Partition {
    /// Splits `text` at annotation edges; text no annotation covers takes `uncovered`.
    fn new(text: &str, annotations: &[Annotation], uncovered: TextStyle) -> Self {
        if annotations.is_empty() {
            return Self(Box::new([]));
        }

        let mut offsets = vec![0, text.len()];
        for annotation in annotations {
            assert!(annotation.span.start <= annotation.span.end);
            assert!(text.is_char_boundary(annotation.span.start));
            assert!(text.is_char_boundary(annotation.span.end));
            offsets.extend([annotation.span.start, annotation.span.end]);
        }
        offsets.sort_unstable();
        offsets.dedup();

        let mut starts = (0..annotations.len()).collect::<Vec<_>>();
        starts.sort_unstable_by_key(|index| annotations[*index].span.start);
        let mut starts = starts.into_iter().peekable();
        let mut active = BinaryHeap::new();
        let mut boundaries: Vec<Boundary> = Vec::new();

        for interval in offsets.windows(2) {
            let [start, end] = [interval[0], interval[1]];
            while let Some(index) = starts.next_if(|index| annotations[*index].span.start <= start)
            {
                let annotation = &annotations[index];
                active.push((Reverse(annotation.priority_length), index));
            }
            while let Some((_, index)) = active.peek()
                && annotations[*index].span.end <= start
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

        Self(boundaries.into_boxed_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::{RowKind, RowStyles, StyledRow, TextStyle, character_partition};
    use crate::{
        document::RecordKind,
        render::PayloadSpan,
        syntax::{SyntaxClass, SyntaxSpan},
    };

    #[test]
    fn chooses_specific_captures_and_later_ties_without_losing_outer_styles() {
        let text = "αcall!";
        let syntax = [
            SyntaxSpan {
                span: PayloadSpan { start: 0, end: 7 },
                capture_length: 7,
                class: SyntaxClass::String,
            },
            SyntaxSpan {
                span: PayloadSpan { start: 2, end: 6 },
                capture_length: 4,
                class: SyntaxClass::Property,
            },
            SyntaxSpan {
                span: PayloadSpan { start: 2, end: 6 },
                capture_length: 4,
                class: SyntaxClass::Function,
            },
        ];
        let styles = RowStyles::syntax(text, &syntax);
        let row = StyledRow {
            kind: RowKind::Record(RecordKind::Context),
            text,
            partition: &styles.syntax,
        };

        // roadmap.md: overlap precedence applies styles without changing source text.
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
        let syntax = [SyntaxSpan {
            span: PayloadSpan { start: 0, end: 7 },
            capture_length: 7,
            class: SyntaxClass::String,
        }];
        let details = [PayloadSpan { start: 2, end: 6 }];
        let mut styles = RowStyles::syntax(text, &syntax);
        styles.character = Some(character_partition(text, &details));
        let row = StyledRow {
            kind: RowKind::Record(RecordKind::Addition),
            text,
            partition: styles.character.as_ref().expect("character presentation"),
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
                span: PayloadSpan { start: 0, end: 4 },
                capture_length: 4,
                class: SyntaxClass::Keyword,
            },
            SyntaxSpan {
                span: PayloadSpan { start: 0, end: 4 },
                capture_length: 20,
                class: SyntaxClass::Comment,
            },
        ];
        let styles = RowStyles::syntax(text, &syntax);
        let row = StyledRow {
            kind: RowKind::Record(RecordKind::Context),
            text,
            partition: &styles.syntax,
        };

        assert_eq!(
            row.segments().collect::<Vec<_>>(),
            [("word", TextStyle::Syntax(SyntaxClass::Keyword)),]
        );
    }
}
