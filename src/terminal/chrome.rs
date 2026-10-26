//! Frame chrome around the diff panes: the file list, the `? help` hint, separators,
//! continuity markers, and the floating box the help hint and keymap popup draw in.

use std::num::NonZeroUsize;

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect, Size},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState},
};

use super::row_writer::Clip;
use crate::{
    file_tree::{FileSelection, PARSING, TreeRow},
    geometry::{AxisSpan, FrameEdges, FramedArea, SplitDirection},
    screen::{Cells, ScreenLines},
    theme::ChromePalette,
};

/// Rows the file list keeps inside its frame before the hint takes any.
const MINIMUM_FILE_LIST_HEIGHT: u16 = 3;

/// The hint that names the key opening the keymap popup.
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
/// the page's file, and returns the window line the selected row is drawn on; a document
/// without files lists nothing.
pub(super) fn render_file_column(
    frame: &mut Frame,
    area: Rect,
    selection: Option<FileSelection<'_>>,
    palette: &ChromePalette,
) -> Option<usize> {
    let list_height = FrameEdges::FILE_LIST.rows_around(MINIMUM_FILE_LIST_HEIGHT);
    let hint_height = area
        .height
        .saturating_sub(list_height)
        .min(HELP_HINT_HEIGHT);
    let [list_area, hint_row] = Layout::vertical([
        Constraint::Min(list_height),
        Constraint::Length(hint_height),
    ])
    .areas(area);
    let [_, hint_area] =
        Layout::horizontal([Constraint::Length(HELP_HINT_INDENT), Constraint::Fill(1)])
            .areas(hint_row);

    let selected_line =
        selection.and_then(|chosen| render_file_list(frame, list_area, chosen, palette));
    draw_help_hint(frame.buffer_mut(), hint_area, palette);
    selected_line
}

/// The help hint in a box floating over the bottom-left corner of `area`, while the file
/// list is hidden.
pub(super) fn render_help_box(frame: &mut Frame, area: Rect, palette: &ChromePalette) {
    let hint_width = Cells::of_text(HELP_HINT).frame_columns();
    let help_box = FloatingBox::bottom_left_of(area, Size::new(hint_width, HELP_HINT_HEIGHT));

    let content = help_box.render(frame, palette);
    draw_help_hint(frame.buffer_mut(), content, palette);
}

/// A bordered box with a padding column inside each side, drawn over the chrome and panes
/// it floats on, and clipped to the area it floats over.
pub(super) struct FloatingBox(FramedArea);

impl FloatingBox {
    /// The border on every side, and a padding column inside each side border.
    pub(super) const EDGES: FrameEdges = FrameEdges {
        rows: FLOATING_BOX_BORDER,
        columns: FLOATING_BOX_BORDER + FLOATING_BOX_PADDING,
    };

    /// A box around `content` in the bottom-left corner of `area`.
    fn bottom_left_of(area: Rect, content: Size) -> Self {
        Self::placed(area, content, |size| {
            Position::new(area.x, area.bottom().saturating_sub(size.height))
        })
    }

    /// A box around `content` centered over `area`.
    pub(super) fn centered_in(area: Rect, content: Size) -> Self {
        Self::placed(area, content, |size| {
            Position::new(
                AxisSpan::columns_of(area)
                    .remaining_after(size.width)
                    .halfway(),
                AxisSpan::rows_of(area)
                    .remaining_after(size.height)
                    .halfway(),
            )
        })
    }

    /// A box around `content`, its corner at the position `origin` picks for the box's
    /// size, clipped to `area`.
    fn placed(area: Rect, content: Size, origin: impl FnOnce(Size) -> Position) -> Self {
        let size = Self::EDGES.around(content);
        let box_area = Rect::from((origin(size), size));

        Self(FramedArea::of(box_area.intersection(area), Self::EDGES))
    }

    /// Clears the box and draws its border, returning the content area inside the border
    /// and padding.
    pub(super) fn render(self, frame: &mut Frame, palette: &ChromePalette) -> Rect {
        let border = Block::bordered().border_style(Style::default().fg(palette.separator));

        frame.render_widget(Clear, self.0.area());
        frame.render_widget(border, self.0.area());
        self.0.content()
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

/// The continuity markers drawn on a framed area's frame. Rules:
/// docs/spec/architecture.md#continuity-markers.
pub(super) struct ContinuityFrame(FramedArea);

impl ContinuityFrame {
    /// The frame of a diff pane filling `area`.
    pub(super) fn diff_pane(area: Rect) -> Self {
        Self(FramedArea::diff_pane(area))
    }

    /// The frame of the file list filling `area`.
    fn file_list(area: Rect) -> Self {
        Self(FramedArea::file_list(area))
    }

    pub(super) fn content(&self) -> Rect {
        self.0.content()
    }

    /// `◂` and `▸` midway down each side where any visible row was clipped.
    pub(super) fn draw_side_markers(
        &self,
        buffer: &mut Buffer,
        clip: Clip,
        palette: &ChromePalette,
    ) {
        let Some(MarkedFrame {
            area,
            content,
            style,
        }) = self.marked(palette)
        else {
            return;
        };
        let y = AxisSpan::rows_of(content).halfway();

        if clip.left {
            buffer[(area.left(), y)].set_symbol("◂").set_style(style);
        }
        if clip.right {
            buffer[(area.right() - 1, y)]
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
        let Some(MarkedFrame {
            area,
            content,
            style,
        }) = self.marked(palette)
        else {
            return;
        };
        let center = AxisSpan::columns_of(content).halfway();
        let shown = ScreenLines::from(content.height);

        if window.first > ScreenLines::ZERO {
            buffer[(center, area.top())]
                .set_symbol("▴")
                .set_style(style);
        }
        if window.first.saturating_add(shown) < window.total {
            buffer[(center, area.bottom() - 1)]
                .set_symbol("▾")
                .set_style(style);
        }
    }

    /// The frame's area and content with the style its markers draw in, when it has
    /// content to mark.
    fn marked(&self, palette: &ChromePalette) -> Option<MarkedFrame> {
        let content = self.0.content();
        if content.is_empty() {
            return None;
        }

        Some(MarkedFrame {
            area: self.0.area(),
            content,
            style: Style::default().fg(palette.dim_separator),
        })
    }
}

/// A frame with content to mark: its area, its content, and its markers' style.
struct MarkedFrame {
    area: Rect,
    content: Rect,
    style: Style,
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
/// the selected row on the last row, or on its pinned line, so listing that window draws
/// the same frame. Returns the window line the selected row is drawn on.
fn render_file_list(
    frame: &mut Frame,
    area: Rect,
    FileSelection {
        tree,
        selected,
        pinned_line,
    }: FileSelection<'_>,
    palette: &ChromePalette,
) -> Option<usize> {
    let list_frame = ContinuityFrame::file_list(area);
    let content = list_frame.content();
    let height = NonZeroUsize::new(usize::from(content.height))?;

    let row_window = selected.window(height, pinned_line);
    let line_window = LineWindow {
        first: row_window.first.screen_line(),
        total: tree.line_count(),
    };

    let items = tree
        .rows_from(row_window.first)
        .iter()
        .take(height.get())
        .map(|row| tree_item(row, palette));
    let file_list = List::new(items).highlight_style(
        Style::default()
            .fg(palette.selected_file)
            .bg(palette.selected_file_background),
    );
    let mut state = ListState::default().with_selected(Some(row_window.selected_offset));

    frame.render_stateful_widget(file_list, content, &mut state);
    list_frame.draw_window_markers(frame.buffer_mut(), line_window, palette);
    Some(row_window.selected_offset)
}

/// The file-list item for `row`: a bold directory heading, or a file's colored marker and
/// its name.
fn tree_item<'a>(row: &'a TreeRow, palette: &ChromePalette) -> ListItem<'a> {
    let line = match row {
        TreeRow::Directory(heading) => Line::styled(
            &**heading,
            Style::default()
                .fg(palette.directory_heading())
                .add_modifier(Modifier::BOLD),
        ),
        TreeRow::File { marker, name } => Line::from(vec![
            Span::styled(marker.label(), Style::default().fg(palette.marker(*marker))),
            Span::raw(" "),
            Span::raw(&**name),
        ])
        .style(Style::default().fg(palette.file_name)),
        TreeRow::Parsing => Line::styled(PARSING, Style::default().fg(palette.hint)),
    };

    ListItem::new(line)
}

/// The `? help` hint at the start of `area`'s first row, cut to its width; the file
/// column and the floating help box both draw it here.
fn draw_help_hint(buffer: &mut Buffer, area: Rect, palette: &ChromePalette) {
    if area.is_empty() {
        return;
    }
    let style = Style::default().fg(palette.hint);

    buffer.set_stringn(area.x, area.y, HELP_HINT, usize::from(area.width), style);
}

/// Draws `symbol` in `style` on every cell of `area`.
fn fill(buffer: &mut Buffer, area: Rect, symbol: &'static str, style: Style) {
    for position in area.positions() {
        buffer[position].set_symbol(symbol).set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::{HELP_HINT, HELP_HINT_INDENT, render_file_column};
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
        let selection = prepared.file_selection(PageIndex::FIRST);
        let mut terminal = Terminal::new(TestBackend::new(24, 18)).expect("test terminal");
        let palette = &Theme::of_depth(ColorDepth::TrueColor).chrome;

        terminal
            .draw(|frame| {
                render_file_column(frame, frame.area(), selection, palette);
            })
            .expect("render file column");

        let buffer = terminal.backend().buffer();
        let row = |y| {
            (buffer.area.left()..buffer.area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        };
        let rows = (buffer.area.top()..buffer.area.bottom())
            .map(row)
            .collect::<Vec<_>>();
        let (last, above) = rows.split_last().expect("a drawn screen");
        let last_row = buffer.area.bottom() - 1;

        let indent = " ".repeat(usize::from(HELP_HINT_INDENT));
        assert_eq!(*last, format!("{indent}{HELP_HINT}"));
        assert!(
            above.iter().all(|row| !row.contains(HELP_HINT)),
            "one hint row"
        );
        assert_eq!(
            buffer[(HELP_HINT_INDENT, last_row)].fg,
            palette.hint,
            "hint tone"
        );
    }
}
