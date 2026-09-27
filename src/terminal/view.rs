use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout as RatatuiLayout},
    widgets::Paragraph,
};

use crate::{
    file_tree::FileTree,
    interaction::{DiffLayout, Interaction},
    layout::{PaneLayout, ScrollExtent, content_pane_height, pane_layout},
    prepared::PreparedFile,
    styling::StyledRow,
};

#[cfg(test)]
use crate::{document::DiffDocument, prepared::PreparedDocument};

use super::{
    chrome::{draw_inner_separator, draw_outer_separator, render_file_column},
    row_writer::write_row,
    rows::{
        PaneRow, VisibleSideBySideRow, aligned_pair, visible_diff_rows, visible_side_by_side_rows,
    },
};

struct DisplayRows<'a> {
    rows: LayoutRows<'a>,
    horizontal_offset: u16,
    color_count: u16,
}

/// Rows of the active layout only, starting at the viewport and bounded by the screen.
enum LayoutRows<'a> {
    Unified(Vec<StyledRow<'a>>),
    Vertical(Vec<VisibleSideBySideRow<'a>>),
    Stacked(Vec<VisibleSideBySideRow<'a>>),
}

/// Prepares `document` and draws the interaction's selected file.
#[cfg(test)]
fn render_document(
    frame: &mut Frame,
    document: &DiffDocument,
    interaction: &Interaction,
    color_count: u16,
) {
    let prepared = PreparedDocument::prepare(document);
    let file = prepared
        .file(interaction.selected_file)
        .expect("selected prepared file");
    let preferences = interaction.preferences;
    let extent = file
        .layout()
        .scroll_extent(preferences.layout, preferences.context_lines);

    render_prepared_frame(
        frame,
        prepared.file_tree(),
        file,
        &extent,
        interaction,
        color_count,
    );
}

/// Draws `file` through `extent`, the scroll extent of the interaction's layout and
/// context count, beside the file list of `tree`.
pub(super) fn render_prepared_frame(
    frame: &mut Frame,
    tree: &FileTree,
    file: &PreparedFile,
    extent: &ScrollExtent,
    interaction: &Interaction,
    color_count: u16,
) {
    let area = frame.area();
    let row_limit = usize::from(area.height);
    let granularity = interaction.preferences.granularity;
    let viewport = &interaction.viewport;
    let side_by_side_rows =
        || visible_side_by_side_rows(file, extent, viewport, granularity, row_limit);
    let rows = match interaction.preferences.layout {
        DiffLayout::Unified => LayoutRows::Unified(visible_diff_rows(
            file,
            extent,
            viewport,
            granularity,
            row_limit,
        )),
        DiffLayout::Vertical => LayoutRows::Vertical(side_by_side_rows()),
        DiffLayout::Stacked => LayoutRows::Stacked(side_by_side_rows()),
    };
    let display = DisplayRows {
        rows,
        horizontal_offset: viewport.horizontal_offset,
        color_count,
    };

    match pane_layout(area.width, area.height) {
        PaneLayout::TooNarrow => frame.render_widget(Paragraph::new("screen too narrow"), area),
        PaneLayout::TooShort => frame.render_widget(Paragraph::new("screen too short"), area),
        PaneLayout::DiffOnly => render_display_layout(frame, display, area),
        PaneLayout::Split {
            file_list_width,
            separator_padding,
        } => {
            let [file_column, _, separator, _, diff] = RatatuiLayout::horizontal([
                Constraint::Length(file_list_width),
                Constraint::Length(separator_padding),
                Constraint::Length(1),
                Constraint::Length(separator_padding),
                Constraint::Min(1),
            ])
            .areas(area);

            render_file_column(
                frame,
                file_column,
                tree,
                interaction.selected_file,
                color_count,
            );
            draw_outer_separator(frame.buffer_mut(), separator, color_count);
            render_display_layout(frame, display, diff);
        }
    }
}

fn render_diff_rows<'a>(
    frame: &mut Frame,
    rows: impl IntoIterator<Item = PaneRow<'a>>,
    area: ratatui::layout::Rect,
    horizontal_offset: u16,
    color_count: u16,
) {
    for (index, row) in rows.into_iter().take(area.height.into()).enumerate() {
        let row_area = ratatui::layout::Rect::new(area.x, area.y + index as u16, area.width, 1);

        write_row(
            frame.buffer_mut(),
            row_area,
            row,
            horizontal_offset,
            color_count,
        );
    }
}

fn render_display_layout(frame: &mut Frame, display: DisplayRows, area: ratatui::layout::Rect) {
    let horizontal_offset = display.horizontal_offset;
    let color_count = display.color_count;

    match display.rows {
        LayoutRows::Unified(rows) => render_diff_rows(
            frame,
            rows.into_iter().map(PaneRow::Text),
            area,
            horizontal_offset,
            color_count,
        ),
        LayoutRows::Vertical(rows) => {
            render_vertical_rows(frame, rows, area, horizontal_offset, color_count)
        }
        LayoutRows::Stacked(rows) => {
            render_stacked_rows(frame, rows, area, horizontal_offset, color_count)
        }
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

    for (index, row) in rows.into_iter().take(area.height.into()).enumerate() {
        let row_area = ratatui::layout::Rect::new(area.x, area.y + index as u16, area.width, 1);
        let before_area = ratatui::layout::Rect::new(panes[0].x, row_area.y, panes[0].width, 1);
        let separator_area = ratatui::layout::Rect::new(panes[1].x, row_area.y, 1, 1);
        let after_area = ratatui::layout::Rect::new(panes[2].x, row_area.y, panes[2].width, 1);
        let pair = match row {
            VisibleSideBySideRow::Shared(row) => Some((PaneRow::Text(row), PaneRow::Text(row))),
            VisibleSideBySideRow::Paired { before, after } => aligned_pair(before, after),
        };
        let Some((before, after)) = pair else {
            continue;
        };

        let buffer = frame.buffer_mut();
        write_row(buffer, before_area, before, horizontal_offset, color_count);
        write_row(buffer, after_area, after, horizontal_offset, color_count);
        draw_inner_separator(buffer, separator_area, "│", color_count);
    }
}

fn render_stacked_rows(
    frame: &mut Frame,
    rows: Vec<VisibleSideBySideRow>,
    area: ratatui::layout::Rect,
    horizontal_offset: u16,
    color_count: u16,
) {
    let pane_height = content_pane_height(usize::from(area.height), DiffLayout::Stacked);
    let pane_height = u16::try_from(pane_height).expect("pane height fits the area");
    let panes = RatatuiLayout::vertical([
        Constraint::Length(pane_height),
        Constraint::Length(1),
        Constraint::Length(pane_height),
    ])
    .flex(Flex::Start)
    .split(area);
    let mut before_rows = Vec::new();
    let mut after_rows = Vec::new();

    for row in rows {
        match row {
            VisibleSideBySideRow::Shared(row) => {
                before_rows.push(PaneRow::Text(row));
                after_rows.push(PaneRow::Text(row));
            }
            VisibleSideBySideRow::Paired { before, after } => {
                let Some((before, after)) = aligned_pair(before, after) else {
                    continue;
                };

                before_rows.push(before);
                after_rows.push(after);
            }
        }
    }

    draw_inner_separator(frame.buffer_mut(), panes[1], "─", color_count);
    render_diff_rows(frame, before_rows, panes[0], horizontal_offset, color_count);
    render_diff_rows(frame, after_rows, panes[2], horizontal_offset, color_count);
}

#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal,
        backend::TestBackend,
        buffer::{Buffer, Cell},
        style::{Color, Modifier},
    };

    use super::{render_document, render_prepared_frame};
    use crate::{
        interaction::{
            DiffLayout, DisplayBounds, Input, Interaction, NavigationBounds, Transition,
            ViewPreferences, Viewport, transition_interaction,
        },
        parser::parse_unified_diff,
        prepared::PreparedDocument,
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
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
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
            ((0, 1), "~", "selected file's state marker"),
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
        let mut terminal = Terminal::new(TestBackend::new(23, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
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
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
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
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
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
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
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
        let mut terminal = Terminal::new(TestBackend::new(80, 14)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
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
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
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
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let selected_marker = &rendered[(0, 0)];
        let selected_label = &rendered[(2, 0)];

        let selected_row_end = &rendered[(23, 0)];

        assert_eq!(selected_marker.symbol(), "~");
        assert_eq!(selected_marker.fg, selected_label.fg);
        assert_eq!(selected_marker.bg, selected_label.bg);
        assert_eq!(
            selected_row_end.bg, selected_label.bg,
            "the highlight spans the whole row"
        );
        assert_eq!(rendered[(30, 3)].fg, Color::Rgb(206, 74, 74));
        assert_eq!(rendered[(30, 4)].fg, Color::Rgb(112, 195, 115));
        assert!(rendered[(30, 3)].modifier.contains(Modifier::DIM));
        assert!(rendered[(30, 4)].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn toggles_unmatched_payload_colors_in_unified_layout() {
        assert_unmatched_payload_toggle(DiffLayout::Unified);
    }

    #[test]
    fn toggles_unmatched_payload_colors_in_vertical_layout() {
        assert_unmatched_payload_toggle(DiffLayout::Vertical);
    }

    #[test]
    fn toggles_unmatched_payload_colors_in_stacked_layout() {
        assert_unmatched_payload_toggle(DiffLayout::Stacked);
    }

    fn assert_unmatched_payload_toggle(diff_layout: DiffLayout) {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n\
              --- a/source.rs\n+++ b/source.rs\n@@ -1 +0,0 @@\n-fn removed() {}\n\
              @@ -2,0 +2 @@\n+fn added() {}\n",
        )
        .expect("valid unmatched Rust changes");
        let prepared = PreparedDocument::prepare(&document);
        let file = prepared.file(1).expect("second prepared file");
        let tree = prepared.file_tree();
        let extent = file.layout().scroll_extent(diff_layout, 3);
        let interaction = Interaction {
            selected_file: 1,
            preferences: ViewPreferences::line(diff_layout),
            viewport: Viewport {
                vertical_offset: 1,
                horizontal_offset: 1,
                width: 160,
                height: 30,
            },
        };
        let bounds = NavigationBounds {
            file_count: prepared.file_count(),
            target_display: DisplayBounds {
                maximum_vertical_offset: file.layout().diff_lines.len().saturating_sub(30),
                maximum_horizontal_offset: 1,
            },
            anchored_vertical_offset: 1,
        };
        let mut terminal = Terminal::new(TestBackend::new(160, 30)).expect("test terminal");
        terminal
            .draw(|frame| render_prepared_frame(frame, tree, file, &extent, &interaction, u16::MAX))
            .expect("render line mode");
        let line_rendered = terminal.backend().buffer().clone();
        let payloads = [
            ("fn removed() {}", Color::Rgb(206, 74, 74)),
            ("fn added() {}", Color::Rgb(112, 195, 115)),
        ];
        for (payload, _) in payloads {
            let cells = unique_payload_cells(&line_rendered, payload);
            assert_eq!(cells[0].fg, Color::Rgb(86, 156, 214), "Rust keyword");
        }

        let Transition::RedrawInteraction(character) =
            transition_interaction(&interaction, Input::ToggleGranularity, &bounds)
        else {
            panic!("character toggle redraws interaction");
        };
        assert_eq!(character.selected_file, interaction.selected_file);
        assert_eq!(character.viewport, interaction.viewport);
        terminal
            .draw(|frame| render_prepared_frame(frame, tree, file, &extent, &character, u16::MAX))
            .expect("render character mode");

        // architecture.md: unmatched source payloads contain no unchanged characters.
        for (payload, color) in payloads {
            let cells = unique_payload_cells(terminal.backend().buffer(), payload);
            for cell in cells {
                assert_eq!(cell.fg, color, "{diff_layout:?}: {payload}");
                assert!(cell.modifier.contains(Modifier::DIM), "{payload}");
            }
        }
        let Transition::RedrawInteraction(line) =
            transition_interaction(&character, Input::ToggleGranularity, &bounds)
        else {
            panic!("line toggle redraws interaction");
        };
        terminal
            .draw(|frame| render_prepared_frame(frame, tree, file, &extent, &line, u16::MAX))
            .expect("restore line mode from the same prepared file");

        assert_eq!(line, interaction, "toggle preserves the reader position");
        assert_eq!(terminal.backend().buffer(), &line_rendered);
    }

    fn unique_payload_cells<'a>(buffer: &'a Buffer, payload: &str) -> &'a [Cell] {
        let mut occurrences = buffer.content.windows(payload.len()).filter(|cells| {
            cells
                .iter()
                .map(Cell::symbol)
                .eq(payload.split("").filter(|symbol| !symbol.is_empty()))
        });
        let cells = occurrences.next().expect("full source payload is visible");

        assert!(occurrences.next().is_none(), "source payload appears once");
        cells
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
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
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
        let mut terminal = Terminal::new(TestBackend::new(80, 15)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
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
