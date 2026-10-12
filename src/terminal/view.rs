use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout as RatatuiLayout, Rect},
    widgets::Paragraph,
};

use crate::{
    file_tree::FileTree,
    interaction::{DiffLayout, Interaction},
    layout::{PaneLayout, ScrollExtent, Wrap, pane_height, pane_layout},
    prepared::PreparedFile,
    styling::StyledRow,
};

#[cfg(test)]
use crate::{document::DiffDocument, prepared::PreparedDocument};

use super::{
    chrome::{
        ContinuityFrame, LineWindow, draw_inner_separator, draw_outer_separator, render_file_column,
    },
    row_writer::{RowPlacement, write_row},
    rows::{
        PaneRow, VisibleLine, VisibleSideBySideRow, aligned_pair, visible_diff_rows,
        visible_side_by_side_rows,
    },
};

struct DisplayRows<'a> {
    rows: LayoutRows<'a>,
    window: LineWindow,
    horizontal_offset: u16,
    wrap: Wrap,
    color_count: u16,
}

/// Screen lines of the active layout only, starting at the viewport and bounded by the
/// screen.
enum LayoutRows<'a> {
    Unified(Vec<VisibleLine<StyledRow<'a>>>),
    Vertical(Vec<VisibleLine<VisibleSideBySideRow<'a>>>),
    Stacked(Vec<VisibleLine<VisibleSideBySideRow<'a>>>),
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
    let wrap = Wrap::of(
        preferences.wrapping,
        interaction.viewport.width,
        preferences.layout,
    );
    let extent = file
        .layout()
        .scroll_extent(preferences.layout, preferences.context_lines, wrap);

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
    let preferences = interaction.preferences;
    let display = DisplayRows {
        rows,
        window: LineWindow {
            first: viewport.vertical_offset,
            total: extent.line_count(),
        },
        horizontal_offset: viewport.horizontal_offset,
        wrap: Wrap::of(preferences.wrapping, area.width, preferences.layout),
        color_count,
    };

    match pane_layout(area.width, area.height, preferences.layout) {
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

            render_file_column(frame, file_column, tree, file.list_row(), color_count);
            draw_outer_separator(frame.buffer_mut(), separator, color_count);
            render_display_layout(frame, display, diff);
        }
    }
}

fn render_display_layout(frame: &mut Frame, display: DisplayRows, area: Rect) {
    let placement = |line| RowPlacement::new(display.wrap, display.horizontal_offset, line);
    let pane_rows = PaneRows {
        window: display.window,
        placement,
        color_count: display.color_count,
    };

    match display.rows {
        LayoutRows::Unified(lines) => pane_rows.render(
            frame,
            ContinuityFrame::diff_pane(area),
            lines.into_iter().map(|line| VisibleLine {
                row: PaneRow::Text(line.row),
                line: line.line,
            }),
        ),
        LayoutRows::Vertical(lines) => render_vertical_rows(frame, lines, area, pane_rows),
        LayoutRows::Stacked(lines) => render_stacked_rows(frame, lines, area, pane_rows),
    }
}

/// Draws one pane's lines inside its continuity-marker frame.
struct PaneRows<P> {
    window: LineWindow,
    placement: P,
    color_count: u16,
}

impl<P: Fn(usize) -> RowPlacement> PaneRows<P> {
    fn render<'a>(
        &self,
        frame: &mut Frame,
        pane: ContinuityFrame,
        lines: impl IntoIterator<Item = VisibleLine<PaneRow<'a>>>,
    ) {
        let content = pane.content();
        let buffer = frame.buffer_mut();

        for (index, line) in lines.into_iter().take(content.height.into()).enumerate() {
            let y = content.y + index as u16;
            let row_area = Rect::new(content.x, y, content.width, 1);

            let clip = write_row(
                buffer,
                row_area,
                line.row,
                (self.placement)(line.line),
                self.color_count,
            );
            pane.draw_line_markers(buffer, y, clip, self.color_count);
        }
        pane.draw_window_markers(buffer, self.window, self.color_count);
    }
}

fn render_vertical_rows(
    frame: &mut Frame,
    lines: Vec<VisibleLine<VisibleSideBySideRow>>,
    area: Rect,
    pane_rows: PaneRows<impl Fn(usize) -> RowPlacement>,
) {
    let [before_pane, separator, after_pane] = RatatuiLayout::horizontal([
        Constraint::Percentage(50),
        Constraint::Length(1),
        Constraint::Percentage(50),
    ])
    .areas(area);
    let (before_lines, after_lines) = split_panes(lines);
    let before_pane = ContinuityFrame::diff_pane(before_pane);
    let after_pane = ContinuityFrame::diff_pane(after_pane);

    let content = before_pane.content();
    let shown = before_lines.len().min(content.height.into()) as u16;
    let separator = Rect::new(separator.x, content.y, separator.width, shown);
    draw_inner_separator(frame.buffer_mut(), separator, "│", pane_rows.color_count);

    pane_rows.render(frame, before_pane, before_lines);
    pane_rows.render(frame, after_pane, after_lines);
}

fn render_stacked_rows(
    frame: &mut Frame,
    lines: Vec<VisibleLine<VisibleSideBySideRow>>,
    area: Rect,
    pane_rows: PaneRows<impl Fn(usize) -> RowPlacement>,
) {
    let pane_height = pane_height(usize::from(area.height), DiffLayout::Stacked);
    let pane_height = u16::try_from(pane_height).expect("pane height fits the area");
    let [before_pane, separator, after_pane] = RatatuiLayout::vertical([
        Constraint::Length(pane_height),
        Constraint::Length(1),
        Constraint::Length(pane_height),
    ])
    .flex(Flex::Start)
    .areas(area);
    let (before_lines, after_lines) = split_panes(lines);

    draw_inner_separator(frame.buffer_mut(), separator, "─", pane_rows.color_count);
    pane_rows.render(frame, ContinuityFrame::diff_pane(before_pane), before_lines);
    pane_rows.render(frame, ContinuityFrame::diff_pane(after_pane), after_lines);
}

/// The before and after pane lines of side-by-side lines.
fn split_panes<'a>(
    lines: Vec<VisibleLine<VisibleSideBySideRow<'a>>>,
) -> (Vec<VisibleLine<PaneRow<'a>>>, Vec<VisibleLine<PaneRow<'a>>>) {
    lines
        .into_iter()
        .filter_map(|line| {
            let (before, after) = pane_pair(line.row)?;
            let before = VisibleLine {
                row: before,
                line: line.line,
            };
            let after = VisibleLine {
                row: after,
                line: line.line,
            };
            Some((before, after))
        })
        .unzip()
}

/// The before and after pane rows of one side-by-side row.
fn pane_pair(row: VisibleSideBySideRow) -> Option<(PaneRow, PaneRow)> {
    match row {
        VisibleSideBySideRow::Shared(row) => Some((PaneRow::Text(row), PaneRow::Text(row))),
        VisibleSideBySideRow::Paired { before, after } => aligned_pair(before, after),
    }
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
            ((2, 1), "f", "first file"),
            ((24, 0), " ", "left separator padding"),
            ((25, 0), "│", "separator"),
            ((26, 0), " ", "right separator padding"),
            ((28, 1), " ", "metadata marker column"),
            ((29, 1), " ", "metadata marker spacing"),
            ((30, 1), "-", "metadata content"),
            ((0, 2), "~", "selected file's state marker"),
            ((28, 4), "-", "deletion marker"),
            ((28, 5), "+", "addition marker"),
            ((3, 5), "k", "footer hint"),
        ];

        for ((x, y), expected, name) in expected_symbols {
            assert_eq!(rendered[(x, y)].symbol(), expected, "{name}");
        }
        assert_eq!(rendered[(2, 1)].fg, Color::Rgb(160, 160, 160), "file name");
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
        assert_eq!(rendered[(1, 4)].symbol(), "-");
        assert_eq!(rendered[(2, 4)].symbol(), " ");
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
        assert_eq!(rendered[(28, 4)].symbol(), "-", "before marker");
        assert_eq!(rendered[(55, 4)].symbol(), "+", "after marker");
        assert_eq!(rendered[(53, 4)].symbol(), "│", "inner separator");
        assert_eq!(rendered[(30, 6)].symbol(), "c", "before context payload");
        assert_eq!(rendered[(57, 6)].symbol(), "c", "after context payload");
        assert_eq!(rendered[(53, 6)].symbol(), "│", "context separator");
        assert_eq!(
            rendered[(55, 5)].symbol(),
            " ",
            "alignment peer has no marker"
        );
        assert_eq!(
            rendered[(78, 5)].bg,
            Color::Rgb(35, 73, 43),
            "alignment peer retains addition background"
        );
        assert!(
            !rendered[(78, 5)].modifier.contains(Modifier::DIM),
            "alignment peer does not inherit deletion dimming"
        );
        assert_eq!(
            rendered[(53, 4)].fg,
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
        assert_eq!(rendered[(28, 4)].symbol(), "a", "before pane offset");
        assert_eq!(rendered[(55, 4)].symbol(), "A", "after pane offset");
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
            rendered[(28, 5)].symbol(),
            " ",
            "deletion peer has no marker"
        );
        assert_eq!(
            rendered[(51, 5)].bg,
            Color::Rgb(50, 30, 30),
            "deletion peer reaches the before pane edge"
        );
        assert!(
            rendered[(51, 5)].modifier.contains(Modifier::DIM),
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
            viewport: Viewport::new(80, 20),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let context_row = (0..20)
            .rev()
            .find(|row| rendered[(30, *row)].symbol() == "c")
            .expect("after context row");
        let alignment_peer_row = context_row
            .checked_sub(1)
            .expect("alignment peer precedes context");

        assert_eq!(
            rendered[(30, alignment_peer_row)].symbol(),
            " ",
            "alignment peer is empty"
        );
        assert_eq!(
            rendered[(78, alignment_peer_row)].bg,
            Color::Rgb(35, 73, 43),
            "alignment peer retains addition background"
        );
        assert!(
            !rendered[(78, alignment_peer_row)]
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
            ((28, 1), " ", "header marker column"),
            ((29, 1), " ", "header marker spacing"),
            ((30, 1), "-", "header content"),
            ((28, 4), " ", "context marker column"),
            ((29, 4), " ", "context marker spacing"),
            ((29, 5), " ", "deletion marker spacing"),
            ((29, 6), " ", "addition marker spacing"),
        ];
        let expected_styles = [
            (
                (30, 4),
                Color::Rgb(180, 180, 180),
                Color::Reset,
                "context payload",
            ),
            (
                (28, 5),
                Color::Rgb(206, 74, 74),
                Color::Rgb(50, 30, 30),
                "deletion marker",
            ),
            (
                (28, 6),
                Color::Rgb(112, 195, 115),
                Color::Rgb(35, 73, 43),
                "addition marker",
            ),
            (
                (30, 5),
                Color::Rgb(86, 156, 214),
                Color::Rgb(50, 30, 30),
                "deletion keyword",
            ),
            (
                (30, 6),
                Color::Rgb(86, 156, 214),
                Color::Rgb(35, 73, 43),
                "addition keyword",
            ),
            (
                (55, 6),
                Color::Rgb(206, 145, 120),
                Color::Rgb(35, 73, 43),
                "addition string",
            ),
            (
                (78, 5),
                Color::Rgb(150, 150, 150),
                Color::Rgb(50, 30, 30),
                "deletion background reaches the diff pane edge",
            ),
            (
                (78, 6),
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
            rendered[(30, 5)].modifier.contains(Modifier::DIM),
            "deletion syntax inherits row-wide dimming"
        );
    }

    #[test]
    fn renders_character_detail_with_bright_changes_over_muted_text_and_selects_files_cohesively() {
        let document =
            parse_unified_diff(b"--- a/source.txt\n+++ b/source.txt\n@@ -1 +1 @@\n-one\n+ore\n")
                .expect("valid character diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences {
                layout: DiffLayout::Unified,
                granularity: crate::interaction::DiffGranularity::Character,
                wrapping: crate::interaction::Wrapping::Disabled,
                context_lines: 3,
            },
            viewport: Viewport::new(80, 8),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let selected_marker = &rendered[(0, 1)];
        let selected_label = &rendered[(2, 1)];

        let selected_row_end = &rendered[(23, 1)];

        assert_eq!(selected_marker.symbol(), "~");
        assert_eq!(selected_marker.fg, selected_label.fg);
        assert_eq!(selected_marker.bg, selected_label.bg);
        assert_eq!(
            selected_row_end.bg, selected_label.bg,
            "the highlight spans the whole row"
        );
        assert_eq!(rendered[(31, 4)].fg, Color::Rgb(206, 74, 74));
        assert_eq!(rendered[(31, 5)].fg, Color::Rgb(112, 195, 115));
        assert!(
            !rendered[(31, 4)].modifier.contains(Modifier::DIM),
            "changed deletion span is not dimmed"
        );
        assert!(
            !rendered[(31, 5)].modifier.contains(Modifier::DIM),
            "changed addition span is not dimmed"
        );
        assert_eq!(
            rendered[(30, 4)].fg,
            Color::Rgb(120, 120, 120),
            "unchanged deletion text is one step below the deletion body"
        );
        assert_eq!(
            rendered[(30, 5)].fg,
            Color::Rgb(170, 185, 170),
            "unchanged addition text is one step below the addition body"
        );
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
        let extent = file
            .layout()
            .scroll_extent(diff_layout, 3, crate::layout::Wrap::Off);
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
                assert!(!cell.modifier.contains(Modifier::DIM), "{payload}");
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
        for y in [3, 6] {
            let hunk_header = &rendered[(30, y)];

            assert_eq!(hunk_header.symbol(), "@");
            assert_eq!(hunk_header.fg, Color::Rgb(100, 155, 175));
            assert_eq!(hunk_header.bg, Color::Reset);
        }
        assert_eq!(
            rendered[(30, 1)].fg,
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
            viewport: Viewport::new(80, 18),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 18)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, u16::MAX))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let expected = [
            ((3, 5), "k", "upward movement"),
            ((1, 6), "h", "leftward movement"),
            ((3, 6), "·", "movement separator"),
            ((5, 6), "l", "rightward movement"),
            ((8, 6), "m", "movement label"),
            ((3, 7), "j", "downward movement"),
            ((2, 9), "^", "page up"),
            ((2, 10), "^", "page down"),
            ((2, 11), "g", "top and bottom"),
            ((2, 12), "v", "layout cycle"),
            ((2, 13), "c", "character detail"),
            ((2, 14), "w", "wrapping"),
            ((1, 15), "+", "context controls"),
            ((0, 16), "(", "file rotation"),
            ((3, 17), "q", "exit"),
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
