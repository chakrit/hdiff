//! Frame chrome around the diff panes: the file list, footer hints, the `? help` box,
//! separators, and continuity markers.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout as RatatuiLayout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState, Padding, Paragraph},
};

use super::row_writer::Clip;
use crate::{
    file_identity::FileState,
    file_tree::{FileTree, Marker, TreeRow},
    layout::{PANE_FRAME_HEIGHT, PANE_FRAME_WIDTH},
};

const MINIMUM_FILE_LIST_HEIGHT: u16 = 3;

const FOOTER_HINTS: [&str; 6] = [
    " hjkl  move",
    " v     layout",
    " Tab   file",
    " e     files",
    " ?     help",
    " q     quit",
];

const HELP_BOX_TEXT: &str = "? help";
/// The text inside a border and a padding column on each side, between two border rows.
const HELP_BOX_WIDTH: u16 = HELP_BOX_TEXT.len() as u16 + 4;
const HELP_BOX_HEIGHT: u16 = 3;

/// Draws the file list above the footer hints in the file-list column.
pub(super) fn render_file_column(
    frame: &mut Frame,
    area: Rect,
    tree: &FileTree,
    selected_row: usize,
    color_count: u16,
) {
    let palette = chrome_palette(color_count);
    let list_height = MINIMUM_FILE_LIST_HEIGHT + FRAME_ROWS;
    let footer_height = area
        .height
        .saturating_sub(list_height)
        .min(FOOTER_HINTS.len() as u16);
    let [list_area, footer_area] = RatatuiLayout::vertical([
        Constraint::Min(list_height),
        Constraint::Length(footer_height),
    ])
    .areas(area);

    render_file_list(frame, list_area, tree, selected_row, color_count);
    render_footer(frame.buffer_mut(), footer_area, palette);
}

/// The floating `? help` box over the bottom-left corner of `area`, shown in place of the
/// footer hints while the file list is hidden.
pub(super) fn render_help_box(frame: &mut Frame, area: Rect, color_count: u16) {
    let palette = chrome_palette(color_count);
    let help_box = Rect::new(
        area.x,
        area.bottom().saturating_sub(HELP_BOX_HEIGHT),
        HELP_BOX_WIDTH,
        HELP_BOX_HEIGHT,
    )
    .intersection(area);

    let border = Block::bordered()
        .border_style(Style::default().fg(palette.separator))
        .padding(Padding::horizontal(1));
    let text = Line::styled(HELP_BOX_TEXT, Style::default().fg(palette.footer));

    frame.render_widget(Clear, help_box);
    frame.render_widget(Paragraph::new(text).block(border), help_box);
}

/// The full-height separator between the file-list column and the diff panes.
pub(super) fn draw_outer_separator(buffer: &mut Buffer, area: Rect, color_count: u16) {
    let style = Style::default().fg(chrome_palette(color_count).separator);

    fill(buffer, area, "│", style);
}

/// The separator between the before and after panes of a side-by-side layout.
pub(super) fn draw_inner_separator(
    buffer: &mut Buffer,
    area: Rect,
    symbol: &'static str,
    color_count: u16,
) {
    let style = Style::default().fg(chrome_palette(color_count).dim_separator);

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
    pub(super) first: usize,
    /// Lines in the scroll extent.
    pub(super) total: usize,
}

impl ContinuityFrame {
    /// A diff pane: one frame row above and below and one frame column on either side.
    pub(super) fn diff_pane(pane: Rect) -> Self {
        let rows = between_frame_rows(pane);
        let content = Rect::new(
            rows.x + PANE_FRAME_WIDTH / 2,
            rows.y,
            rows.width.saturating_sub(PANE_FRAME_WIDTH),
            rows.height,
        );

        Self { pane, content }
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
    pub(super) fn draw_side_markers(&self, buffer: &mut Buffer, clip: Clip, color_count: u16) {
        if self.content.is_empty() {
            return;
        }
        let style = Style::default().fg(chrome_palette(color_count).dim_separator);
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
        color_count: u16,
    ) {
        if self.content.is_empty() {
            return;
        }
        let style = Style::default().fg(chrome_palette(color_count).dim_separator);
        let center = self.pane.x + self.pane.width / 2;
        let shown = usize::from(self.content.height);

        if window.first > 0 {
            buffer[(center, self.pane.top())]
                .set_symbol("▴")
                .set_style(style);
        }
        if window.first + shown < window.total {
            buffer[(center, self.pane.bottom() - 1)]
                .set_symbol("▾")
                .set_style(style);
        }
    }
}

/// `area` without its frame rows above and below.
fn between_frame_rows(area: Rect) -> Rect {
    Rect::new(
        area.x,
        area.y + FRAME_ROWS / 2,
        area.width,
        area.height.saturating_sub(FRAME_ROWS),
    )
}

const FRAME_ROWS: u16 = PANE_FRAME_HEIGHT as u16;

/// Lists only the rows that fit: a fresh `ListState` scrolls just far enough to keep
/// the selected row on the last row, so listing that window draws the same frame.
fn render_file_list(
    frame: &mut Frame,
    area: Rect,
    tree: &FileTree,
    selected_row: usize,
    color_count: u16,
) {
    let palette = chrome_palette(color_count);
    let list_frame = ContinuityFrame::file_list(area);
    let content = list_frame.content();
    let height = usize::from(content.height);
    let first = (selected_row + 1).saturating_sub(height);
    let window = LineWindow {
        first,
        total: tree.rows().len(),
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
    let mut state = ListState::default().with_selected(Some(selected_row - first));

    frame.render_stateful_widget(file_list, content, &mut state);
    list_frame.draw_window_markers(frame.buffer_mut(), window, color_count);
}

fn tree_item(row: &TreeRow, palette: ChromePalette) -> ListItem<'_> {
    let line = match row {
        TreeRow::Directory(heading) => Line::styled(
            heading.as_str(),
            Style::default()
                .fg(palette.directory_heading)
                .add_modifier(Modifier::BOLD),
        ),
        TreeRow::File { marker, name } => Line::from(vec![
            Span::styled(marker.label(), Style::default().fg(palette.marker(*marker))),
            Span::raw(" "),
            Span::raw(name.as_str()),
        ])
        .style(Style::default().fg(palette.file_name)),
    };

    ListItem::new(line)
}

fn render_footer(buffer: &mut Buffer, area: Rect, palette: ChromePalette) {
    let style = Style::default().fg(palette.footer);

    buffer.set_style(area, style);
    for (y, hint) in area.rows().map(|row| row.y).zip(FOOTER_HINTS) {
        buffer.set_stringn(area.x, y, hint, usize::from(area.width), style);
    }
}

fn fill(buffer: &mut Buffer, area: Rect, symbol: &'static str, style: Style) {
    for position in area.positions() {
        buffer[position].set_symbol(symbol).set_style(style);
    }
}

#[derive(Clone, Copy)]
pub(super) struct ChromePalette {
    directory_heading: Color,
    pub(super) file_name: Color,
    modified_marker: Color,
    added_marker: Color,
    deleted_marker: Color,
    renamed_marker: Color,
    selected_file: Color,
    selected_file_background: Color,
    pub(super) separator: Color,
    /// The inner separator between side-by-side panes and the continuity markers.
    dim_separator: Color,
    footer: Color,
}

impl ChromePalette {
    fn marker(self, marker: Marker) -> Color {
        match marker {
            Marker::State(FileState::Modified) => self.modified_marker,
            Marker::State(FileState::Added) => self.added_marker,
            Marker::State(FileState::Deleted) => self.deleted_marker,
            Marker::State(FileState::Renamed) => self.renamed_marker,
            Marker::Unknown => self.file_name,
        }
    }
}

pub(super) fn chrome_palette(color_count: u16) -> ChromePalette {
    match color_count {
        u16::MAX => ChromePalette {
            directory_heading: Color::Rgb(125, 125, 125),
            file_name: Color::Rgb(160, 160, 160),
            modified_marker: Color::Rgb(190, 170, 120),
            added_marker: Color::Rgb(130, 185, 130),
            deleted_marker: Color::Rgb(200, 130, 130),
            renamed_marker: Color::Rgb(130, 170, 205),
            selected_file: Color::Rgb(220, 220, 220),
            selected_file_background: Color::Rgb(65, 65, 65),
            separator: Color::Rgb(125, 125, 125),
            dim_separator: Color::Rgb(95, 95, 95),
            footer: Color::Rgb(110, 110, 110),
        },
        256.. => ChromePalette {
            directory_heading: Color::Indexed(243),
            file_name: Color::Indexed(246),
            modified_marker: Color::Indexed(144),
            added_marker: Color::Indexed(108),
            deleted_marker: Color::Indexed(138),
            renamed_marker: Color::Indexed(110),
            selected_file: Color::Indexed(255),
            selected_file_background: Color::Indexed(238),
            separator: Color::Indexed(243),
            dim_separator: Color::Indexed(238),
            footer: Color::Indexed(240),
        },
        _ => ChromePalette {
            directory_heading: Color::DarkGray,
            file_name: Color::Gray,
            modified_marker: Color::Yellow,
            added_marker: Color::Green,
            deleted_marker: Color::Red,
            renamed_marker: Color::Blue,
            selected_file: Color::White,
            selected_file_background: Color::DarkGray,
            separator: Color::DarkGray,
            dim_separator: Color::DarkGray,
            footer: Color::DarkGray,
        },
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    use super::render_file_column;
    use crate::{parser::parse_unified_diff, prepared::PreparedDocument};

    /// architecture.md (Keyboard bindings): the footer lists one shortcut per row below the
    /// file list.
    #[test]
    fn lists_compact_shortcuts_below_the_file_list() {
        let document = parse_unified_diff(b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n")
            .expect("valid diff");
        let prepared = PreparedDocument::prepare(&document);
        let mut terminal = Terminal::new(TestBackend::new(24, 18)).expect("test terminal");

        terminal
            .draw(|frame| {
                render_file_column(frame, frame.area(), prepared.file_tree(), 0, u16::MAX)
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
        assert_eq!(
            (12..18).map(row).collect::<Vec<_>>(),
            [
                " hjkl  move",
                " v     layout",
                " Tab   file",
                " e     files",
                " ?     help",
                " q     quit"
            ]
        );
        assert_eq!(buffer[(1, 12)].fg, Color::Rgb(110, 110, 110), "footer tone");
    }
}
