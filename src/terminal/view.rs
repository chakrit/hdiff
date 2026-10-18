use ratatui::{Frame, layout::Rect, widgets::Paragraph};

use crate::{
    extent::{DisplayExtent, Wrap},
    file_tree::FileTree,
    geometry::{PaneGeometry, SplitPanes},
    interaction::{Interaction, KeymapPopup},
    page_rows::SideBySideRow,
    prepared::PreparedPage,
    styling::StyledRow,
    theme::Theme,
};

use super::{
    chrome::{
        ContinuityFrame, LineWindow, draw_inner_separator, draw_outer_separator,
        render_file_column, render_help_box,
    },
    keymap::render_keymap_popup,
    row_writer::{Clip, RowPlacement, write_row},
    rows::{PaneRow, VisibleLine, visible_diff_rows, visible_side_by_side_rows},
};

/// Draws `page` through `extent`, the scroll extent of the interaction's layout, context
/// count, and wrap, beside the file list of `tree`.
pub(super) fn render_prepared_frame(
    frame: &mut Frame,
    tree: &FileTree,
    page: &PreparedPage,
    extent: &DisplayExtent,
    interaction: &Interaction,
    theme: &Theme,
) {
    let area = frame.area();
    let geometry = PaneGeometry::of(area, interaction.preferences);
    if let Some(too_small) = geometry.too_small() {
        frame.render_widget(Paragraph::new(too_small.message()), area);
        return;
    }

    let display = DisplayRows::of(page, extent, interaction, usize::from(area.height), theme);
    render_display_layout(frame, display, geometry.diff());
    match geometry.file_column() {
        Some(column) => {
            render_file_column(frame, column.list, tree, page.list_row(), &theme.chrome);
            draw_outer_separator(frame.buffer_mut(), column.separator, &theme.chrome);
        }
        None => render_help_box(frame, area, &theme.chrome),
    }

    if let KeymapPopup::Open = interaction.keymap {
        render_keymap_popup(frame, area, theme);
    }
}

struct DisplayRows<'a> {
    rows: LayoutRows<'a>,
    window: LineWindow,
    horizontal_offset: u16,
    wrap: Wrap,
    theme: &'a Theme,
}

/// Screen lines of the active layout only, starting at the viewport and bounded by the
/// screen.
enum LayoutRows<'a> {
    Unified(Vec<VisibleLine<StyledRow<'a>>>),
    Vertical(Vec<VisibleLine<SideBySideRow<StyledRow<'a>>>>),
    Stacked(Vec<VisibleLine<SideBySideRow<StyledRow<'a>>>>),
}

impl<'a> DisplayRows<'a> {
    /// Up to `row_limit` lines of `extent` from the interaction's scroll position.
    fn of(
        page: &'a PreparedPage,
        extent: &DisplayExtent,
        interaction: &Interaction,
        row_limit: usize,
        theme: &'a Theme,
    ) -> Self {
        let granularity = interaction.preferences.granularity;
        let scroll = interaction.viewport.scroll;
        let pairs = |extent| {
            visible_side_by_side_rows(page, extent, scroll.vertical, granularity, row_limit)
        };
        let rows = match extent {
            DisplayExtent::Unified(extent) => LayoutRows::Unified(visible_diff_rows(
                page,
                extent,
                scroll.vertical,
                granularity,
                row_limit,
            )),
            DisplayExtent::Vertical(extent) => LayoutRows::Vertical(pairs(extent)),
            DisplayExtent::Stacked(extent) => LayoutRows::Stacked(pairs(extent)),
        };

        Self {
            rows,
            window: LineWindow {
                first: scroll.vertical,
                total: extent.line_count(),
            },
            horizontal_offset: scroll.horizontal,
            wrap: extent.wrap(),
            theme,
        }
    }
}

fn render_display_layout(frame: &mut Frame, display: DisplayRows, area: Rect) {
    let placement = |line| RowPlacement::new(display.wrap, display.horizontal_offset, line);
    let pane_rows = PaneRows {
        window: display.window,
        placement,
        theme: display.theme,
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
        LayoutRows::Vertical(lines) => {
            render_vertical_rows(frame, lines, SplitPanes::vertical(area), pane_rows)
        }
        LayoutRows::Stacked(lines) => {
            render_stacked_rows(frame, lines, SplitPanes::stacked(area), pane_rows)
        }
    }
}

/// Draws one pane's lines inside its continuity-marker frame.
struct PaneRows<'a, P> {
    window: LineWindow,
    placement: P,
    theme: &'a Theme,
}

impl<P: Fn(usize) -> RowPlacement> PaneRows<'_, P> {
    fn render<'a>(
        &self,
        frame: &mut Frame,
        pane: ContinuityFrame,
        lines: impl IntoIterator<Item = VisibleLine<PaneRow<'a>>>,
    ) {
        let content = pane.content();
        let buffer = frame.buffer_mut();
        let mut clipped = Clip::default();

        for (index, line) in lines.into_iter().take(content.height.into()).enumerate() {
            let y = content.y + index as u16;
            let row_area = Rect::new(content.x, y, content.width, 1);

            let clip = write_row(
                buffer,
                row_area,
                line.row,
                (self.placement)(line.line),
                self.theme,
            );
            clipped = clipped.union(clip);
        }
        pane.draw_side_markers(buffer, clipped, &self.theme.chrome);
        pane.draw_window_markers(buffer, self.window, &self.theme.chrome);
    }
}

fn render_vertical_rows(
    frame: &mut Frame,
    lines: Vec<VisibleLine<SideBySideRow<StyledRow>>>,
    panes: SplitPanes,
    pane_rows: PaneRows<'_, impl Fn(usize) -> RowPlacement>,
) {
    let (before_lines, after_lines) = split_panes(lines);
    let before_pane = ContinuityFrame::diff_pane(panes.before);
    let after_pane = ContinuityFrame::diff_pane(panes.after);

    let content = before_pane.content();
    let shown = before_lines.len().min(content.height.into()) as u16;
    let separator = Rect::new(panes.separator.x, content.y, panes.separator.width, shown);
    draw_inner_separator(frame.buffer_mut(), separator, "│", &pane_rows.theme.chrome);

    pane_rows.render(frame, before_pane, before_lines);
    pane_rows.render(frame, after_pane, after_lines);
}

fn render_stacked_rows(
    frame: &mut Frame,
    lines: Vec<VisibleLine<SideBySideRow<StyledRow>>>,
    panes: SplitPanes,
    pane_rows: PaneRows<'_, impl Fn(usize) -> RowPlacement>,
) {
    let (before_lines, after_lines) = split_panes(lines);

    draw_inner_separator(
        frame.buffer_mut(),
        panes.separator,
        "─",
        &pane_rows.theme.chrome,
    );
    pane_rows.render(
        frame,
        ContinuityFrame::diff_pane(panes.before),
        before_lines,
    );
    pane_rows.render(frame, ContinuityFrame::diff_pane(panes.after), after_lines);
}

/// The before and after pane lines of side-by-side lines.
fn split_panes<'a>(
    lines: Vec<VisibleLine<SideBySideRow<StyledRow<'a>>>>,
) -> (Vec<VisibleLine<PaneRow<'a>>>, Vec<VisibleLine<PaneRow<'a>>>) {
    lines
        .into_iter()
        .map(|line| {
            let (before, after) = line.row.pane_pair();
            let before = VisibleLine {
                row: before,
                line: line.line,
            };
            let after = VisibleLine {
                row: after,
                line: line.line,
            };
            (before, after)
        })
        .unzip()
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use ratatui::{
        Frame, Terminal,
        backend::TestBackend,
        buffer::{Buffer, Cell},
        layout::Size,
        style::{Color, Modifier},
    };

    use super::render_prepared_frame;
    use crate::{
        document::DiffDocument,
        extent::{DisplayExtent, Wrap},
        interaction::{
            Command, Destination, DisplayBounds, Event, Interaction, KeymapPopup, ScrollPosition,
            Transition, Viewport, transition,
        },
        navigation::Navigation,
        parser::parse_unified_diff,
        preferences::{DiffLayout, FileList, PreferenceChange, ViewPreferences},
        prepared::{PageIndex, PreparedDocument},
        theme::{ColorDepth, Theme},
    };

    const THEME: &Theme = Theme::for_depth(ColorDepth::TrueColor);

    /// Prepares `document` and draws the interaction's selected page.
    fn render_document(
        frame: &mut Frame,
        document: &DiffDocument,
        interaction: &Interaction,
        theme: &Theme,
    ) {
        let prepared = PreparedDocument::prepare(document);
        let navigation = Navigation::new(&prepared, interaction);

        render_prepared_frame(
            frame,
            prepared.file_tree(),
            prepared.page(interaction.page),
            navigation.extent(),
            interaction,
            theme,
        );
    }

    /// The second page of a two-page document.
    fn second_page() -> PageIndex {
        PageIndex::FIRST.next(NonZeroUsize::MIN.saturating_add(1))
    }

    #[test]
    fn renders_compact_file_and_diff_panes_with_one_separator() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-before\n+after\n",
        )
        .expect("valid two-file diff");
        let interaction = Interaction {
            keymap: KeymapPopup::Closed,
            page: second_page(),
            preferences: ViewPreferences::initial(DiffLayout::Unified),
            viewport: Viewport::at_start(Size::new(80, 8)),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, THEME))
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
            ((1, 9), "?", "help hint"),
        ];

        for ((x, y), expected, name) in expected_symbols {
            assert_eq!(rendered[(x, y)].symbol(), expected, "{name}");
        }
        assert_eq!(rendered[(2, 1)].fg, THEME.chrome.file_name, "file name");
        assert_eq!(rendered[(0, 9)].fg, THEME.chrome.hint, "help hint");
    }

    #[test]
    fn collapses_the_file_list_before_the_diff_pane() {
        let document = parse_unified_diff(b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n")
            .expect("valid diff");
        let interaction = Interaction {
            keymap: KeymapPopup::Closed,
            page: PageIndex::FIRST,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
            viewport: Viewport::at_start(Size::new(23, 8)),
        };
        let mut terminal = Terminal::new(TestBackend::new(23, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, THEME))
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
            keymap: KeymapPopup::Closed,
            page: PageIndex::FIRST,
            preferences: ViewPreferences::initial(DiffLayout::Vertical),
            viewport: Viewport::at_start(Size::new(80, 9)),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 9)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, THEME))
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
            THEME.rows.addition.background,
            "alignment peer retains addition background"
        );
        assert!(
            !rendered[(78, 5)].modifier.contains(Modifier::DIM),
            "alignment peer does not inherit deletion dimming"
        );
        assert_eq!(
            rendered[(53, 4)].fg,
            THEME.chrome.dim_separator,
            "inner separator is dimmer than the outer separator"
        );
    }

    /// architecture.md (Continuity markers): markers take the dimmed separator tone.
    #[test]
    fn draws_continuity_markers_in_the_dimmed_separator_tone() {
        let rows = (0..20)
            .map(|row| format!("+row_{row}\n"))
            .collect::<String>();
        let patch = format!("--- a/file\n+++ b/file\n@@ -0,0 +1,20 @@\n{rows}");
        let document = parse_unified_diff(patch.as_bytes()).expect("valid diff");
        let interaction = Interaction {
            keymap: KeymapPopup::Closed,
            page: PageIndex::FIRST,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
            viewport: Viewport::at_start(Size::new(80, 8)),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, THEME))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let marker = rendered
            .content()
            .iter()
            .find(|cell| cell.symbol() == "▾")
            .expect("lines continue below");
        assert_eq!(marker.fg, THEME.chrome.dim_separator);
    }

    #[test]
    fn scrolls_both_vertical_panes_by_one_horizontal_offset() {
        let document = parse_unified_diff(b"--- a/f\n+++ b/f\n@@ -1 +1 @@\n-abcdef\n+ABCDEF\n")
            .expect("valid diff");
        let interaction = Interaction {
            keymap: KeymapPopup::Closed,
            page: PageIndex::FIRST,
            preferences: ViewPreferences::initial(DiffLayout::Vertical),
            viewport: Viewport {
                screen: Size::new(80, 8),
                scroll: ScrollPosition {
                    vertical: 0,
                    horizontal: 2,
                },
            },
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, THEME))
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
            keymap: KeymapPopup::Closed,
            page: PageIndex::FIRST,
            preferences: ViewPreferences::initial(DiffLayout::Vertical),
            viewport: Viewport::at_start(Size::new(80, 8)),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, THEME))
            .expect("render frame");

        let rendered = terminal.backend().buffer();

        assert_eq!(
            rendered[(28, 5)].symbol(),
            " ",
            "deletion peer has no marker"
        );
        assert_eq!(
            rendered[(51, 5)].bg,
            THEME.rows.deletion.background,
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
            keymap: KeymapPopup::Closed,
            page: PageIndex::FIRST,
            preferences: ViewPreferences::initial(DiffLayout::Stacked),
            viewport: Viewport::at_start(Size::new(80, 20)),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, THEME))
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
            THEME.rows.addition.background,
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
            keymap: KeymapPopup::Closed,
            page: PageIndex::FIRST,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
            viewport: Viewport::at_start(Size::new(80, 9)),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 9)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, THEME))
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
                THEME.rows.context_body,
                Color::Reset,
                "context payload",
            ),
            (
                (28, 5),
                THEME.rows.deletion.marker,
                THEME.rows.deletion.background,
                "deletion marker",
            ),
            (
                (28, 6),
                THEME.rows.addition.marker,
                THEME.rows.addition.background,
                "addition marker",
            ),
            (
                (30, 5),
                THEME.syntax.keyword,
                THEME.rows.deletion.background,
                "deletion keyword",
            ),
            (
                (30, 6),
                THEME.syntax.keyword,
                THEME.rows.addition.background,
                "addition keyword",
            ),
            (
                (55, 6),
                THEME.syntax.string,
                THEME.rows.addition.background,
                "addition string",
            ),
            (
                (78, 5),
                THEME.rows.deletion.body,
                THEME.rows.deletion.background,
                "deletion background reaches the diff pane edge",
            ),
            (
                (78, 6),
                THEME.rows.addition.body,
                THEME.rows.addition.background,
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
            keymap: KeymapPopup::Closed,
            page: PageIndex::FIRST,
            preferences: ViewPreferences {
                granularity: crate::preferences::DiffGranularity::Character,
                ..ViewPreferences::initial(DiffLayout::Unified)
            },
            viewport: Viewport::at_start(Size::new(80, 8)),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, THEME))
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
        assert_eq!(rendered[(31, 4)].fg, THEME.rows.deletion.marker);
        assert_eq!(rendered[(31, 5)].fg, THEME.rows.addition.marker);
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
            THEME.rows.deletion.unchanged,
            "unchanged deletion text is one step below the deletion body"
        );
        assert_eq!(
            rendered[(30, 5)].fg,
            THEME.rows.addition.unchanged,
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
        let file = prepared.page(second_page());
        let tree = prepared.file_tree();
        let extent = DisplayExtent::of(file.rows(), diff_layout, 3, Wrap::Off);
        let interaction = Interaction {
            keymap: KeymapPopup::Closed,
            page: second_page(),
            preferences: ViewPreferences::initial(diff_layout),
            viewport: Viewport {
                screen: Size::new(160, 30),
                scroll: ScrollPosition {
                    vertical: 1,
                    horizontal: 1,
                },
            },
        };
        let bounds = DisplayBounds {
            maximum_vertical_offset: extent.line_count(),
            maximum_horizontal_offset: 1,
        };
        let toggle = |interaction: &Interaction| {
            let event = Event::Command(Command::Preference(PreferenceChange::ToggleGranularity));
            let destination = Destination::new(
                interaction.target(&event),
                prepared.page_count(),
                bounds,
                interaction.viewport.scroll,
            );
            let Transition::Redraw(next) = transition(interaction, event, &destination) else {
                panic!("detail toggle redraws interaction");
            };
            next
        };
        let mut terminal = Terminal::new(TestBackend::new(160, 30)).expect("test terminal");
        terminal
            .draw(|frame| render_prepared_frame(frame, tree, file, &extent, &interaction, THEME))
            .expect("render line mode");
        let line_rendered = terminal.backend().buffer().clone();
        let payloads = [
            ("fn removed() {}", THEME.rows.deletion.marker),
            ("fn added() {}", THEME.rows.addition.marker),
        ];
        for (payload, _) in payloads {
            let cells = unique_payload_cells(&line_rendered, payload);
            assert_eq!(cells[0].fg, THEME.syntax.keyword, "Rust keyword");
        }

        let character = toggle(&interaction);
        assert_eq!(character.page, interaction.page);
        assert_eq!(character.viewport, interaction.viewport);
        terminal
            .draw(|frame| render_prepared_frame(frame, tree, file, &extent, &character, THEME))
            .expect("render character mode");

        // architecture.md: unmatched source payloads contain no unchanged characters.
        for (payload, color) in payloads {
            let cells = unique_payload_cells(terminal.backend().buffer(), payload);
            for cell in cells {
                assert_eq!(cell.fg, color, "{diff_layout:?}: {payload}");
                assert!(!cell.modifier.contains(Modifier::DIM), "{payload}");
            }
        }
        let line = toggle(&character);
        terminal
            .draw(|frame| render_prepared_frame(frame, tree, file, &extent, &line, THEME))
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
            keymap: KeymapPopup::Closed,
            page: PageIndex::FIRST,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
            viewport: Viewport::at_start(Size::new(80, 9)),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 9)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &document, &interaction, THEME))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        for y in [3, 6] {
            let hunk_header = &rendered[(30, y)];

            assert_eq!(hunk_header.symbol(), "@");
            assert_eq!(hunk_header.fg, THEME.rows.hunk_header);
            assert_eq!(hunk_header.bg, Color::Reset);
        }
        assert_eq!(
            rendered[(30, 1)].fg,
            Color::Reset,
            "file header stays neutral"
        );
    }

    fn two_file_document() -> crate::document::DiffDocument {
        parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-before\n+after\n",
        )
        .expect("valid two-file diff")
    }

    fn render(interaction: &Interaction, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");

        terminal
            .draw(|frame| render_document(frame, &two_file_document(), interaction, THEME))
            .expect("render frame");
        terminal.backend().buffer().clone()
    }

    fn interaction(
        file_list: FileList,
        keymap: KeymapPopup,
        width: u16,
        height: u16,
    ) -> Interaction {
        Interaction {
            keymap,
            page: PageIndex::FIRST,
            preferences: ViewPreferences {
                file_list,
                ..ViewPreferences::initial(DiffLayout::Unified)
            },
            viewport: Viewport::at_start(Size::new(width, height)),
        }
    }

    fn row_text(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    fn screen_text(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| row_text(buffer, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// architecture.md (Interaction model): a `? help` box floats over the bottom-left
    /// corner of the diff while the file list is hidden, by `e` or by a narrow screen.
    #[test]
    fn floats_a_help_box_while_the_file_list_is_hidden() {
        let hidden = render(
            &interaction(FileList::Hidden, KeymapPopup::Closed, 80, 10),
            80,
            10,
        );
        let narrow = render(
            &interaction(FileList::Shown, KeymapPopup::Closed, 23, 10),
            23,
            10,
        );

        for rendered in [&hidden, &narrow] {
            assert!(row_text(rendered, 7).starts_with("┌────────┐"));
            assert!(row_text(rendered, 8).starts_with("│ ? help │"));
            assert!(row_text(rendered, 9).starts_with("└────────┘"));
            assert_eq!(
                screen_text(rendered).matches("? help").count(),
                1,
                "one help hint"
            );
        }
        assert_eq!(
            hidden[(1, 4)].symbol(),
            "-",
            "deletion marker at the left edge"
        );
        assert!(!screen_text(&hidden).contains("second"), "no file list");
    }

    /// architecture.md (Interaction model): the popup shows only while it is open.
    #[test]
    fn draws_the_keymap_popup_only_while_it_is_open() {
        let text = |keymap| {
            screen_text(&render(
                &interaction(FileList::Shown, keymap, 120, 30),
                120,
                30,
            ))
        };

        assert!(text(KeymapPopup::Open).contains("cycle layout"));
        assert!(!text(KeymapPopup::Closed).contains("cycle layout"));
    }
}
