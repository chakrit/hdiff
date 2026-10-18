//! The terminal styles of a row's tone and text styles, drawn from the row palette.
//! Rules: `docs/spec/architecture.md` (View model).

use ratatui::style::{Modifier, Style};

use crate::{
    extent::MARKER_GUTTER_WIDTH,
    styling::{RowTone, TextStyle},
    theme::{RowPalette, Theme},
};

/// The marker gutter that precedes a row's payload: a change's marker, then spacing.
pub(super) fn gutter(
    tone: RowTone,
    palette: &RowPalette,
) -> [(&'static str, Style); MARKER_GUTTER_WIDTH] {
    let spacing = (" ", Style::default());

    match tone {
        RowTone::Changed(side) => {
            let marker = Style::default().fg(palette.change(side).marker);
            [(side.change().marker(), marker), spacing]
        }
        RowTone::Context | RowTone::HunkHeader | RowTone::Plain => [spacing, spacing],
    }
}

/// The style of the whole row, behind and under its text.
pub(super) fn row_style(tone: RowTone, palette: &RowPalette) -> Style {
    match tone {
        RowTone::Changed(side) => {
            let colors = palette.change(side);
            Style::default()
                .bg(colors.background)
                .fg(colors.body)
                .add_modifier(colors.body_modifier)
        }
        RowTone::Context => Style::default().fg(palette.context_body),
        RowTone::HunkHeader => Style::default().fg(palette.hunk_header),
        RowTone::Plain => Style::default(),
    }
}

/// The style of a text segment on a row of `tone`.
pub(super) fn segment_style(tone: RowTone, text_style: TextStyle, theme: &Theme) -> Style {
    let palette = &theme.rows;
    let (detail, unchanged) = match tone {
        RowTone::Changed(side) => {
            let colors = palette.change(side);
            (colors.marker, colors.unchanged)
        }
        RowTone::Context | RowTone::HunkHeader | RowTone::Plain => {
            (palette.context_body, palette.context_body)
        }
    };

    match text_style {
        TextStyle::Plain => Style::default(),
        TextStyle::Syntax(class) => Style::default().fg(theme.syntax.color(class)),
        TextStyle::Detail => Style::default().fg(detail).remove_modifier(Modifier::DIM),
        TextStyle::Unchanged => Style::default().fg(unchanged),
    }
}
