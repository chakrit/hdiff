//! Draws one frame: the diff panes of the shown page, the file-list column or the help box,
//! and the keymap popup. Rules: `docs/spec/architecture.md` (Interaction model, View model,
//! Viewport coordinates).

use ratatui::{Frame, layout::Rect, widgets::Paragraph};

use crate::{
    effective_view::TooSmall,
    extent::{ScreenLine, ShownLines, WrapWidth},
    geometry::{SplitDirection, SplitPanes},
    interaction::{Interaction, KeymapPopup},
    navigation::Navigation,
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

/// Draws `document` as `navigation` shows `interaction`: the page of its scroll extent
/// through its effective view, beside the file list.
pub(super) fn render_prepared_frame(
    frame: &mut Frame,
    document: &PreparedDocument,
    navigation: &Navigation,
    interaction: &Interaction,
    theme: &Theme,
) {
    let view = navigation.view();
    let geometry = view.geometry();
    let screen = geometry.screen();
    if let Some(too_small) = view.too_small() {
        frame.render_widget(Paragraph::new(too_small_message(too_small)), screen);
        return;
    }

    let extent = navigation.extent();
    let page = document.page(extent.page());
    let scroll = interaction.scroll().position();
    let granularity = interaction.preferences().granularity;
    let pane_lines = view.pane_content().height;
    let panes = PaneRows {
        window: LineWindow {
            first: scroll.vertical,
            total: extent.line_count(),
        },
        wrap: extent.wrap(),
        horizontal_offset: scroll.horizontal,
        theme,
    };
    match extent.lines(scroll.vertical, pane_lines) {
        ShownLines::Single(lines) => {
            let lines =
                lines.map(|line| line.map(|line| PaneRow::Text(page.row(line, granularity))));
            let pane = ContinuityFrame::diff_pane(geometry.single_pane());
            panes.render(frame, pane, lines);
        }
        ShownLines::Split { direction, lines } => {
            let lines = lines
                .map(|line| {
                    line.map(|pair| {
                        page.rows()
                            .pair(pair)
                            .map(|line| page.row(line, granularity))
                    })
                })
                .collect();
            let split = geometry.split_panes(direction);
            render_split_rows(frame, lines, split, direction, &panes);
        }
    }

    match geometry.file_column() {
        Some(column) => {
            let tree = document.file_tree();
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

/// Draws side-by-side lines in the old and new panes of `split`, split in `direction`,
/// with their separator.
fn render_split_rows(
    frame: &mut Frame,
    lines: Vec<ScreenLine<SideBySideRow<StyledRow>>>,
    split: SplitPanes,
    direction: SplitDirection,
    pane_rows: &PaneRows,
) {
    let panes = split.panes.map(ContinuityFrame::diff_pane);
    let shown = ScreenLines::new(lines.len());
    let (old_lines, new_lines): (Vec<_>, Vec<_>) = lines
        .into_iter()
        .map(|line| {
            let rows = line.row.pane_pair();
            (line.map(|_| rows.old), line.map(|_| rows.new))
        })
        .unzip();

    let palette = &pane_rows.theme.chrome;
    match direction {
        // The separator runs beside the shown lines only.
        SplitDirection::Vertical => {
            let content = panes.old.content();
            let height = content.height.min(frame_rows(shown));
            let separator = Rect::new(split.separator.x, content.y, split.separator.width, height);
            draw_inner_separator(frame.buffer_mut(), separator, direction, palette);
        }
        SplitDirection::Stacked => {
            draw_inner_separator(frame.buffer_mut(), split.separator, direction, palette);
        }
    }
    pane_rows.render(frame, panes.old, old_lines);
    pane_rows.render(frame, panes.new, new_lines);
}

/// Frame rows `lines` take; lines are collected up to a pane's height, so they fit.
fn frame_rows(lines: ScreenLines) -> u16 {
    u16::try_from(lines.get()).expect("a pane's lines fit its height")
}
