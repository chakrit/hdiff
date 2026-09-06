use ratatui::{
    Frame,
    layout::{Constraint, Layout as RatatuiLayout},
    style::{Color, Style},
    text::{Line, Text},
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::{
    interaction::{DiffLayout, Interaction},
    layout::{FileListRow, PaneLayout, pane_layout},
    prepared::PreparedFile,
};

#[cfg(test)]
use crate::{
    document::DiffDocument,
    interaction::{ViewPreferences, Viewport},
    layout::Layout,
    prepared::PreparedDocument,
};

use super::rows::{
    DiffRow, VisibleSideBySideRow, aligned_pair, inner_separator_color, visible_diff_rows,
    visible_side_by_side_rows,
};

const MINIMUM_FILE_LIST_HEIGHT: u16 = 3;

struct DisplayRows {
    unified: Vec<DiffRow>,
    side_by_side: Vec<VisibleSideBySideRow>,
    layout: DiffLayout,
    horizontal_offset: u16,
    color_count: u16,
}

#[cfg(test)]
fn render_frame(
    frame: &mut Frame,
    document: &DiffDocument,
    layout: &Layout,
    viewport: &Viewport,
    color_count: u16,
) {
    let preferences = ViewPreferences::line(DiffLayout::Unified);

    render_frame_with_layout(frame, document, layout, viewport, &preferences, color_count);
}

#[cfg(test)]
fn render_frame_with_layout(
    frame: &mut Frame,
    document: &DiffDocument,
    layout: &Layout,
    viewport: &Viewport,
    preferences: &ViewPreferences,
    color_count: u16,
) {
    let selected = layout
        .files
        .iter()
        .position(|file| file.selected)
        .expect("rendering fixture selects a file");
    let prepared = PreparedDocument::prepare(document);
    let file = prepared.file(selected).expect("selected prepared file");
    let interaction = Interaction {
        selected_file: selected,
        preferences: ViewPreferences {
            layout: preferences.layout,
            granularity: preferences.granularity,
            context_lines: preferences.context_lines,
        },
        viewport: viewport.clone(),
    };

    render_prepared_frame(frame, &layout.files, file, &interaction, color_count);
}

pub(super) fn render_prepared_frame(
    frame: &mut Frame,
    files: &[FileListRow],
    file: &PreparedFile,
    interaction: &Interaction,
    color_count: u16,
) {
    let area = frame.area();
    let layout = &file.layout;
    let details = file.details(interaction.preferences.granularity);
    let rows = visible_diff_rows(
        layout,
        &interaction.viewport,
        file.syntax(),
        details,
        interaction.preferences.context_lines,
        color_count,
    );
    let side_by_side_rows = visible_side_by_side_rows(
        layout,
        &interaction.viewport,
        file.syntax(),
        details,
        interaction.preferences.context_lines,
        color_count,
    );
    let display = DisplayRows {
        unified: rows,
        side_by_side: side_by_side_rows,
        layout: interaction.preferences.layout,
        horizontal_offset: interaction.viewport.horizontal_offset,
        color_count,
    };

    match pane_layout(area.width, area.height) {
        PaneLayout::TooNarrow => frame.render_widget(Paragraph::new("screen too narrow"), area),
        PaneLayout::TooShort => frame.render_widget(Paragraph::new("screen too short"), area),
        PaneLayout::DiffOnly => render_display_layout(frame, display, area),
        PaneLayout::Split {
            file_list_width,
            separator_padding,
        } => render_split_panes(
            frame,
            files,
            area,
            file_list_width,
            separator_padding,
            display,
        ),
    }
}

fn render_split_panes(
    frame: &mut Frame,
    files: &[FileListRow],
    area: ratatui::layout::Rect,
    file_list_width: u16,
    separator_padding: u16,
    display: DisplayRows,
) {
    let panes = RatatuiLayout::horizontal([
        Constraint::Length(file_list_width),
        Constraint::Length(separator_padding),
        Constraint::Length(1),
        Constraint::Length(separator_padding),
        Constraint::Min(1),
    ])
    .split(area);
    let hints = footer_hints();
    let footer_height = area
        .height
        .saturating_sub(MINIMUM_FILE_LIST_HEIGHT)
        .min(hints.len() as u16);
    let file_list_areas = RatatuiLayout::vertical([
        Constraint::Min(MINIMUM_FILE_LIST_HEIGHT),
        Constraint::Length(footer_height),
    ])
    .split(panes[0]);
    let mut selected_file = ListState::default();
    selected_file.select(files.iter().position(|file| file.selected));
    let chrome = chrome_palette(display.color_count);
    let files = files
        .iter()
        .map(|file| {
            ListItem::new(Line::styled(
                file.label.as_str(),
                Style::default().fg(chrome.file_list),
            ))
        })
        .collect::<Vec<_>>();
    let file_list = List::new(files).highlight_symbol("· ").highlight_style(
        Style::default()
            .fg(chrome.selected_file)
            .bg(chrome.selected_file_background),
    );
    let separator = Text::from(
        (0..area.height)
            .map(|_| Line::styled("│", Style::default().fg(chrome.separator)))
            .collect::<Vec<_>>(),
    );
    let hints = Paragraph::new(hints).style(Style::default().fg(chrome.footer));

    frame.render_stateful_widget(file_list, file_list_areas[0], &mut selected_file);
    frame.render_widget(hints, file_list_areas[1]);
    frame.render_widget(Paragraph::new(separator), panes[2]);
    render_display_layout(frame, display, panes[4]);
}

fn render_diff_rows(
    frame: &mut Frame,
    rows: Vec<DiffRow>,
    area: ratatui::layout::Rect,
    horizontal_offset: u16,
) {
    for (index, row) in rows.into_iter().take(area.height.into()).enumerate() {
        let row_area = ratatui::layout::Rect::new(area.x, area.y + index as u16, area.width, 1);

        frame.render_widget(
            Paragraph::new(row.line)
                .style(row.style)
                .scroll((0, horizontal_offset)),
            row_area,
        );
    }
}

fn render_display_layout(frame: &mut Frame, display: DisplayRows, area: ratatui::layout::Rect) {
    match display.layout {
        DiffLayout::Unified => {
            render_diff_rows(frame, display.unified, area, display.horizontal_offset)
        }
        DiffLayout::Vertical => render_vertical_rows(
            frame,
            display.side_by_side,
            area,
            display.horizontal_offset,
            display.color_count,
        ),
        DiffLayout::Stacked => render_stacked_rows(
            frame,
            display.side_by_side,
            area,
            display.horizontal_offset,
            display.color_count,
        ),
    }
}

fn render_vertical_rows(
    frame: &mut Frame,
    rows: Vec<VisibleSideBySideRow>,
    area: ratatui::layout::Rect,
    horizontal_offset: u16,
    color_count: u16,
) {
    let panes = RatatuiLayout::horizontal([
        Constraint::Percentage(50),
        Constraint::Length(1),
        Constraint::Percentage(50),
    ])
    .split(area);
    let separator_style = Style::default().fg(inner_separator_color(color_count));

    for (index, row) in rows.into_iter().take(area.height.into()).enumerate() {
        let row_area = ratatui::layout::Rect::new(area.x, area.y + index as u16, area.width, 1);
        let before_area = ratatui::layout::Rect::new(panes[0].x, row_area.y, panes[0].width, 1);
        let separator_area = ratatui::layout::Rect::new(panes[1].x, row_area.y, 1, 1);
        let after_area = ratatui::layout::Rect::new(panes[2].x, row_area.y, panes[2].width, 1);
        match row {
            VisibleSideBySideRow::Shared(row) => {
                render_diff_rows(frame, vec![row.clone()], before_area, horizontal_offset);
                frame.render_widget(
                    Paragraph::new(Line::styled("│", separator_style)),
                    separator_area,
                );
                render_diff_rows(frame, vec![row], after_area, horizontal_offset);
            }
            VisibleSideBySideRow::Paired { before, after } => {
                let Some((before, after)) = aligned_pair(before, after, color_count) else {
                    continue;
                };

                render_diff_rows(frame, vec![before], before_area, horizontal_offset);
                frame.render_widget(
                    Paragraph::new(Line::styled("│", separator_style)),
                    separator_area,
                );
                render_diff_rows(frame, vec![after], after_area, horizontal_offset);
            }
        }
    }
}

fn render_stacked_rows(
    frame: &mut Frame,
    rows: Vec<VisibleSideBySideRow>,
    area: ratatui::layout::Rect,
    horizontal_offset: u16,
    color_count: u16,
) {
    let panes = RatatuiLayout::vertical([
        Constraint::Percentage(50),
        Constraint::Length(1),
        Constraint::Percentage(50),
    ])
    .split(area);
    let mut before_rows = Vec::new();
    let mut after_rows = Vec::new();

    for row in rows {
        match row {
            VisibleSideBySideRow::Shared(row) => {
                before_rows.push(row.clone());
                after_rows.push(row);
            }
            VisibleSideBySideRow::Paired { before, after } => {
                let Some((before, after)) = aligned_pair(before, after, color_count) else {
                    continue;
                };

                before_rows.push(before);
                after_rows.push(after);
            }
        }
    }

    let separator = "─".repeat(usize::from(area.width));
    frame.render_widget(
        Paragraph::new(Line::styled(
            separator,
            Style::default().fg(inner_separator_color(color_count)),
        )),
        panes[1],
    );
    render_diff_rows(frame, before_rows, panes[0], horizontal_offset);
    render_diff_rows(frame, after_rows, panes[2], horizontal_offset);
}

fn footer_hints() -> Vec<Line<'static>> {
    vec![
        Line::raw("   k"),
        Line::raw(" h · l  movement"),
        Line::raw("   j"),
        Line::raw(""),
        Line::raw("  ^U    page up"),
        Line::raw("  ^D    page down"),
        Line::raw("  g·G   top/bottom"),
        Line::raw("  v    cycle layout"),
        Line::raw("  c    character detail"),
        Line::raw(" +/-   context"),
        Line::raw("(⇧)Tab  next/prev file"),
        Line::raw("   q    exit"),
    ]
}

#[derive(Clone, Copy)]
struct ChromePalette {
    file_list: Color,
    selected_file: Color,
    selected_file_background: Color,
    separator: Color,
    footer: Color,
}

fn chrome_palette(color_count: u16) -> ChromePalette {
    match color_count {
        u16::MAX => ChromePalette {
            file_list: Color::Rgb(160, 160, 160),
            selected_file: Color::Rgb(220, 220, 220),
            selected_file_background: Color::Rgb(65, 65, 65),
            separator: Color::Rgb(125, 125, 125),
            footer: Color::Rgb(110, 110, 110),
        },
        256.. => ChromePalette {
            file_list: Color::Indexed(246),
            selected_file: Color::Indexed(255),
            selected_file_background: Color::Indexed(238),
            separator: Color::Indexed(243),
            footer: Color::Indexed(240),
        },
        _ => ChromePalette {
            file_list: Color::Gray,
            selected_file: Color::White,
            selected_file_background: Color::DarkGray,
            separator: Color::DarkGray,
            footer: Color::DarkGray,
        },
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };

    use super::{render_frame, render_frame_with_layout};
    use crate::{
        interaction::{DiffLayout, Interaction, ViewPreferences, Viewport},
        layout::layout,
        parser::parse_unified_diff,
    };

    #[test]
    fn renders_compact_file_and_diff_panes_with_one_separator() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-before\n+after\n",
        )
        .expect("valid two-file diff");
        let interaction = Interaction {
            selected_file: 1,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport::new(80, 8),
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).expect("test terminal");

        terminal
            .draw(|frame| render_frame(frame, &document, &view, &interaction.viewport, u16::MAX))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let expected_symbols = [
            ((2, 0), "f", "first file"),
            ((24, 0), " ", "left separator padding"),
            ((25, 0), "│", "separator"),
            ((26, 0), " ", "right separator padding"),
            ((27, 0), " ", "metadata marker column"),
            ((28, 0), " ", "metadata marker spacing"),
            ((29, 0), "-", "metadata content"),
            ((0, 1), "·", "selection marker"),
            ((27, 3), "-", "deletion marker"),
            ((27, 4), "+", "addition marker"),
            ((3, 3), "k", "footer hint"),
        ];

        for ((x, y), expected, name) in expected_symbols {
            assert_eq!(rendered[(x, y)].symbol(), expected, "{name}");
        }
        assert_eq!(rendered[(2, 0)].fg, Color::Rgb(160, 160, 160), "file label");
        assert_eq!(
            rendered[(0, 9)].fg,
            Color::Rgb(110, 110, 110),
            "footer hint"
        );
    }

    #[test]
    fn collapses_the_file_list_before_the_diff_pane() {
        let document = parse_unified_diff(b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n")
            .expect("valid diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport::new(23, 8),
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(23, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_frame(frame, &document, &view, &interaction.viewport, u16::MAX))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        assert_eq!(rendered[(0, 3)].symbol(), "-");
        assert_eq!(rendered[(1, 3)].symbol(), " ");
    }

    #[test]
    fn renders_vertical_pairs_with_a_dimmer_inner_separator() {
        let document = parse_unified_diff(
            b"--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        )
        .expect("valid asymmetric diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Vertical),
            viewport: Viewport::new(80, 8),
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| {
                render_frame_with_layout(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &interaction.preferences,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        assert_eq!(rendered[(27, 3)].symbol(), "-", "before marker");
        assert_eq!(rendered[(54, 3)].symbol(), "+", "after marker");
        assert_eq!(rendered[(53, 3)].symbol(), "│", "inner separator");
        assert_eq!(rendered[(29, 5)].symbol(), "c", "before context payload");
        assert_eq!(rendered[(56, 5)].symbol(), "c", "after context payload");
        assert_eq!(rendered[(53, 5)].symbol(), "│", "context separator");
        assert_eq!(
            rendered[(54, 4)].symbol(),
            " ",
            "alignment peer has no marker"
        );
        assert_eq!(
            rendered[(79, 4)].bg,
            Color::Rgb(35, 73, 43),
            "alignment peer retains addition background"
        );
        assert!(
            !rendered[(79, 4)].modifier.contains(Modifier::DIM),
            "alignment peer does not inherit deletion dimming"
        );
        assert_eq!(
            rendered[(53, 3)].fg,
            Color::Rgb(95, 95, 95),
            "inner separator is dimmer than the outer separator"
        );
    }

    #[test]
    fn scrolls_both_vertical_panes_by_one_horizontal_offset() {
        let document = parse_unified_diff(b"--- a/f\n+++ b/f\n@@ -1 +1 @@\n-abcdef\n+ABCDEF\n")
            .expect("valid diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Vertical),
            viewport: Viewport {
                vertical_offset: 0,
                horizontal_offset: 2,
                width: 80,
                height: 8,
            },
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| {
                render_frame_with_layout(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &interaction.preferences,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        assert_eq!(rendered[(27, 3)].symbol(), "a", "before pane offset");
        assert_eq!(rendered[(54, 3)].symbol(), "A", "after pane offset");
    }

    #[test]
    fn renders_vertical_deletion_peers_with_deletion_background() {
        let document = parse_unified_diff(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,3 @@\n-old first\n+new first\n+new second\n context\n",
        )
        .expect("valid asymmetric diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Vertical),
            viewport: Viewport::new(80, 8),
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| {
                render_frame_with_layout(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &interaction.preferences,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();

        assert_eq!(
            rendered[(27, 4)].symbol(),
            " ",
            "deletion peer has no marker"
        );
        assert_eq!(
            rendered[(52, 4)].bg,
            Color::Rgb(50, 30, 30),
            "deletion peer reaches the before pane edge"
        );
        assert!(
            rendered[(52, 4)].modifier.contains(Modifier::DIM),
            "deletion peer retains deletion dimming"
        );
    }

    #[test]
    fn renders_stacked_alignment_peers_as_markerless_rows() {
        let document = parse_unified_diff(
            b"--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        )
        .expect("valid asymmetric diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Stacked),
            viewport: Viewport::new(80, 14),
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 14)).expect("test terminal");

        terminal
            .draw(|frame| {
                render_frame_with_layout(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &interaction.preferences,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let context_row = (0..14)
            .rev()
            .find(|row| rendered[(29, *row)].symbol() == "c")
            .expect("after context row");
        let alignment_peer_row = context_row
            .checked_sub(1)
            .expect("alignment peer precedes context");

        assert_eq!(
            rendered[(29, alignment_peer_row)].symbol(),
            " ",
            "alignment peer is empty"
        );
        assert_eq!(
            rendered[(79, alignment_peer_row)].bg,
            Color::Rgb(35, 73, 43),
            "alignment peer retains addition background"
        );
        assert!(
            !rendered[(79, alignment_peer_row)]
                .modifier
                .contains(Modifier::DIM),
            "alignment peer does not inherit deletion dimming"
        );
    }

    #[test]
    fn renders_syntax_and_low_contrast_record_styles() {
        let document = parse_unified_diff(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1,2 +1,2 @@\n context\n-fn removed() { let label = \"value\"; }\n+fn added() { let label = \"value\"; }\n",
        )
        .expect("valid Rust diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport::new(80, 8),
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_frame(frame, &document, &view, &interaction.viewport, u16::MAX))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let expected_symbols = [
            ((27, 0), " ", "header marker column"),
            ((28, 0), " ", "header marker spacing"),
            ((29, 0), "-", "header content"),
            ((27, 3), " ", "context marker column"),
            ((28, 3), " ", "context marker spacing"),
            ((28, 4), " ", "deletion marker spacing"),
            ((28, 5), " ", "addition marker spacing"),
        ];
        let expected_styles = [
            (
                (29, 3),
                Color::Rgb(180, 180, 180),
                Color::Reset,
                "context payload",
            ),
            (
                (27, 4),
                Color::Rgb(206, 74, 74),
                Color::Rgb(50, 30, 30),
                "deletion marker",
            ),
            (
                (27, 5),
                Color::Rgb(112, 195, 115),
                Color::Rgb(35, 73, 43),
                "addition marker",
            ),
            (
                (29, 4),
                Color::Rgb(86, 156, 214),
                Color::Rgb(50, 30, 30),
                "deletion keyword",
            ),
            (
                (29, 5),
                Color::Rgb(86, 156, 214),
                Color::Rgb(35, 73, 43),
                "addition keyword",
            ),
            (
                (54, 5),
                Color::Rgb(206, 145, 120),
                Color::Rgb(35, 73, 43),
                "addition string",
            ),
            (
                (79, 4),
                Color::Rgb(150, 150, 150),
                Color::Rgb(50, 30, 30),
                "deletion background reaches the diff pane edge",
            ),
            (
                (79, 5),
                Color::Rgb(220, 235, 220),
                Color::Rgb(35, 73, 43),
                "addition background reaches the diff pane edge",
            ),
        ];

        for ((x, y), expected, name) in expected_symbols {
            assert_eq!(rendered[(x, y)].symbol(), expected, "{name}");
        }
        for ((x, y), expected_foreground, expected_background, name) in expected_styles {
            let cell = &rendered[(x, y)];

            assert_eq!(cell.fg, expected_foreground, "{name} foreground");
            assert_eq!(cell.bg, expected_background, "{name} background");
        }
        assert!(
            rendered[(29, 4)].modifier.contains(Modifier::DIM),
            "deletion syntax inherits row-wide dimming"
        );
    }

    #[test]
    fn renders_character_detail_in_dim_change_colors_and_selects_files_cohesively() {
        let document =
            parse_unified_diff(b"--- a/source.txt\n+++ b/source.txt\n@@ -1 +1 @@\n-one\n+ore\n")
                .expect("valid character diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences {
                layout: DiffLayout::Unified,
                granularity: crate::interaction::DiffGranularity::Character,
                context_lines: 3,
            },
            viewport: Viewport::new(80, 8),
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| {
                render_frame_with_layout(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &interaction.preferences,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let selected_marker = &rendered[(0, 0)];
        let selected_label = &rendered[(2, 0)];

        assert_eq!(selected_marker.symbol(), "·");
        assert_eq!(selected_marker.fg, selected_label.fg);
        assert_eq!(selected_marker.bg, selected_label.bg);
        assert_eq!(rendered[(30, 3)].fg, Color::Rgb(206, 74, 74));
        assert_eq!(rendered[(30, 4)].fg, Color::Rgb(112, 195, 115));
        assert!(rendered[(30, 3)].modifier.contains(Modifier::DIM));
        assert!(rendered[(30, 4)].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn renders_hunk_headers_as_muted_cyan_blue() {
        let document = parse_unified_diff(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-old\n+new\n@@ -3 +3 @@\n-before\n+after\n",
        )
        .expect("valid Rust diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport::new(80, 8),
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_frame(frame, &document, &view, &interaction.viewport, u16::MAX))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        for y in [2, 5] {
            let hunk_header = &rendered[(29, y)];

            assert_eq!(hunk_header.symbol(), "@");
            assert_eq!(hunk_header.fg, Color::Rgb(100, 155, 175));
            assert_eq!(hunk_header.bg, Color::Reset);
        }
        assert_eq!(
            rendered[(29, 0)].fg,
            Color::Reset,
            "file header stays neutral"
        );
    }

    #[test]
    fn renders_current_shortcuts_in_a_multiline_footer() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-before\n+after\n",
        )
        .expect("valid two-file diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport::new(80, 12),
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 15)).expect("test terminal");

        terminal
            .draw(|frame| render_frame(frame, &document, &view, &interaction.viewport, u16::MAX))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let expected = [
            ((3, 3), "k", "upward movement"),
            ((1, 4), "h", "leftward movement"),
            ((3, 4), "·", "movement separator"),
            ((5, 4), "l", "rightward movement"),
            ((8, 4), "m", "movement label"),
            ((3, 5), "j", "downward movement"),
            ((2, 7), "^", "page up"),
            ((2, 8), "^", "page down"),
            ((2, 9), "g", "top and bottom"),
            ((2, 10), "v", "layout cycle"),
            ((2, 11), "c", "character detail"),
            ((1, 12), "+", "context controls"),
            ((0, 13), "(", "file rotation"),
            ((3, 14), "q", "exit"),
        ];

        for ((x, y), symbol, name) in expected {
            assert_eq!(rendered[(x, y)].symbol(), symbol, "{name}");
            assert_eq!(
                rendered[(x, y)].fg,
                Color::Rgb(110, 110, 110),
                "{name} color"
            );
        }
    }
}
