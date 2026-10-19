//! Draws one frame: the diff panes of the shown page, the file-list column or the help
//! box, and the keymap popup. Rules: docs/spec/architecture.md#interaction-model and
//! docs/spec/architecture.md#view-model and
//! docs/spec/architecture.md#viewport-coordinates.

use ratatui::{Frame, layout::Rect, widgets::Paragraph};

use crate::{
    effective_view::{TooSmall, WrapWidth},
    extent::{RowLine, ShownLines},
    geometry::{SplitDirection, SplitPanes},
    navigation::{Navigation, interaction::KeymapPopup},
    page_rows::SideBySideRow,
    prepared::PreparedDocument,
    screen::{Cells, ScreenLines},
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

/// Draws `document` as `navigation` shows its interaction: the page of its scroll extent
/// through its effective view, beside the file list.
pub(super) fn render_prepared_frame(
    frame: &mut Frame,
    document: &PreparedDocument,
    navigation: &Navigation,
    theme: &Theme,
) {
    let interaction = navigation.interaction();
    let view = navigation.view();
    let geometry = view.geometry();
    let screen = geometry.screen();
    if let Some(too_small) = view.too_small() {
        frame.render_widget(Paragraph::new(too_small_message(too_small)), screen);
        return;
    }

    let extent = navigation.extent();
    let scroll = interaction.scroll().position();
    let granularity = interaction.preferences().granularity;
    let pane_lines = view.pane_content().height;
    let pane_drawing = PaneDrawing {
        window: LineWindow {
            first: scroll.vertical,
            total: extent.line_count(),
        },
        wrap: extent.wrap(),
        horizontal_offset: scroll.horizontal,
        theme,
    };
    match extent.shown(document, granularity, scroll.vertical, pane_lines) {
        ShownLines::Single(lines) => {
            let lines = lines.map(|screen_line| screen_line.map(PaneRow::Text));
            let pane = ContinuityFrame::diff_pane(geometry.single_pane());
            pane_drawing.render(frame, pane, lines);
        }
        ShownLines::Split { direction, lines } => {
            let split = geometry.split_panes(direction);
            render_split_rows(frame, lines, split, &pane_drawing);
        }
    }

    match geometry.file_column() {
        Some(column) => {
            let tree = document.file_tree();
            let page_kind = document.page(interaction.page()).kind();
            render_file_column(frame, column.list, tree, page_kind, &theme.chrome);
            draw_outer_separator(frame.buffer_mut(), column.separator, &theme.chrome);
        }
        None => render_help_box(frame, screen, &theme.chrome),
    }
    if let KeymapPopup::Open = interaction.keymap() {
        render_keymap_popup(frame, screen, theme);
    }
}

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

    fn placement(&self, wrapped_line: ScreenLines) -> RowPlacement {
        match self.wrap {
            // An unbounded row is one line, which only the horizontal offset places.
            WrapWidth::Unbounded => RowPlacement::Scrolled {
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
    let (old_lines, new_lines): (Vec<_>, Vec<_>) = lines
        .map(|line| line.map(SideBySideRow::pane_pair).split())
        .map(|sides| (sides.old, sides.new))
        .unzip();
    let shown = ScreenLines::new(old_lines.len());

    let palette = &pane_drawing.theme.chrome;
    match direction {
        // The separator runs beside the shown lines only.
        SplitDirection::Vertical => {
            let content = panes.old.content();
            let height = shown_rows(content.height, shown);
            let shown_separator = Rect::new(separator.x, content.y, separator.width, height);
            draw_inner_separator(frame.buffer_mut(), shown_separator, direction, palette);
        }
        SplitDirection::Stacked => {
            draw_inner_separator(frame.buffer_mut(), separator, direction, palette);
        }
    }
    pane_drawing.render(frame, panes.old, old_lines);
    pane_drawing.render(frame, panes.new, new_lines);
}

/// Rows of a `height`-row area that `lines` fill: all of it when they reach past it.
fn shown_rows(height: u16, lines: ScreenLines) -> u16 {
    ScreenLines::from(height).min(lines).frame_rows()
}
