//! The terminal styles of a row's tone and text styles, drawn from the row palette.
//! Rules: docs/spec/architecture.md#palette-and-character-detail.

use ratatui::style::{Modifier, Style};

use crate::{
    document::RecordKind,
    screen::MARKER_GUTTER_CLUSTERS,
    styling::{DetailStyle, RowTone, StyledText, TextStyle},
    theme::{RowPalette, Theme},
};

/// The marker gutter that precedes a row's payload: a change's marker, then spacing.
pub(super) fn gutter(
    tone: RowTone,
    palette: &RowPalette,
) -> [StyledText<'static, Style>; MARKER_GUTTER_CLUSTERS] {
    let spacing = StyledText {
        text: " ",
        style: Style::default(),
    };

    match tone {
        RowTone::Changed(side) => {
            let marker = StyledText {
                text: RecordKind::change_of(side).marker(),
                style: Style::default().fg(palette.change.get(side).marker),
            };
            [marker, spacing]
        }
        RowTone::UnchangedRecord | RowTone::HunkHeader | RowTone::Plain => [spacing, spacing],
    }
}

/// The style of the whole row, behind and under its text.
pub(super) fn row_style(tone: RowTone, palette: &RowPalette) -> Style {
    match tone {
        RowTone::Changed(side) => {
            let colors = palette.change.get(side);
            Style::default()
                .bg(colors.background)
                .fg(colors.body)
                .add_modifier(colors.body_modifier)
        }
        RowTone::UnchangedRecord => Style::default().fg(palette.context_body),
        RowTone::HunkHeader => Style::default().fg(palette.hunk_header),
        RowTone::Plain => Style::default(),
    }
}

/// The style of a text segment, over its row's style.
pub(super) fn segment_style(text_style: TextStyle, theme: &Theme) -> Style {
    match text_style {
        TextStyle::Plain => Style::default(),
        TextStyle::Syntax(class) => Style::default().fg(theme.syntax.color(class)),
        TextStyle::Detail(side, DetailStyle::Changed) => Style::default()
            .fg(theme.rows.change.get(side).marker)
            .remove_modifier(Modifier::DIM),
        TextStyle::Detail(side, DetailStyle::Unchanged) => {
            Style::default().fg(theme.rows.change.get(side).unchanged)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        screen::Cells,
        side::Side,
        theme::{ColorDepth, Theme},
    };

    /// The gutter's cell width is its cluster count, so every cluster it draws covers
    /// one cell.
    #[test]
    fn draws_every_gutter_cluster_in_one_cell() {
        let palette = &Theme::for_depth(ColorDepth::TrueColor).rows;
        let tones = Side::BOTH.map(RowTone::Changed).into_iter().chain([
            RowTone::UnchangedRecord,
            RowTone::HunkHeader,
            RowTone::Plain,
        ]);

        for tone in tones {
            for cluster in gutter(tone, palette) {
                assert_eq!(
                    Cells::of_text(cluster.text),
                    Cells::ONE,
                    "gutter cluster {:?}",
                    cluster.text
                );
            }
        }
    }
}
