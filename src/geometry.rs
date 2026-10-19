//! The screen's areas for one screen size and file-list choice: the file-list column, its
//! separator, and the diff area, which each pane shape divides into diff panes. Rules:
//! docs/spec/architecture.md#interaction-model and
//! docs/spec/architecture.md#continuity-markers and
//! docs/spec/architecture.md#viewport-coordinates.

use ratatui::layout::{Constraint, Flex, Layout, Position, Rect, Size};

use crate::{
    preferences::{DiffLayout, FileList},
    screen::{Cells, ScreenLines},
    side::BySide,
};

/// Columns below which a screen shows only the too-narrow message.
const MINIMUM_SCREEN_WIDTH: u16 = 20;
/// Columns of the file list.
const FILE_LIST_WIDTH: u16 = 24;
/// Columns of the separator between the file list and the diff panes.
const SEPARATOR_WIDTH: u16 = 1;
/// Blank columns on each side of that separator.
const SEPARATOR_PADDING: u16 = 1;
/// Rows of the separator between stacked panes.
const STACKED_SEPARATOR_HEIGHT: u16 = 1;

/// The file list and its padded separator.
const FILE_COLUMN_WIDTH: u16 =
    FILE_LIST_WIDTH + SEPARATOR_PADDING + SEPARATOR_WIDTH + SEPARATOR_PADDING;

/// How a layout divides the diff area: one pane, or a before and an after pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneShape {
    Single,
    Split(SplitDirection),
}

impl PaneShape {
    /// The panes `layout` divides the diff area into.
    pub const fn of(layout: DiffLayout) -> Self {
        match layout {
            DiffLayout::Unified => Self::Single,
            DiffLayout::Vertical => Self::Split(SplitDirection::Vertical),
            DiffLayout::Stacked => Self::Split(SplitDirection::Stacked),
        }
    }
}

/// Which way a split layout divides the diff area between its panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// The old pane left of the new pane.
    Vertical,
    /// The old pane above the new pane.
    Stacked,
}

/// A screen's areas: the whole screen, the file-list column when it is shown, and the
/// diff area beside it.
pub struct ScreenGeometry {
    screen: Rect,
    file_column: Option<FileColumn>,
    diff: Rect,
}

impl ScreenGeometry {
    /// The areas of a screen of `size` at the origin.
    pub fn of(size: Size, file_list: FileList) -> Self {
        let screen = Rect::from((Position::ORIGIN, size));

        match shows_file_list(screen.width, file_list) {
            true => {
                let [column, diff] =
                    Layout::horizontal([Constraint::Length(FILE_COLUMN_WIDTH), Constraint::Min(0)])
                        .areas(screen);
                Self {
                    screen,
                    file_column: Some(FileColumn::of(column)),
                    diff,
                }
            }
            false => Self {
                screen,
                file_column: None,
                diff: screen,
            },
        }
    }

    pub fn screen(&self) -> Rect {
        self.screen
    }

    pub fn file_column(&self) -> Option<FileColumn> {
        self.file_column
    }

    /// Too narrow to show panes at all.
    pub fn is_narrow(&self) -> bool {
        self.screen.width < MINIMUM_SCREEN_WIDTH
    }

    /// The one diff pane of the single shape, frame included: the whole diff area.
    pub fn single_pane(&self) -> Rect {
        self.diff
    }

    /// The diff panes a split in `direction` divides the diff area into.
    pub fn split_panes(&self, direction: SplitDirection) -> SplitPanes {
        SplitPanes::of(self.diff, direction)
    }

    /// The content each pane of `shape` shows.
    pub fn pane_content(&self, shape: PaneShape) -> PaneContent {
        match shape {
            PaneShape::Single => PaneContent::inside(self.single_pane()),
            PaneShape::Split(direction) => self.split_panes(direction).content(),
        }
    }
}

/// The file list and the full-height separator beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileColumn {
    pub list: Rect,
    pub separator: Rect,
}

impl FileColumn {
    /// `column` divided into the list, then the separator padded on both sides.
    fn of(column: Rect) -> Self {
        let [list, _, separator, _] = Layout::horizontal([
            Constraint::Length(FILE_LIST_WIDTH),
            Constraint::Length(SEPARATOR_PADDING),
            Constraint::Length(SEPARATOR_WIDTH),
            Constraint::Length(SEPARATOR_PADDING),
        ])
        .areas(column);

        Self { list, separator }
    }
}

/// The old and new panes of a split in one direction, frames included, and the separator
/// between them.
pub struct SplitPanes {
    direction: SplitDirection,
    panes: BySide<Rect>,
    separator: Rect,
}

impl SplitPanes {
    /// `area` split in `direction` into two even panes around their separator.
    fn of(area: Rect, direction: SplitDirection) -> Self {
        let [old, separator, new] = match direction {
            SplitDirection::Vertical => Layout::horizontal([
                Constraint::Percentage(50),
                Constraint::Length(SEPARATOR_WIDTH),
                Constraint::Percentage(50),
            ])
            .areas(area),
            SplitDirection::Stacked => {
                let height = stacked_pane_height(area.height);
                Layout::vertical([
                    Constraint::Length(height),
                    Constraint::Length(STACKED_SEPARATOR_HEIGHT),
                    Constraint::Length(height),
                ])
                .flex(Flex::Start)
                .areas(area)
            }
        };

        Self {
            direction,
            panes: BySide { old, new },
            separator,
        }
    }

    pub fn direction(&self) -> SplitDirection {
        self.direction
    }

    pub fn panes(&self) -> BySide<Rect> {
        self.panes
    }

    pub fn separator(&self) -> Rect {
        self.separator
    }

    /// The narrowest pane's content columns and the shortest pane's content lines.
    fn content(self) -> PaneContent {
        let contents = self.panes.map(PaneContent::inside);

        PaneContent {
            width: contents.old.width.min(contents.new.width),
            height: contents.old.height.min(contents.new.height),
        }
    }
}

/// The content of each diff pane, inside its frame: the narrowest pane's columns, and the
/// lines every pane shows.
#[derive(Clone, Copy)]
pub struct PaneContent {
    pub width: Cells,
    pub height: ScreenLines,
}

impl PaneContent {
    /// The content inside `pane`'s frame.
    fn inside(pane: Rect) -> Self {
        let interior = FramedArea::diff_pane(pane).content();

        Self {
            width: Cells::from(interior.width),
            height: ScreenLines::from(interior.height),
        }
    }
}

/// The rows a frame takes on each of its top and bottom edges, and the columns it takes
/// on each of its side edges.
#[derive(Clone, Copy)]
pub struct FrameEdges {
    pub rows: u16,
    pub columns: u16,
}

impl FrameEdges {
    /// A diff pane's continuity-marker frame: a row above and below and a column on
    /// either side.
    pub const DIFF_PANE: Self = Self {
        rows: 1,
        columns: 1,
    };
    /// The file list's continuity-marker frame: a row above and below, and no columns.
    pub const FILE_LIST: Self = Self {
        rows: 1,
        columns: 0,
    };

    /// The size of a frame around `content`.
    pub const fn around(self, content: Size) -> Size {
        Size::new(
            self.columns_around(content.width),
            self.rows_around(content.height),
        )
    }

    /// The rows of a frame around `height` rows of content.
    pub const fn rows_around(self, height: u16) -> u16 {
        height + 2 * self.rows
    }

    /// The columns of a frame around `width` columns of content.
    pub const fn columns_around(self, width: u16) -> u16 {
        width + 2 * self.columns
    }

    /// The content inside a frame over `area`.
    pub fn inside(self, area: Rect) -> Rect {
        Rect::new(
            area.x + self.columns,
            area.y + self.rows,
            area.width.saturating_sub(2 * self.columns),
            area.height.saturating_sub(2 * self.rows),
        )
    }
}

/// An area with a continuity-marker frame, and the content inside the frame.
pub struct FramedArea {
    area: Rect,
    content: Rect,
}

impl FramedArea {
    /// A diff pane in `area`, inside its continuity-marker frame.
    pub fn diff_pane(area: Rect) -> Self {
        Self::of(area, FrameEdges::DIFF_PANE)
    }

    /// The file list in `area`, inside its continuity-marker frame.
    pub fn file_list(area: Rect) -> Self {
        Self::of(area, FrameEdges::FILE_LIST)
    }

    /// `area` inside a frame with `edges`.
    fn of(area: Rect, edges: FrameEdges) -> Self {
        Self {
            area,
            content: edges.inside(area),
        }
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    pub fn content(&self) -> Rect {
        self.content
    }
}

/// The cell halfway along `length` cells from `start`, rounding toward `start`.
pub const fn halfway(start: u16, length: u16) -> u16 {
    start + length / 2
}

/// The file list is shown, and its column, padded separator, and at least one diff column
/// fit.
fn shows_file_list(width: u16, file_list: FileList) -> bool {
    match file_list {
        FileList::Shown => width > FILE_COLUMN_WIDTH,
        FileList::Hidden => false,
    }
}

/// Rows of each stacked pane, frame included: the panes share the height around a one-row
/// separator.
fn stacked_pane_height(area_height: u16) -> u16 {
    area_height.saturating_sub(STACKED_SEPARATOR_HEIGHT) / 2
}

#[cfg(test)]
mod tests {
    use ratatui::layout::{Rect, Size};

    use super::{FileColumn, PaneShape, ScreenGeometry};
    use crate::{
        preferences::{DiffLayout, FileList},
        screen::{Cells, ScreenLines},
    };

    /// The areas of a `width` by `height` screen with the file list shown.
    fn geometry(width: u16, height: u16) -> ScreenGeometry {
        ScreenGeometry::of(Size::new(width, height), FileList::Shown)
    }

    /// docs/spec/architecture.md#interaction-model: one vertical separator, padded by one
    /// space on each side, follows the file list.
    #[test]
    fn pads_the_separator_after_the_file_list() {
        let screen = geometry(28, 3);

        assert_eq!(
            screen.file_column(),
            Some(FileColumn {
                list: Rect::new(0, 0, 24, 3),
                separator: Rect::new(25, 0, 1, 3),
            })
        );
        assert_eq!(screen.single_pane(), Rect::new(27, 0, 1, 3));
    }

    #[test]
    fn gives_the_diff_the_whole_width_while_the_file_list_is_hidden() {
        let screen = ScreenGeometry::of(Size::new(80, 3), FileList::Hidden);

        assert_eq!(screen.file_column(), None);
        assert_eq!(screen.single_pane(), Rect::new(0, 0, 80, 3));
        assert_eq!(screen.pane_content(PaneShape::Single).width, Cells::new(78));
    }

    #[test]
    fn derives_the_narrowest_content_pane_width_for_each_layout() {
        let width = |screen_width, layout| {
            geometry(screen_width, 30)
                .pane_content(PaneShape::of(layout))
                .width
        };

        assert_eq!(width(80, DiffLayout::Unified), Cells::new(51));
        assert_eq!(width(80, DiffLayout::Vertical), Cells::new(24));
        assert_eq!(width(80, DiffLayout::Stacked), Cells::new(51));
        assert_eq!(width(23, DiffLayout::Unified), Cells::new(21));
        assert_eq!(width(23, DiffLayout::Vertical), Cells::new(9));
    }

    #[test]
    fn leaves_each_pane_frame_out_of_its_content_height() {
        let height = |screen_height, layout| {
            geometry(80, screen_height)
                .pane_content(PaneShape::of(layout))
                .height
        };

        assert_eq!(height(36, DiffLayout::Unified), ScreenLines::new(34));
        assert_eq!(height(36, DiffLayout::Vertical), ScreenLines::new(34));
        assert_eq!(height(36, DiffLayout::Stacked), ScreenLines::new(15));
        assert_eq!(height(2, DiffLayout::Unified), ScreenLines::ZERO);
        assert_eq!(height(6, DiffLayout::Stacked), ScreenLines::ZERO);
    }
}
