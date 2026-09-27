use ratatui::style::{Color, Modifier, Style};

use crate::{
    interaction::{DiffGranularity, Viewport},
    layout::{ScrollExtent, SideBySideRow},
    prepared::PreparedFile,
    styling::{RowKind, StyledRow, TextStyle},
    theme::DEFAULT_SYNTAX_THEME,
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

pub(super) fn visible_diff_rows<'a>(
    file: &'a PreparedFile,
    extent: &ScrollExtent,
    viewport: &Viewport,
    granularity: DiffGranularity,
    row_limit: usize,
) -> Vec<StyledRow<'a>> {
    extent
        .rows(viewport.vertical_offset, row_limit)
        .map(|index| file.row(index, granularity))
        .collect()
}

pub(super) fn visible_side_by_side_rows<'a>(
    file: &'a PreparedFile,
    extent: &ScrollExtent,
    viewport: &Viewport,
    granularity: DiffGranularity,
    row_limit: usize,
) -> Vec<VisibleSideBySideRow<'a>> {
    let side_by_side_rows = &file.layout().side_by_side_rows;

    extent
        .rows(viewport.vertical_offset, row_limit)
        .map(|index| match &side_by_side_rows[index] {
            SideBySideRow::Shared(index) => {
                VisibleSideBySideRow::Shared(file.row(*index, granularity))
            }
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
        })
        .collect()
}

/// The two-cell marker gutter that precedes a row's payload.
pub(super) fn gutter(row_kind: RowKind, color_count: u16) -> [(&'static str, Style); 2] {
    let palette = low_contrast_palette(color_count);
    let spacing = (" ", Style::default());

    match row_kind {
        RowKind::Addition => [("+", Style::default().fg(palette.addition_marker)), spacing],
        RowKind::Deletion => [("-", Style::default().fg(palette.deletion_marker)), spacing],
        RowKind::Context | RowKind::Raw | RowKind::Metadata | RowKind::HunkHeader => {
            [spacing, spacing]
        }
    }
}

pub(super) fn segment_style(row_kind: RowKind, text_style: TextStyle, color_count: u16) -> Style {
    match text_style {
        TextStyle::Plain => Style::default(),
        TextStyle::Syntax(class) => {
            Style::default().fg(DEFAULT_SYNTAX_THEME.color(class, color_count))
        }
        TextStyle::Detail => detail_style(row_kind, low_contrast_palette(color_count)),
    }
}

fn detail_style(row_kind: RowKind, palette: LowContrastPalette) -> Style {
    let color = match row_kind {
        RowKind::Addition => palette.addition_marker,
        RowKind::Deletion => palette.deletion_marker,
        RowKind::Context | RowKind::Raw | RowKind::HunkHeader | RowKind::Metadata => {
            palette.context_body
        }
    };

    Style::default().fg(color).add_modifier(Modifier::DIM)
}

pub(super) fn row_style(row_kind: RowKind, color_count: u16) -> Style {
    let palette = low_contrast_palette(color_count);

    match row_kind {
        RowKind::Addition => Style::default()
            .bg(palette.addition_background)
            .fg(palette.addition_body),
        RowKind::Deletion => Style::default()
            .bg(palette.deletion_background)
            .fg(palette.deletion_body)
            .add_modifier(Modifier::DIM),
        RowKind::Context | RowKind::Raw => Style::default().fg(palette.context_body),
        RowKind::HunkHeader => Style::default().fg(hunk_header_color(color_count)),
        RowKind::Metadata => Style::default(),
    }
}

fn hunk_header_color(color_count: u16) -> Color {
    match color_count {
        u16::MAX => Color::Rgb(100, 155, 175),
        256.. => Color::Indexed(109),
        _ => Color::Cyan,
    }
}

#[derive(Clone, Copy)]
struct LowContrastPalette {
    addition_background: Color,
    addition_body: Color,
    deletion_background: Color,
    deletion_body: Color,
    context_body: Color,
    deletion_marker: Color,
    addition_marker: Color,
}

fn low_contrast_palette(color_count: u16) -> LowContrastPalette {
    match color_count {
        u16::MAX => LowContrastPalette {
            addition_background: Color::Rgb(35, 73, 43),
            addition_body: Color::Rgb(220, 235, 220),
            deletion_background: Color::Rgb(50, 30, 30),
            deletion_body: Color::Rgb(150, 150, 150),
            context_body: Color::Rgb(180, 180, 180),
            deletion_marker: Color::Rgb(206, 74, 74),
            addition_marker: Color::Rgb(112, 195, 115),
        },
        256.. => LowContrastPalette {
            addition_background: Color::Indexed(22),
            addition_body: Color::Indexed(255),
            deletion_background: Color::Indexed(52),
            deletion_body: Color::Indexed(245),
            context_body: Color::Indexed(250),
            deletion_marker: Color::Indexed(167),
            addition_marker: Color::Indexed(71),
        },
        _ => LowContrastPalette {
            addition_background: Color::Green,
            addition_body: Color::White,
            deletion_background: Color::Red,
            deletion_body: Color::DarkGray,
            context_body: Color::Gray,
            deletion_marker: Color::Red,
            addition_marker: Color::Green,
        },
    }
}

pub(super) fn inner_separator_color(color_count: u16) -> Color {
    match color_count {
        u16::MAX => Color::Rgb(95, 95, 95),
        256.. => Color::Indexed(238),
        _ => Color::DarkGray,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::{hunk_header_color, low_contrast_palette};

    #[test]
    fn falls_back_from_truecolor_to_256_and_basic_low_contrast_colors() {
        assert_eq!(
            low_contrast_palette(u16::MAX).addition_background,
            Color::Rgb(35, 73, 43)
        );
        assert_eq!(
            low_contrast_palette(256).addition_marker,
            Color::Indexed(71)
        );
        assert_eq!(low_contrast_palette(8).deletion_marker, Color::Red);
    }

    #[test]
    fn falls_back_from_truecolor_to_256_and_basic_hunk_header_colors() {
        assert_eq!(hunk_header_color(u16::MAX), Color::Rgb(100, 155, 175));
        assert_eq!(hunk_header_color(256), Color::Indexed(109));
        assert_eq!(hunk_header_color(8), Color::Cyan);
    }
}
