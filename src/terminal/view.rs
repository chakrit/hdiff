//! Draws one frame: the diff panes of the shown page, the file-list column or the help box,
//! and the keymap popup. Rules: `docs/spec/architecture.md` (Interaction model, View model,
//! Viewport coordinates).

use ratatui::{Frame, layout::Rect, widgets::Paragraph};

use crate::{
    extent::{DisplayExtent, ScreenLine, WrapWidth},
    file_tree::FileTree,
    geometry::{SplitDirection, SplitPanes, TooSmall},
    interaction::{Interaction, KeymapPopup},
    page_rows::SideBySideRow,
    prepared::PreparedPage,
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
    rows::{PaneRow, visible_diff_rows, visible_side_by_side_rows},
};

/// Draws `page` through `extent`, the scroll extent of the interaction's display, beside
/// the file list of `tree`.
pub(super) fn render_prepared_frame(
    frame: &mut Frame,
    tree: &FileTree,
    page: &PreparedPage,
    extent: &DisplayExtent,
    interaction: &Interaction,
    theme: &Theme,
) {
    let geometry = interaction.display().geometry();
    let screen = geometry.screen();
    // Bounds and drawing share the interaction's geometry, so a frame of another size waits
    // for the resize event that follows it.
    if frame.area() != screen {
        return;
    }
    let shape = extent.shape();
    if let Some(too_small) = geometry.too_small(shape) {
        frame.render_widget(Paragraph::new(too_small_message(too_small)), screen);
        return;
    }

    let scroll = interaction.scroll();
    let granularity = interaction.preferences().granularity;
    let pane_lines = geometry.pane_content(shape).height;
    let panes = PaneRows {
        window: LineWindow {
            first: scroll.vertical,
            total: extent.line_count(),
        },
        wrap: extent.wrap(),
        horizontal_offset: scroll.horizontal,
        theme,
    };
    match extent {
        DisplayExtent::Single(lines) => {
            let lines = visible_diff_rows(page, lines, scroll.vertical, granularity, pane_lines);
            let pane = ContinuityFrame::diff_pane(geometry.single_pane());
            panes.render(
                frame,
                pane,
                lines.into_iter().map(|line| line.map(PaneRow::Text)),
            );
        }
        DisplayExtent::Split { direction, extent } => {
            let lines =
                visible_side_by_side_rows(page, extent, scroll.vertical, granularity, pane_lines);
            let split = geometry.split_panes(*direction);
            render_split_rows(frame, lines, split, *direction, &panes);
        }
    }

    match geometry.file_column() {
        Some(column) => {
            render_file_column(frame, column.list, tree, page.kind(), &theme.chrome);
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
struct PaneRows<'a> {
    window: LineWindow,
    wrap: WrapWidth,
    horizontal_offset: Cells,
    theme: &'a Theme,
}

impl PaneRows<'_> {
    fn render<'a>(
        &self,
        frame: &mut Frame,
        pane: ContinuityFrame,
        lines: impl IntoIterator<Item = ScreenLine<PaneRow<'a>>>,
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

    fn placement(&self, wrapped_line: usize) -> RowPlacement {
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

/// Draws side-by-side lines in the before and after panes of `panes`, split in
/// `direction`, with their separator.
fn render_split_rows(
    frame: &mut Frame,
    lines: Vec<ScreenLine<SideBySideRow<StyledRow>>>,
    panes: SplitPanes,
    direction: SplitDirection,
    pane_rows: &PaneRows,
) {
    let before_pane = ContinuityFrame::diff_pane(panes.before);
    let after_pane = ContinuityFrame::diff_pane(panes.after);
    let shown = ScreenLines::new(lines.len());
    let (before_lines, after_lines): (Vec<_>, Vec<_>) = lines
        .into_iter()
        .map(|line| {
            let (before, after) = line.row.pane_pair();
            (line.map(|_| before), line.map(|_| after))
        })
        .unzip();

    let palette = &pane_rows.theme.chrome;
    match direction {
        // The separator runs beside the shown lines only.
        SplitDirection::SideBySide => {
            let content = before_pane.content();
            let height = content.height.min(frame_rows(shown));
            let separator = Rect::new(panes.separator.x, content.y, panes.separator.width, height);
            draw_inner_separator(frame.buffer_mut(), separator, "│", palette);
        }
        SplitDirection::Stacked => {
            draw_inner_separator(frame.buffer_mut(), panes.separator, "─", palette);
        }
    }
    pane_rows.render(frame, before_pane, before_lines);
    pane_rows.render(frame, after_pane, after_lines);
}

/// Frame rows `lines` take; lines are collected up to a pane's height, so they fit.
fn frame_rows(lines: ScreenLines) -> u16 {
    u16::try_from(lines.get()).expect("a pane's lines fit its height")
}
