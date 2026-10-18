//! Frame chrome around the diff panes: the file list, the `? help` hint, separators, and
//! continuity markers.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState, Padding, Paragraph},
};

use super::row_writer::Clip;
use crate::{
    file_tree::{FileTree, TreeRow},
    geometry::{PANE_FRAME_HEIGHT, between_frame_rows, pane_content},
    prepared::PageKind,
    screen::ScreenLines,
    theme::ChromePalette,
};

const MINIMUM_FILE_LIST_HEIGHT: u16 = 3;

const HELP_HINT: &str = "? help";
/// A border and a padding column on each side of the hint.
const HELP_BOX_FRAME_WIDTH: u16 = 4;
const HELP_BOX_HEIGHT: u16 = 3;

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
        PageKind::File { row } => Some(row.get()),
        PageKind::Text => None,
    };
    let list_height = MINIMUM_FILE_LIST_HEIGHT + PANE_FRAME_HEIGHT;
    let hint_height = area.height.saturating_sub(list_height).min(1);
    let [list_area, hint_area] = Layout::vertical([
        Constraint::Min(list_height),
        Constraint::Length(hint_height),
    ])
    .areas(area);

    render_file_list(frame, list_area, tree, selected_row, palette);
    render_help_hint(frame.buffer_mut(), hint_area, palette);
}

/// The help hint in a box floating over the bottom-left corner of `area`, while the file list
/// is hidden.
pub(super) fn render_help_box(frame: &mut Frame, area: Rect, palette: &ChromePalette) {
    let hint_width = u16::try_from(HELP_HINT.len()).expect("the help hint fits a row");
    let help_box = Rect::new(
        area.x,
        area.bottom().saturating_sub(HELP_BOX_HEIGHT),
        hint_width + HELP_BOX_FRAME_WIDTH,
        HELP_BOX_HEIGHT,
    )
    .intersection(area);

    let border = Block::bordered()
        .border_style(Style::default().fg(palette.separator))
        .padding(Padding::horizontal(1));
    let text = Line::styled(HELP_HINT, Style::default().fg(palette.hint));

    frame.render_widget(Clear, help_box);
    frame.render_widget(Paragraph::new(text).block(border), help_box);
}

/// The full-height separator between the file-list column and the diff panes.
pub(super) fn draw_outer_separator(buffer: &mut Buffer, area: Rect, palette: &ChromePalette) {
    let style = Style::default().fg(palette.separator);

    fill(buffer, area, "│", style);
}

/// The separator between the before and after panes of a side-by-side layout.
pub(super) fn draw_inner_separator(
    buffer: &mut Buffer,
    area: Rect,
    symbol: &'static str,
    palette: &ChromePalette,
) {
    let style = Style::default().fg(palette.dim_separator);

    fill(buffer, area, symbol, style);
}

/// A content area inside a continuity-marker frame. Rules: `docs/spec/architecture.md`
/// (Continuity markers).
#[derive(Clone, Copy)]
pub(super) struct ContinuityFrame {
    pane: Rect,
    content: Rect,
}

/// The visible lines of a pane among all the lines of its scroll extent.
#[derive(Clone, Copy)]
pub(super) struct LineWindow {
    /// The first visible line.
    pub(super) first: ScreenLines,
    /// Lines in the scroll extent.
    pub(super) total: ScreenLines,
}

impl ContinuityFrame {
    /// A diff pane: one frame row above and below and one frame column on either side.
    pub(super) fn diff_pane(pane: Rect) -> Self {
        Self {
            pane,
            content: pane_content(pane),
        }
    }

    /// The file list: one frame row above and below, and no frame columns.
    fn file_list(area: Rect) -> Self {
        Self {
            pane: area,
            content: between_frame_rows(area),
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

/// Lists only the rows that fit: a fresh `ListState` scrolls just far enough to keep
/// the selected row on the last row, so listing that window draws the same frame.
fn render_file_list(
    frame: &mut Frame,
    area: Rect,
    tree: &FileTree,
    selected_row: Option<usize>,
    palette: &ChromePalette,
) {
    let list_frame = ContinuityFrame::file_list(area);
    let content = list_frame.content();
    let height = usize::from(content.height);
    // Without a selected row, the list shows from its top.
    let first = selected_row.map_or(0, |row| (row + 1).saturating_sub(height));
    let window = LineWindow {
        first: ScreenLines::new(first),
        total: ScreenLines::new(tree.rows().len()),
    };

    let items = tree
        .rows()
        .iter()
        .skip(first)
        .take(height)
        .map(|row| tree_item(row, palette));
    let file_list = List::new(items).highlight_style(
        Style::default()
            .fg(palette.selected_file)
            .bg(palette.selected_file_background),
    );
    let mut state = ListState::default().with_selected(selected_row.map(|row| row - first));

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

fn render_help_hint(buffer: &mut Buffer, area: Rect, palette: &ChromePalette) {
    if area.is_empty() {
        return;
    }
    let style = Style::default().fg(palette.hint);

    buffer.set_style(area, style);
    buffer.set_stringn(
        area.x + 1,
        area.y,
        HELP_HINT,
        usize::from(area.width) - 1,
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

    /// architecture.md (Interaction model): the help hint sits on the bottom row of the file
    /// list.
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
