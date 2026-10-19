//! The screen's areas for one screen size, file-list preference and pane shape: the
//! file-list column and its separator while the list fits, the diff area beside it, the
//! content each diff pane of the shape shows, and whether the screen is too small to show
//! panes at all. Rules:
//! docs/spec/architecture.md#interaction-model and
//! docs/spec/architecture.md#continuity-markers and
//! docs/spec/architecture.md#viewport-coordinates.

use ratatui::layout::{Constraint, Flex, Layout, Position, Rect, Size};

use crate::{
    preferences::{DiffLayout, FileList},
    screen::{Cells, MARKER_GUTTER_WIDTH, NonZeroCells, ScreenLines},
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

/// A screen's areas for one pane shape: the whole screen, the file-list column when it is
/// shown, the diff area beside it, the content each pane of the shape shows there, and
/// whether the screen is too small to show those panes.
pub struct ScreenGeometry {
    screen: Rect,
    shape: PaneShape,
    file_column: Option<FileColumn>,
    diff: Rect,
    pane_content: PaneContent,
    too_small: Option<TooSmall>,
}

impl ScreenGeometry {
    /// The areas of a screen of `size` at the origin, with its diff area divided into the
    /// panes of `shape`. A shown file list collapses before the diff: it takes its column
    /// only while every pane beside it keeps a payload column.
    pub fn of(size: Size, file_list: FileList, shape: PaneShape) -> Self {
        let screen = Rect::from((Position::ORIGIN, size));
        let diff_only = Self::of_areas(screen, shape, None, screen);

        match file_list {
            FileList::Hidden => diff_only,
            FileList::Shown => {
                let beside_file_list = Self::beside_file_list(screen, shape);
                match beside_file_list.pane_content.payload_width() {
                    Some(_) => beside_file_list,
                    None => diff_only,
                }
            }
        }
    }

    /// `screen` divided into the file-list column and the diff area after it, which
    /// `shape` divides into panes.
    fn beside_file_list(screen: Rect, shape: PaneShape) -> Self {
        let [column, diff] =
            Layout::horizontal([Constraint::Length(FILE_COLUMN_WIDTH), Constraint::Min(0)])
                .areas(screen);

        Self::of_areas(screen, shape, Some(FileColumn::of(column)), diff)
    }

    /// `screen` with `file_column` beside `diff`, which `shape` divides into panes.
    fn of_areas(
        screen: Rect,
        shape: PaneShape,
        file_column: Option<FileColumn>,
        diff: Rect,
    ) -> Self {
        let pane_content = match shape {
            PaneShape::Single => PaneContent::inside(diff),
            PaneShape::Split(direction) => SplitPanes::of(diff, direction).content(),
        };
        let too_small = TooSmall::of(screen, pane_content);

        Self {
            screen,
            shape,
            file_column,
            diff,
            pane_content,
            too_small,
        }
    }

    pub fn screen(&self) -> Rect {
        self.screen
    }

    pub fn shape(&self) -> PaneShape {
        self.shape
    }

    pub fn file_column(&self) -> Option<FileColumn> {
        self.file_column
    }

    /// The diff area beside the file list, or the whole screen while the list is hidden:
    /// the one pane of the single shape, frame included, and the area a split divides.
    pub fn diff(&self) -> Rect {
        self.diff
    }

    /// The content each pane of the shape shows.
    pub fn pane_content(&self) -> PaneContent {
        self.pane_content
    }

    /// Why the screen shows only a message instead of its panes, if it does.
    pub fn too_small(&self) -> Option<TooSmall> {
        self.too_small
    }
}

/// Why a screen shows only a message instead of its panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TooSmall {
    /// The screen is narrower than the minimum layout.
    Narrow,
    /// A diff pane would have no content line.
    Short,
}

impl TooSmall {
    /// Why `screen`, whose panes show `pane_content`, is too small to show them, if it
    /// is: narrower than the minimum layout, or too short for each pane to show one
    /// content line.
    fn of(screen: Rect, pane_content: PaneContent) -> Option<Self> {
        let narrow = screen.width < MINIMUM_SCREEN_WIDTH;

        match (narrow, pane_content.height) {
            (true, _) => Some(Self::Narrow),
            (false, ScreenLines::ZERO) => Some(Self::Short),
            (false, _) => None,
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
    pub fn of(area: Rect, direction: SplitDirection) -> Self {
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

    /// The columns beside the marker gutter left for payloads, or none when the gutter
    /// takes every column.
    pub fn payload_width(self) -> Option<NonZeroCells> {
        NonZeroCells::new(self.width.saturating_sub(MARKER_GUTTER_WIDTH))
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

/// An area with a frame of edges around its sides, and the content inside the frame.
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
    pub fn of(area: Rect, edges: FrameEdges) -> Self {
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

/// Rows of each stacked pane, frame included: the panes share the height around a one-row
/// separator.
fn stacked_pane_height(area_height: u16) -> u16 {
    area_height.saturating_sub(STACKED_SEPARATOR_HEIGHT) / 2
}

#[cfg(test)]
mod tests {
    use ratatui::layout::{Rect, Size};

    use super::{FileColumn, PaneShape, ScreenGeometry, TooSmall};
    use crate::{
        preferences::{DiffLayout, FileList},
        screen::{Cells, ScreenLines},
    };

    /// What a screen shows: a message, or its panes with or without the file list.
    #[derive(Debug, PartialEq, Eq)]
    enum Shown {
        Message(TooSmall),
        DiffOnly,
        WithFileList,
    }

    /// The areas of a `width` by `height` screen in `layout` with the file list preferred
    /// shown; the list still collapses where its panes would lose their payload column.
    fn geometry(width: u16, height: u16, layout: DiffLayout) -> ScreenGeometry {
        ScreenGeometry::of(
            Size::new(width, height),
            FileList::Shown,
            PaneShape::of(layout),
        )
    }

    /// docs/spec/architecture.md#interaction-model: one vertical separator, padded by one
    /// space on each side, follows the file list.
    #[test]
    fn pads_the_separator_after_the_file_list() {
        let screen = geometry(40, 3, DiffLayout::Unified);

        assert_eq!(
            screen.file_column(),
            Some(FileColumn {
                list: Rect::new(0, 0, 24, 3),
                separator: Rect::new(25, 0, 1, 3),
            })
        );
        assert_eq!(screen.diff(), Rect::new(27, 0, 13, 3));
    }

    #[test]
    fn gives_the_diff_the_whole_width_while_the_file_list_is_hidden() {
        let screen = ScreenGeometry::of(Size::new(80, 3), FileList::Hidden, PaneShape::Single);

        assert_eq!(screen.file_column(), None);
        assert_eq!(screen.diff(), Rect::new(0, 0, 80, 3));
        assert_eq!(screen.pane_content().width, Cells::new(78));
    }

    #[test]
    fn derives_the_narrowest_content_pane_width_for_each_layout() {
        let content_width =
            |screen_width, layout| geometry(screen_width, 30, layout).pane_content().width;

        assert_eq!(content_width(80, DiffLayout::Unified), Cells::new(51));
        assert_eq!(content_width(80, DiffLayout::Vertical), Cells::new(24));
        assert_eq!(content_width(80, DiffLayout::Stacked), Cells::new(51));
        assert_eq!(content_width(23, DiffLayout::Unified), Cells::new(21));
        assert_eq!(content_width(23, DiffLayout::Vertical), Cells::new(9));
    }

    /// docs/spec/architecture.md#interaction-model: the minimum layout shows one content
    /// line in each diff pane, and the file list collapses before the diff view.
    #[test]
    fn derives_compact_pane_visibility_from_terminal_size() {
        let cases = [
            (19, 3, DiffLayout::Unified, Shown::Message(TooSmall::Narrow)),
            (20, 2, DiffLayout::Unified, Shown::Message(TooSmall::Short)),
            (27, 3, DiffLayout::Unified, Shown::DiffOnly),
            (28, 3, DiffLayout::Unified, Shown::DiffOnly),
            (28, 3, DiffLayout::Vertical, Shown::DiffOnly),
            (28, 6, DiffLayout::Stacked, Shown::Message(TooSmall::Short)),
            (28, 7, DiffLayout::Stacked, Shown::DiffOnly),
            (80, 7, DiffLayout::Stacked, Shown::WithFileList),
        ];

        for (width, height, layout, expected) in cases {
            let screen = geometry(width, height, layout);
            let shown = match (screen.too_small(), screen.file_column()) {
                (Some(too_small), _) => Shown::Message(too_small),
                (None, None) => Shown::DiffOnly,
                (None, Some(_)) => Shown::WithFileList,
            };

            assert_eq!(shown, expected, "{width}×{height} {layout:?}");
        }
    }

    /// docs/spec/architecture.md#interaction-model: as width decreases the file list
    /// collapses before the diff view, so no pane it leaves lacks a payload column, and a
    /// screen too narrow for both gives the panes the whole width.
    #[test]
    fn collapses_the_file_list_before_any_pane_loses_its_payload_column() {
        let layouts = [
            DiffLayout::Unified,
            DiffLayout::Vertical,
            DiffLayout::Stacked,
        ];

        for layout in layouts {
            for width in 20..=80 {
                let screen = geometry(width, 30, layout);
                if screen.file_column().is_some() {
                    assert!(
                        screen.pane_content().payload_width().is_some(),
                        "{layout:?} at {width} columns shows the file list over a pane \
                         without a payload column"
                    );
                }
            }
            assert!(
                geometry(23, 30, layout).file_column().is_none(),
                "{layout:?} at 23 columns keeps the file list beside the diff"
            );
            assert!(
                geometry(120, 30, layout).file_column().is_some(),
                "{layout:?} shows the file list where it fits"
            );
        }
    }

    #[test]
    fn leaves_each_pane_frame_out_of_its_content_height() {
        let content_height =
            |screen_height, layout| geometry(80, screen_height, layout).pane_content().height;

        assert_eq!(
            content_height(36, DiffLayout::Unified),
            ScreenLines::new(34)
        );
        assert_eq!(
            content_height(36, DiffLayout::Vertical),
            ScreenLines::new(34)
        );
        assert_eq!(
            content_height(36, DiffLayout::Stacked),
            ScreenLines::new(15)
        );
        assert_eq!(content_height(2, DiffLayout::Unified), ScreenLines::ZERO);
        assert_eq!(content_height(6, DiffLayout::Stacked), ScreenLines::ZERO);
    }
}
