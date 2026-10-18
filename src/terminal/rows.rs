use ratatui::style::{Modifier, Style};

use crate::{
    document::RecordKind,
    layout::{LineIndex, PairIndex, ScrollExtent, SideBySideRow},
    preferences::DiffGranularity,
    prepared::PreparedPage,
    styling::{RowKind, StyledRow, TextStyle},
    theme::{RowPalette, Theme},
};

/// One pane's row: prepared text, or the blank peer that aligns a one-sided change.
#[derive(Clone, Copy)]
pub(super) enum PaneRow<'a> {
    Text(StyledRow<'a>),
    AlignmentPeer(RowKind),
}

impl PaneRow<'_> {
    pub(super) fn kind(&self) -> RowKind {
        match self {
            Self::Text(row) => row.kind,
            Self::AlignmentPeer(kind) => *kind,
        }
    }
}

impl<'a> SideBySideRow<StyledRow<'a>> {
    /// The before and after pane rows of this row; a one-sided change pairs with a blank
    /// peer of the opposite kind.
    pub(super) fn pane_pair(self) -> (PaneRow<'a>, PaneRow<'a>) {
        match self {
            Self::Shared(row) => (PaneRow::Text(row), PaneRow::Text(row)),
            Self::Paired { before, after } => (PaneRow::Text(before), PaneRow::Text(after)),
            Self::BeforeOnly(before) => (
                PaneRow::Text(before),
                PaneRow::AlignmentPeer(RowKind::Record(RecordKind::Addition)),
            ),
            Self::AfterOnly(after) => (
                PaneRow::AlignmentPeer(RowKind::Record(RecordKind::Deletion)),
                PaneRow::Text(after),
            ),
        }
    }
}

/// One screen line: line `line` of `row`.
pub(super) struct VisibleLine<R> {
    pub(super) row: R,
    pub(super) line: usize,
}

pub(super) fn visible_diff_rows<'a>(
    page: &'a PreparedPage,
    extent: &ScrollExtent<LineIndex>,
    first_line: usize,
    granularity: DiffGranularity,
    row_limit: usize,
) -> Vec<VisibleLine<StyledRow<'a>>> {
    extent
        .lines(first_line, row_limit)
        .map(|screen_line| VisibleLine {
            row: page.row(screen_line.row, granularity),
            line: screen_line.line,
        })
        .collect()
}

pub(super) fn visible_side_by_side_rows<'a>(
    page: &'a PreparedPage,
    extent: &ScrollExtent<PairIndex>,
    first_line: usize,
    granularity: DiffGranularity,
    row_limit: usize,
) -> Vec<VisibleLine<SideBySideRow<StyledRow<'a>>>> {
    let layout = page.layout();

    extent
        .lines(first_line, row_limit)
        .map(|screen_line| VisibleLine {
            row: layout
                .pair(screen_line.row)
                .map(|line| page.row(line, granularity)),
            line: screen_line.line,
        })
        .collect()
}

/// The two-cell marker gutter that precedes a row's payload.
pub(super) fn gutter(row_kind: RowKind, palette: &RowPalette) -> [(&'static str, Style); 2] {
    let spacing = (" ", Style::default());

    match row_kind {
        RowKind::Record(RecordKind::Addition) => {
            [("+", Style::default().fg(palette.addition_marker)), spacing]
        }
        RowKind::Record(RecordKind::Deletion) => {
            [("-", Style::default().fg(palette.deletion_marker)), spacing]
        }
        RowKind::Record(RecordKind::Context | RecordKind::Raw)
        | RowKind::Metadata
        | RowKind::HunkHeader => [spacing, spacing],
    }
}

pub(super) fn segment_style(row_kind: RowKind, text_style: TextStyle, theme: &Theme) -> Style {
    match text_style {
        TextStyle::Plain => Style::default(),
        TextStyle::Syntax(class) => Style::default().fg(theme.syntax.color(class)),
        TextStyle::Detail => detail_style(row_kind, &theme.rows),
        TextStyle::Unchanged => unchanged_style(row_kind, &theme.rows),
    }
}

fn detail_style(row_kind: RowKind, palette: &RowPalette) -> Style {
    let color = match row_kind {
        RowKind::Record(RecordKind::Addition) => palette.addition_marker,
        RowKind::Record(RecordKind::Deletion) => palette.deletion_marker,
        RowKind::Record(RecordKind::Context | RecordKind::Raw)
        | RowKind::HunkHeader
        | RowKind::Metadata => palette.context_body,
    };

    Style::default().fg(color).remove_modifier(Modifier::DIM)
}

fn unchanged_style(row_kind: RowKind, palette: &RowPalette) -> Style {
    let color = match row_kind {
        RowKind::Record(RecordKind::Addition) => palette.unchanged_addition,
        RowKind::Record(RecordKind::Deletion) => palette.unchanged_deletion,
        RowKind::Record(RecordKind::Context | RecordKind::Raw)
        | RowKind::HunkHeader
        | RowKind::Metadata => palette.context_body,
    };

    Style::default().fg(color)
}

pub(super) fn row_style(row_kind: RowKind, palette: &RowPalette) -> Style {
    match row_kind {
        RowKind::Record(RecordKind::Addition) => Style::default()
            .bg(palette.addition_background)
            .fg(palette.addition_body),
        RowKind::Record(RecordKind::Deletion) => Style::default()
            .bg(palette.deletion_background)
            .fg(palette.deletion_body)
            .add_modifier(Modifier::DIM),
        RowKind::Record(RecordKind::Context | RecordKind::Raw) => {
            Style::default().fg(palette.context_body)
        }
        RowKind::HunkHeader => Style::default().fg(palette.hunk_header),
        RowKind::Metadata => Style::default(),
    }
}
