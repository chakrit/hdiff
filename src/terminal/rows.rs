use ratatui::style::{Modifier, Style};

use crate::{
    interaction::Viewport,
    layout::{ScrollExtent, SideBySideRow},
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

pub(super) fn aligned_pair<'a>(
    before: Option<StyledRow<'a>>,
    after: Option<StyledRow<'a>>,
) -> Option<(PaneRow<'a>, PaneRow<'a>)> {
    match (before, after) {
        (Some(before), Some(after)) => Some((PaneRow::Text(before), PaneRow::Text(after))),
        (Some(before), None) => Some((
            PaneRow::Text(before),
            PaneRow::AlignmentPeer(RowKind::Addition),
        )),
        (None, Some(after)) => Some((
            PaneRow::AlignmentPeer(RowKind::Deletion),
            PaneRow::Text(after),
        )),
        (None, None) => None,
    }
}

pub(super) enum VisibleSideBySideRow<'a> {
    Shared(StyledRow<'a>),
    Paired {
        before: Option<StyledRow<'a>>,
        after: Option<StyledRow<'a>>,
    },
}

/// One screen line: line `line` of `row`.
pub(super) struct VisibleLine<R> {
    pub(super) row: R,
    pub(super) line: usize,
}

pub(super) fn visible_diff_rows<'a>(
    file: &'a PreparedPage,
    extent: &ScrollExtent,
    viewport: &Viewport,
    granularity: DiffGranularity,
    row_limit: usize,
) -> Vec<VisibleLine<StyledRow<'a>>> {
    extent
        .lines(viewport.vertical_offset, row_limit)
        .map(|screen_line| VisibleLine {
            row: file.row(screen_line.row, granularity),
            line: screen_line.line,
        })
        .collect()
}

pub(super) fn visible_side_by_side_rows<'a>(
    file: &'a PreparedPage,
    extent: &ScrollExtent,
    viewport: &Viewport,
    granularity: DiffGranularity,
    row_limit: usize,
) -> Vec<VisibleLine<VisibleSideBySideRow<'a>>> {
    let side_by_side_rows = &file.layout().side_by_side_rows;
    let side_by_side_row = |index| match &side_by_side_rows[index] {
        SideBySideRow::Shared(index) => VisibleSideBySideRow::Shared(file.row(*index, granularity)),
        SideBySideRow::Paired { before, after } => VisibleSideBySideRow::Paired {
            before: Some(file.row(*before, granularity)),
            after: Some(file.row(*after, granularity)),
        },
        SideBySideRow::BeforeOnly(index) => VisibleSideBySideRow::Paired {
            before: Some(file.row(*index, granularity)),
            after: None,
        },
        SideBySideRow::AfterOnly(index) => VisibleSideBySideRow::Paired {
            before: None,
            after: Some(file.row(*index, granularity)),
        },
    };

    extent
        .lines(viewport.vertical_offset, row_limit)
        .map(|screen_line| VisibleLine {
            row: side_by_side_row(screen_line.row),
            line: screen_line.line,
        })
        .collect()
}

/// The two-cell marker gutter that precedes a row's payload.
pub(super) fn gutter(row_kind: RowKind, palette: &RowPalette) -> [(&'static str, Style); 2] {
    let spacing = (" ", Style::default());

    match row_kind {
        RowKind::Addition => [("+", Style::default().fg(palette.addition_marker)), spacing],
        RowKind::Deletion => [("-", Style::default().fg(palette.deletion_marker)), spacing],
        RowKind::Context | RowKind::Raw | RowKind::Metadata | RowKind::HunkHeader => {
            [spacing, spacing]
        }
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
        RowKind::Addition => palette.addition_marker,
        RowKind::Deletion => palette.deletion_marker,
        RowKind::Context | RowKind::Raw | RowKind::HunkHeader | RowKind::Metadata => {
            palette.context_body
        }
    };

    Style::default().fg(color).remove_modifier(Modifier::DIM)
}

fn unchanged_style(row_kind: RowKind, palette: &RowPalette) -> Style {
    let color = match row_kind {
        RowKind::Addition => palette.unchanged_addition,
        RowKind::Deletion => palette.unchanged_deletion,
        RowKind::Context | RowKind::Raw | RowKind::HunkHeader | RowKind::Metadata => {
            palette.context_body
        }
    };

    Style::default().fg(color)
}

pub(super) fn row_style(row_kind: RowKind, palette: &RowPalette) -> Style {
    match row_kind {
        RowKind::Addition => Style::default()
            .bg(palette.addition_background)
            .fg(palette.addition_body),
        RowKind::Deletion => Style::default()
            .bg(palette.deletion_background)
            .fg(palette.deletion_body)
            .add_modifier(Modifier::DIM),
        RowKind::Context | RowKind::Raw => Style::default().fg(palette.context_body),
        RowKind::HunkHeader => Style::default().fg(palette.hunk_header),
        RowKind::Metadata => Style::default(),
    }
}
