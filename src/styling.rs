use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{
    detail::DetailSpan,
    document::RecordKind,
    layout::Layout,
    preferences::DiffGranularity,
    render::LineKind,
    syntax::{SyntaxClass, SyntaxSpan},
};

#[derive(Debug, PartialEq, Eq)]
pub struct StyledLayout {
    layout: Layout,
    rows: Box<[RowStyles]>,
}

impl StyledLayout {
    pub fn new(layout: Layout, syntax: &[Vec<SyntaxSpan>], details: &[Vec<DetailSpan>]) -> Self {
        assert_eq!(layout.diff_lines.len(), syntax.len());
        assert_eq!(layout.diff_lines.len(), details.len());
        let rows = layout
            .diff_lines
            .iter()
            .zip(syntax)
            .zip(details)
            .map(|((line, syntax), details)| RowStyles::new(line.payload(), syntax, details))
            .collect();

        Self { layout, rows }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn row(&self, index: usize, granularity: DiffGranularity) -> StyledRow<'_> {
        let line = &self.layout.diff_lines[index];
        let styles = &self.rows[index];
        let partition = match granularity {
            DiffGranularity::Line => &styles.syntax,
            DiffGranularity::Character => styles.character.as_ref().unwrap_or(&styles.syntax),
        };

        StyledRow {
            kind: RowKind::of(line.kind()),
            text: line.payload(),
            partition,
        }
    }
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

#[derive(Debug, PartialEq, Eq)]
struct RowStyles {
    syntax: Partition,
    character: Option<Partition>,
}

impl RowStyles {
    fn new(text: &str, syntax: &[SyntaxSpan], details: &[DetailSpan]) -> Self {
        let syntax = syntax
            .iter()
            .map(|span| Annotation {
                start: span.start,
                end: span.end,
                priority_length: span.capture_length,
                style: TextStyle::Syntax(span.class),
            })
            .collect::<Vec<_>>();
        let character = (!details.is_empty()).then(|| {
            let annotations = details
                .iter()
                .map(|span| Annotation {
                    start: span.start,
                    end: span.end,
                    priority_length: span.end - span.start,
                    style: TextStyle::Detail,
                })
                .collect::<Vec<_>>();
            Partition::new(text, &annotations, TextStyle::Unchanged)
        });

        Self {
            syntax: Partition::new(text, &syntax, TextStyle::Plain),
            character,
        }
    }
}

struct Annotation {
    start: usize,
    end: usize,
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
            assert!(annotation.start <= annotation.end);
            assert!(text.is_char_boundary(annotation.start));
            assert!(text.is_char_boundary(annotation.end));
            offsets.extend([annotation.start, annotation.end]);
        }
        offsets.sort_unstable();
        offsets.dedup();
        let mut starts = (0..annotations.len()).collect::<Vec<_>>();
        starts.sort_unstable_by_key(|index| annotations[*index].start);
        let mut starts = starts.into_iter().peekable();
        let mut active = BinaryHeap::new();
        let mut boundaries: Vec<Boundary> = Vec::new();

        for interval in offsets.windows(2) {
            let [start, end] = [interval[0], interval[1]];
            while let Some(index) = starts.next_if(|index| annotations[*index].start <= start) {
                let annotation = &annotations[index];
                active.push((Reverse(annotation.priority_length), index));
            }
            while let Some((_, index)) = active.peek()
                && annotations[*index].end <= start
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
    use super::{RowKind, RowStyles, StyledRow, TextStyle};
    use crate::{
        detail::DetailSpan,
        document::RecordKind,
        syntax::{SyntaxClass, SyntaxSpan},
    };

    #[test]
    fn chooses_specific_captures_and_later_ties_without_losing_outer_styles() {
        let text = "αcall!";
        let syntax = [
            SyntaxSpan {
                start: 0,
                end: 7,
                capture_length: 7,
                class: SyntaxClass::String,
            },
            SyntaxSpan {
                start: 2,
                end: 6,
                capture_length: 4,
                class: SyntaxClass::Property,
            },
            SyntaxSpan {
                start: 2,
                end: 6,
                capture_length: 4,
                class: SyntaxClass::Function,
            },
        ];
        let styles = RowStyles::new(text, &syntax, &[]);
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
            start: 0,
            end: 7,
            capture_length: 7,
            class: SyntaxClass::String,
        }];
        let details = [DetailSpan { start: 2, end: 6 }];
        let styles = RowStyles::new(text, &syntax, &details);
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
                start: 0,
                end: 4,
                capture_length: 4,
                class: SyntaxClass::Keyword,
            },
            SyntaxSpan {
                start: 0,
                end: 4,
                capture_length: 20,
                class: SyntaxClass::Comment,
            },
        ];
        let styles = RowStyles::new(text, &syntax, &[]);
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
