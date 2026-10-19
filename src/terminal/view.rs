//! Draws one frame: the diff panes of the shown page, the file-list column or the help
//! box, and the keymap popup. Rules: docs/spec/architecture.md#interaction-model and
//! docs/spec/architecture.md#view-model and
//! docs/spec/architecture.md#viewport-coordinates.

use ratatui::{Frame, layout::Rect, widgets::Paragraph};

use crate::{
    effective_view::WrapWidth,
    geometry::{SplitDirection, SplitPanes, TooSmall},
    navigation::{
        Navigation,
        extent::{RowLine, ShownLines},
        interaction::KeymapPopup,
    },
    page_rows::SideBySideRow,
    screen::{Cells, ScreenLines},
    side::BySide,
    styling::StyledRow,
    theme::Theme,
};

use super::{
    chrome::{
        ContinuityFrame, LineWindow, draw_inner_separator, draw_outer_separator,
        render_file_column, render_help_box,
    },
    keymap_popup::render_keymap_popup,
    row_writer::{Clip, RowPlacement, write_row},
    rows::PaneRow,
};

/// Draws the document as `navigation` shows its interaction: the page of its scroll
/// extent through its effective view, beside the file list.
pub(super) fn render_prepared_frame(frame: &mut Frame, navigation: &Navigation, theme: &Theme) {
    let interaction = navigation.interaction();
    let view = navigation.view();
    let geometry = view.geometry();
    let screen = geometry.screen();

    if let Some(too_small) = geometry.too_small() {
        frame.render_widget(Paragraph::new(too_small_message(too_small)), screen);
        return;
    }

    let extent = navigation.extent();
    let scroll = interaction.scroll().position();
    let pane_drawing = PaneDrawing {
        window: LineWindow {
            first: scroll.vertical,
            total: extent.line_count(),
        },
        wrap: extent.wrap(),
        horizontal_offset: scroll.horizontal,
        theme,
    };

    // The shown lines carry the shape they were measured for, so the diff area is divided
    // by that shape.
    let diff = geometry.diff();
    match navigation.shown() {
        ShownLines::Single(lines) => {
            let pane_lines = lines.map(|screen_line| screen_line.map(PaneRow::Text));
            let pane = ContinuityFrame::diff_pane(diff);
            pane_drawing.render(frame, pane, pane_lines);
        }
        ShownLines::Split { direction, lines } => {
            let split = SplitPanes::of(diff, direction);
            render_split_rows(frame, lines, split, &pane_drawing);
        }
    }

    match geometry.file_column() {
        Some(column) => {
            let selection = navigation.file_selection();
            render_file_column(frame, column.list, selection, &theme.chrome);
            draw_outer_separator(frame.buffer_mut(), column.separator, &theme.chrome);
        }
        None => render_help_box(frame, diff, &theme.chrome),
    }
    match interaction.keymap() {
        KeymapPopup::Open => render_keymap_popup(frame, diff, theme),
        KeymapPopup::Closed => {}
    }
}

/// The only content a screen too small for the minimum layout renders. Rules:
/// docs/spec/architecture.md#interaction-model.
const fn too_small_message(too_small: TooSmall) -> &'static str {
    match too_small {
        TooSmall::Narrow => "screen too narrow",
        TooSmall::Short => "screen too short",
    }
}

/// Draws one pane's lines inside its continuity-marker frame, at one wrap width and
/// horizontal offset.
struct PaneDrawing<'a> {
    window: LineWindow,
    wrap: WrapWidth,
    horizontal_offset: Cells,
    theme: &'a Theme,
}

impl PaneDrawing<'_> {
    /// Writes `lines` into `pane`'s content rows, then marks the frame where rows were
    /// clipped and where the window has lines past an edge.
    fn render<'a>(
        &self,
        frame: &mut Frame,
        pane: ContinuityFrame,
        lines: impl IntoIterator<Item = RowLine<PaneRow<'a>>>,
    ) {
        let buffer = frame.buffer_mut();
        let mut clipped = Clip::default();

        for (row_area, line) in pane.content().rows().zip(lines) {
            let clip = write_row(
                buffer,
                row_area,
                line.row,
                self.placement(line.wrapped_line),
                self.theme,
            );
            clipped = clipped.union(clip);
        }

        pane.draw_side_markers(buffer, clipped, &self.theme.chrome);
        pane.draw_window_markers(buffer, self.window, &self.theme.chrome);
    }

    /// Where wrapped line `wrapped_line` of a row is placed: at the horizontal offset
    /// when rows are unwrapped, or as that line of the row broken at the wrap width.
    fn placement(&self, wrapped_line: ScreenLines) -> RowPlacement {
        match self.wrap {
            WrapWidth::Off | WrapWidth::NoPayload => RowPlacement::Scrolled {
                horizontal_offset: self.horizontal_offset,
            },
            WrapWidth::At(width) => RowPlacement::Wrapped {
                line: wrapped_line,
                width,
            },
        }
    }
}

/// Draws side-by-side lines in the old and new panes of `split`, with their separator.
fn render_split_rows<'a>(
    frame: &mut Frame,
    lines: impl Iterator<Item = RowLine<SideBySideRow<StyledRow<'a>>>>,
    split: SplitPanes,
    pane_drawing: &PaneDrawing,
) {
    let direction = split.direction();
    let separator = split.separator();
    let panes = split.panes().map(ContinuityFrame::diff_pane);
    let pair_lines = lines
        .map(|line| line.map(SideBySideRow::pane_pair))
        .collect::<Vec<_>>();
    let shown = ScreenLines::new(pair_lines.len());

    let palette = &pane_drawing.theme.chrome;
    match direction {
        // The separator runs beside the shown lines only, across the rows both panes'
        // content spans.
        SplitDirection::Vertical => {
            let content = panes
                .as_ref()
                .map(ContinuityFrame::content)
                .merge(Rect::union);
            let height = shown_rows(content.height, shown);
            let shown_separator = Rect::new(separator.x, content.y, separator.width, height);
            draw_inner_separator(frame.buffer_mut(), shown_separator, direction, palette);
        }
        SplitDirection::Stacked => {
            draw_inner_separator(frame.buffer_mut(), separator, direction, palette);
        }
    }
    let side_lines = BySide::of(|side| {
        pair_lines.iter().map(move |line| RowLine {
            row: *line.row.get(side),
            wrapped_line: line.wrapped_line,
        })
    });
    for (pane, pane_lines) in panes.zip(side_lines) {
        pane_drawing.render(frame, pane, pane_lines);
    }
}

/// Rows of a `height`-row area that `lines` fill: all of it when they reach past it.
fn shown_rows(height: u16, lines: ScreenLines) -> u16 {
    ScreenLines::from(height).min(lines).frame_rows()
}
