//! Frame chrome around the diff panes: the file list, footer hints, and separators.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout as RatatuiLayout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use crate::{
    file_identity::FileState,
    file_tree::{FileTree, Marker, TreeRow},
};

const MINIMUM_FILE_LIST_HEIGHT: u16 = 3;

const FOOTER_HINTS: [&str; 13] = [
    "   k",
    " h · l  movement",
    "   j",
    "",
    "  ^U    page up",
    "  ^D    page down",
    "  g·G   top/bottom",
    "  v    cycle layout",
    "  c    character detail",
    "  w    wrap",
    " +/-   context",
    "(⇧)Tab  next/prev file",
    "   q    exit",
];

/// Draws the file list above the footer hints in the file-list column.
pub(super) fn render_file_column(
    frame: &mut Frame,
    area: Rect,
    tree: &FileTree,
    selected_row: usize,
    color_count: u16,
) {
    let palette = chrome_palette(color_count);
    let footer_height = area
        .height
        .saturating_sub(MINIMUM_FILE_LIST_HEIGHT)
        .min(FOOTER_HINTS.len() as u16);
    let [list_area, footer_area] = RatatuiLayout::vertical([
        Constraint::Min(MINIMUM_FILE_LIST_HEIGHT),
        Constraint::Length(footer_height),
    ])
    .areas(area);

    render_file_list(frame, list_area, tree, selected_row, palette);
    render_footer(frame.buffer_mut(), footer_area, palette);
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
    let style = Style::default().fg(inner_separator_color(color_count));

    fill(buffer, area, symbol, style);
}

/// Lists only the rows that fit: a fresh `ListState` scrolls just far enough to keep
/// the selected row on the last row, so listing that window draws the same frame.
fn render_file_list(
    frame: &mut Frame,
    area: Rect,
    tree: &FileTree,
    selected_row: usize,
    palette: ChromePalette,
) {
    let height = usize::from(area.height);
    let first = (selected_row + 1).saturating_sub(height);

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

    frame.render_stateful_widget(file_list, area, &mut state);
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
            footer: Color::DarkGray,
        },
    }
}

fn inner_separator_color(color_count: u16) -> Color {
    match color_count {
        u16::MAX => Color::Rgb(95, 95, 95),
        256.. => Color::Indexed(238),
        _ => Color::DarkGray,
    }
}
