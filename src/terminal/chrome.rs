//! Frame chrome around the diff panes: the file list, the `? help` hint, separators,
//! continuity markers, and the floating box the help hint and keymap popup draw in.

use std::num::NonZeroUsize;

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect, Size},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState, Padding, Paragraph},
};

use super::row_writer::Clip;
use crate::{
    file_tree::{FileTree, TreeRow, TreeRowIndex},
    geometry::{PANE_FRAME_HEIGHT, SplitDirection, frame_interior, inside_frame_rows},
    prepared::PageKind,
    screen::{Cells, ScreenLines},
    theme::ChromePalette,
};

const MINIMUM_FILE_LIST_HEIGHT: u16 = 3;

const HELP_HINT: &str = "? help";
/// Columns before the file column's hint.
const HELP_HINT_INDENT: u16 = 1;
/// Lines the hint takes.
const HELP_HINT_HEIGHT: u16 = 1;

/// The border's column or row on each side of a floating box.
const FLOATING_BOX_BORDER: u16 = 1;
/// Columns between a floating box's border and its content on each side.
const FLOATING_BOX_PADDING: u16 = 1;

/// Draws the file list above the help hint in the file-list column, selecting the row of
/// the page's file; a text page selects none.
pub(super) fn render_file_column(
    frame: &mut Frame,
    area: Rect,
    tree: &FileTree,
    page: PageKind,
    palette: &ChromePalette,
) {
    let selected_row = match page {
        PageKind::File { row } => Some(row),
        PageKind::Text => None,
    };
    let list_height = MINIMUM_FILE_LIST_HEIGHT + PANE_FRAME_HEIGHT;
    let hint_height = area
        .height
        .saturating_sub(list_height)
        .min(HELP_HINT_HEIGHT);
    let [list_area, hint_area] = Layout::vertical([
        Constraint::Min(list_height),
        Constraint::Length(hint_height),
    ])
    .areas(area);

    render_file_list(frame, list_area, tree, selected_row, palette);
    draw_help_hint(frame.buffer_mut(), hint_area, palette);
}

/// The help hint in a box floating over the bottom-left corner of `area`, while the file
/// list is hidden.
pub(super) fn render_help_box(frame: &mut Frame, area: Rect, palette: &ChromePalette) {
    let hint_width = Cells::of_text(HELP_HINT).frame_columns();
    let help_box = FloatingBox::bottom_left_of(area, Size::new(hint_width, HELP_HINT_HEIGHT));

    let content = help_box.render(frame, palette);
    let text = Line::styled(HELP_HINT, Style::default().fg(palette.hint));
    frame.render_widget(Paragraph::new(text), content);
}

/// A bordered box with a padding column inside each side, drawn over the chrome and panes
/// it floats on, and clipped to the area it floats over.
pub(super) struct FloatingBox {
    area: Rect,
}

impl FloatingBox {
    /// Columns the border and padding take across a box.
    pub(super) const FRAME_WIDTH: u16 = 2 * (FLOATING_BOX_BORDER + FLOATING_BOX_PADDING);
    /// Rows the border takes down a box.
    const FRAME_HEIGHT: u16 = 2 * FLOATING_BOX_BORDER;

    /// A box around `content` in the bottom-left corner of `area`.
    fn bottom_left_of(area: Rect, content: Size) -> Self {
        let Size { width, height } = Self::framed(content);
        let box_area = Rect::new(area.x, area.bottom().saturating_sub(height), width, height);

        Self {
            area: box_area.intersection(area),
        }
    }

    /// A box around `content` centered over `area`.
    pub(super) fn centered_in(area: Rect, content: Size) -> Self {
        let Size { width, height } = Self::framed(content);
        let box_area = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );

        Self {
            area: box_area.intersection(area),
        }
    }

    /// The size of a box around `content`, with its border and padding.
    const fn framed(content: Size) -> Size {
        Size::new(
            content.width + Self::FRAME_WIDTH,
            content.height + Self::FRAME_HEIGHT,
        )
    }

    /// Clears the box and draws its border, returning the content area inside the border
    /// and padding.
    pub(super) fn render(self, frame: &mut Frame, palette: &ChromePalette) -> Rect {
        let border = Block::bordered()
            .border_style(Style::default().fg(palette.separator))
            .padding(Padding::horizontal(FLOATING_BOX_PADDING));
        let content = border.inner(self.area);

        frame.render_widget(Clear, self.area);
        frame.render_widget(border, self.area);
        content
    }
}

/// The full-height separator between the file-list column and the diff panes.
pub(super) fn draw_outer_separator(buffer: &mut Buffer, area: Rect, palette: &ChromePalette) {
    let style = Style::default().fg(palette.separator);

    fill(buffer, area, "│", style);
}

/// The separator between the before and after panes of a split layout, across the split.
pub(super) fn draw_inner_separator(
    buffer: &mut Buffer,
    area: Rect,
    direction: SplitDirection,
    palette: &ChromePalette,
) {
    let style = Style::default().fg(palette.dim_separator);
    let symbol = match direction {
        SplitDirection::Vertical => "│",
        SplitDirection::Stacked => "─",
    };

    fill(buffer, area, symbol, style);
}

/// A content area inside a continuity-marker frame. Rules:
/// docs/spec/architecture.md#continuity-markers.
#[derive(Clone, Copy)]
pub(super) struct ContinuityFrame {
    pane: Rect,
    content: Rect,
}

impl ContinuityFrame {
    /// A diff pane: one frame row above and below and one frame column on either side.
    pub(super) fn diff_pane(pane: Rect) -> Self {
        Self {
            pane,
            content: frame_interior(pane),
        }
    }

    /// The file list: one frame row above and below, and no frame columns.
    fn file_list(area: Rect) -> Self {
        Self {
            pane: area,
            content: inside_frame_rows(area),
        }
    }

    pub(super) fn content(&self) -> Rect {
        self.content
    }

    /// `◂` and `▸` midway down each side where any visible row was clipped.
    pub(super) fn draw_side_markers(
        &self,
        buffer: &mut Buffer,
        clip: Clip,
        palette: &ChromePalette,
    ) {
        if self.content.is_empty() {
            return;
        }
        let style = Style::default().fg(palette.dim_separator);
        let y = self.content.y + self.content.height / 2;

        if clip.left {
            buffer[(self.pane.left(), y)]
                .set_symbol("◂")
                .set_style(style);
        }
        if clip.right {
            buffer[(self.pane.right() - 1, y)]
                .set_symbol("▸")
                .set_style(style);
        }
    }

    /// `▴` above and `▾` below the content while `window` has lines past that edge.
    pub(super) fn draw_window_markers(
        &self,
        buffer: &mut Buffer,
        window: LineWindow,
        palette: &ChromePalette,
    ) {
        if self.content.is_empty() {
            return;
        }
        let style = Style::default().fg(palette.dim_separator);
        let center = self.pane.x + self.pane.width / 2;
        let shown = ScreenLines::from(self.content.height);

        if window.first > ScreenLines::ZERO {
            buffer[(center, self.pane.top())]
                .set_symbol("▴")
                .set_style(style);
        }
        if window.first.saturating_add(shown) < window.total {
            buffer[(center, self.pane.bottom() - 1)]
                .set_symbol("▾")
                .set_style(style);
        }
    }
}

/// The visible lines of a pane among all the lines of its scroll extent.
#[derive(Clone, Copy)]
pub(super) struct LineWindow {
    /// The first visible line.
    pub(super) first: ScreenLines,
    /// Lines in the scroll extent.
    pub(super) total: ScreenLines,
}

/// Lists only the rows that fit: a fresh `ListState` scrolls just far enough to keep
/// the selected row on the last row, so listing that window draws the same frame.
fn render_file_list(
    frame: &mut Frame,
    area: Rect,
    tree: &FileTree,
    selected_row: Option<TreeRowIndex>,
    palette: &ChromePalette,
) {
    let list_frame = ContinuityFrame::file_list(area);
    let content = list_frame.content();
    let Some(height) = NonZeroUsize::new(usize::from(content.height)) else {
        return;
    };

    let selection = selected_row.map(|row| row.window(height));
    // Without a selected row, the list shows from its top.
    let first = selection.map_or(TreeRowIndex::FIRST, |window| window.first);
    let window = LineWindow {
        first: ScreenLines::new(first.get()),
        total: ScreenLines::new(tree.rows().len()),
    };

    let items = tree
        .rows_from(first)
        .iter()
        .take(height.get())
        .map(|row| tree_item(row, palette));
    let file_list = List::new(items).highlight_style(
        Style::default()
            .fg(palette.selected_file)
            .bg(palette.selected_file_background),
    );
    let mut state =
        ListState::default().with_selected(selection.map(|window| window.selected_offset));

    frame.render_stateful_widget(file_list, content, &mut state);
    list_frame.draw_window_markers(frame.buffer_mut(), window, palette);
}

fn tree_item<'a>(row: &'a TreeRow, palette: &ChromePalette) -> ListItem<'a> {
    let line = match row {
        TreeRow::Directory(heading) => Line::styled(
            &**heading,
            Style::default()
                .fg(palette.directory_heading)
                .add_modifier(Modifier::BOLD),
        ),
        TreeRow::File { marker, name } => Line::from(vec![
            Span::styled(marker.label(), Style::default().fg(palette.marker(*marker))),
            Span::raw(" "),
            Span::raw(&**name),
        ])
        .style(Style::default().fg(palette.file_name)),
    };

    ListItem::new(line)
}

fn draw_help_hint(buffer: &mut Buffer, area: Rect, palette: &ChromePalette) {
    if area.is_empty() {
        return;
    }
    let style = Style::default().fg(palette.hint);

    buffer.set_style(area, style);
    buffer.set_stringn(
        area.x + HELP_HINT_INDENT,
        area.y,
        HELP_HINT,
        usize::from(area.width.saturating_sub(HELP_HINT_INDENT)),
        style,
    );
}

fn fill(buffer: &mut Buffer, area: Rect, symbol: &'static str, style: Style) {
    for position in area.positions() {
        buffer[position].set_symbol(symbol).set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::render_file_column;
    use crate::{
        parser::parse_patch,
        prepared::{PageIndex, PreparedDocument},
        theme::{ColorDepth, Theme},
    };

    /// docs/spec/architecture.md#interaction-model: the help hint sits on the bottom row
    /// of the file list.
    #[test]
    fn ends_the_file_list_with_the_help_hint() {
        let document = parse_patch(b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n")
            .expect("valid diff");
        let prepared = PreparedDocument::prepare(&document);
        let page = prepared.page(PageIndex::FIRST).kind();
        let mut terminal = Terminal::new(TestBackend::new(24, 18)).expect("test terminal");
        let palette = &Theme::for_depth(ColorDepth::TrueColor).chrome;

        terminal
            .draw(|frame| {
                render_file_column(frame, frame.area(), prepared.file_tree(), page, palette)
            })
            .expect("render file column");

        let buffer = terminal.backend().buffer();
        let row = |y| {
            (0..24)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        };
        let rows = (0..18).map(row).collect::<Vec<_>>();

        assert_eq!(rows[17], " ? help");
        assert!(
            rows[..17].iter().all(|row| !row.contains("help")),
            "one hint row"
        );
        assert_eq!(buffer[(1, 17)].fg, palette.hint, "hint tone");
    }
}
