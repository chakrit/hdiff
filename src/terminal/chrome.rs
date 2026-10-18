//! Frame chrome around the diff panes: the file list, the hint row, the keymap popup,
//! separators, and continuity markers.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph},
};

use super::row_writer::Clip;
use crate::{
    file_identity::FileState,
    file_tree::{FileTree, Marker, TreeRow},
    layout::{PANE_FRAME_HEIGHT, PANE_FRAME_WIDTH},
};

const HINTS: &str = "hjkl move · v layout · Tab file · e files · ? help · q quit";

/// Every shortcut, as keys and the action they take, in the keymap popup.
const KEYMAP: [(&str, &str); 15] = [
    ("j k ↓ ↑", "scroll down / up"),
    ("h l", "scroll left / right"),
    ("0 ^", "left edge"),
    ("$", "right end"),
    ("^D ^U", "half page down / up"),
    ("g G", "top / bottom"),
    ("Tab ⇧Tab", "next / previous file"),
    ("v", "cycle layout"),
    ("c", "character detail"),
    ("w", "wrap"),
    ("+ -", "more / less context"),
    ("e", "show / hide file list"),
    ("?", "open keymap"),
    ("? Esc q", "close keymap"),
    ("q ^C", "exit"),
];

const KEYMAP_KEYS_WIDTH: u16 = 9;
const KEYMAP_ACTION_WIDTH: u16 = 21;
/// A border and a padding column on either side of the keys and actions.
const KEYMAP_POPUP_WIDTH: u16 = 1 + 1 + KEYMAP_KEYS_WIDTH + 1 + KEYMAP_ACTION_WIDTH + 1 + 1;
const KEYMAP_POPUP_HEIGHT: u16 = KEYMAP.len() as u16 + 2;

/// The shortcut hint row across the bottom of the screen.
pub(super) fn render_hint_row(buffer: &mut Buffer, area: Rect, color_count: u16) {
    let style = Style::default().fg(chrome_palette(color_count).hints);

    buffer.set_style(area, style);
    buffer.set_stringn(area.x, area.y, HINTS, usize::from(area.width), style);
}

/// The popup listing every shortcut, centered over `area` and clipped to it.
pub(super) fn render_keymap_popup(frame: &mut Frame, area: Rect, color_count: u16) {
    let palette = chrome_palette(color_count);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(KEYMAP_POPUP_WIDTH) / 2,
        area.y + area.height.saturating_sub(KEYMAP_POPUP_HEIGHT) / 2,
        KEYMAP_POPUP_WIDTH,
        KEYMAP_POPUP_HEIGHT,
    )
    .intersection(area);
    let keys_width = usize::from(KEYMAP_KEYS_WIDTH);

    let lines = KEYMAP.map(|(keys, action)| {
        Line::from(vec![
            Span::styled(
                format!(" {keys:<keys_width$} "),
                Style::default().fg(palette.selected_file),
            ),
            Span::styled(action, Style::default().fg(palette.file_name)),
        ])
    });
    let border = Block::bordered().border_style(Style::default().fg(palette.separator));

    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(Vec::from(lines)).block(border), popup);
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

/// Draws the file list in the file-list column `area`. Lists only the rows that fit: a
/// fresh `ListState` scrolls just far enough to keep the selected row on the last row, so
/// listing that window draws the same frame.
pub(super) fn render_file_list(
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

fn fill(buffer: &mut Buffer, area: Rect, symbol: &'static str, style: Style) {
    for position in area.positions() {
        buffer[position].set_symbol(symbol).set_style(style);
    }
}

#[derive(Clone, Copy)]
struct ChromePalette {
    directory_heading: Color,
    file_name: Color,
    modified_marker: Color,
    added_marker: Color,
    deleted_marker: Color,
    renamed_marker: Color,
    selected_file: Color,
    selected_file_background: Color,
    separator: Color,
    /// The inner separator between side-by-side panes and the continuity markers.
    dim_separator: Color,
    hints: Color,
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

fn chrome_palette(color_count: u16) -> ChromePalette {
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
            hints: Color::Rgb(110, 110, 110),
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
            hints: Color::Indexed(240),
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
            hints: Color::DarkGray,
        },
    }
}
