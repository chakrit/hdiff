//! The screen's panes for one screen size and set of view preferences: the file-list
//! column, its separator, and the diff panes. Navigation bounds and drawing both read this
//! one geometry. Rules: `docs/spec/architecture.md` (Interaction model, Continuity markers).

use ratatui::layout::{Constraint, Flex, Layout as RatatuiLayout, Rect, Size};

use crate::preferences::{DiffLayout, FileList, ViewPreferences};

/// Rows each pane's continuity-marker frame takes: one above and one below.
pub const PANE_FRAME_HEIGHT: u16 = 2;
/// Columns each diff pane's continuity-marker frame takes: one on either side.
pub const PANE_FRAME_WIDTH: u16 = 2;

const MINIMUM_SCREEN_WIDTH: u16 = 20;
const FILE_LIST_WIDTH: u16 = 24;
const SEPARATOR_WIDTH: u16 = 1;
const SEPARATOR_PADDING: u16 = 1;
const STACKED_SEPARATOR_HEIGHT: u16 = 1;

/// The file list and its padded separator.
const FILE_COLUMN_WIDTH: u16 =
    FILE_LIST_WIDTH + SEPARATOR_PADDING + SEPARATOR_WIDTH + SEPARATOR_PADDING;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneGeometry {
    screen: Rect,
    layout: DiffLayout,
    file_column: Option<FileColumn>,
    diff: Rect,
}

/// The file list and the full-height separator beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileColumn {
    pub list: Rect,
    pub separator: Rect,
}

/// The before and after panes of a side-by-side layout, frames included, and the separator
/// between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitPanes {
    pub before: Rect,
    pub separator: Rect,
    pub after: Rect,
}

impl SplitPanes {
    /// Side by side across `area`, each half its width.
    pub fn vertical(area: Rect) -> Self {
        let [before, separator, after] = RatatuiLayout::horizontal([
            Constraint::Percentage(50),
            Constraint::Length(SEPARATOR_WIDTH),
            Constraint::Percentage(50),
        ])
        .areas(area);

        Self {
            before,
            separator,
            after,
        }
    }

    /// One above the other, from the top of `area`.
    pub fn stacked(area: Rect) -> Self {
        let height = pane_height(area.height, DiffLayout::Stacked);
        let [before, separator, after] = RatatuiLayout::vertical([
            Constraint::Length(height),
            Constraint::Length(STACKED_SEPARATOR_HEIGHT),
            Constraint::Length(height),
        ])
        .flex(Flex::Start)
        .areas(area);

        Self {
            before,
            separator,
            after,
        }
    }

    fn content_width(self) -> u16 {
        pane_content(self.before)
            .width
            .min(pane_content(self.after).width)
    }
}

/// Why a screen shows only a message instead of its panes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TooSmall {
    Narrow,
    /// A diff pane would have no content line.
    Short,
}

impl TooSmall {
    pub const fn message(self) -> &'static str {
        match self {
            Self::Narrow => "screen too narrow",
            Self::Short => "screen too short",
        }
    }
}

impl PaneGeometry {
    pub fn of(screen: Rect, preferences: ViewPreferences) -> Self {
        let file_column = match shows_file_list(screen.width, preferences.file_list) {
            true => Some(FILE_COLUMN_WIDTH),
            false => None,
        };
        let [column, diff] = RatatuiLayout::horizontal([
            Constraint::Length(file_column.unwrap_or(0)),
            Constraint::Min(0),
        ])
        .areas(screen);

        Self {
            screen,
            layout: preferences.layout,
            file_column: file_column.map(|_| FileColumn::of(column)),
            diff,
        }
    }

    /// The geometry of a screen of `size` at the origin.
    pub fn of_size(size: Size, preferences: ViewPreferences) -> Self {
        Self::of(
            Rect::from((ratatui::layout::Position::ORIGIN, size)),
            preferences,
        )
    }

    pub fn too_small(&self) -> Option<TooSmall> {
        if self.screen.width < MINIMUM_SCREEN_WIDTH {
            return Some(TooSmall::Narrow);
        }
        match self.content_height() {
            0 => Some(TooSmall::Short),
            _ => None,
        }
    }

    pub fn file_column(&self) -> Option<FileColumn> {
        self.file_column
    }

    /// The area of the diff panes, beside the file-list column.
    pub fn diff(&self) -> Rect {
        self.diff
    }

    /// Columns of the narrowest diff pane's content; none on a too-narrow screen.
    pub fn content_width(&self) -> u16 {
        if self.screen.width < MINIMUM_SCREEN_WIDTH {
            return 0;
        }

        match self.layout {
            DiffLayout::Unified => pane_content(self.diff).width,
            DiffLayout::Vertical => SplitPanes::vertical(self.diff).content_width(),
            DiffLayout::Stacked => SplitPanes::stacked(self.diff).content_width(),
        }
    }

    /// Rows of each diff pane's content.
    pub fn content_height(&self) -> u16 {
        pane_height(self.screen.height, self.layout).saturating_sub(PANE_FRAME_HEIGHT)
    }
}

impl FileColumn {
    fn of(column: Rect) -> Self {
        let [list, _, separator, _] = RatatuiLayout::horizontal([
            Constraint::Length(FILE_LIST_WIDTH),
            Constraint::Length(SEPARATOR_PADDING),
            Constraint::Length(SEPARATOR_WIDTH),
            Constraint::Length(SEPARATOR_PADDING),
        ])
        .areas(column);

        Self { list, separator }
    }
}

/// The content area inside a diff pane's continuity-marker frame.
pub fn pane_content(pane: Rect) -> Rect {
    let rows = between_frame_rows(pane);

    Rect::new(
        rows.x + PANE_FRAME_WIDTH / 2,
        rows.y,
        rows.width.saturating_sub(PANE_FRAME_WIDTH),
        rows.height,
    )
}

/// `area` without its frame rows above and below.
pub fn between_frame_rows(area: Rect) -> Rect {
    Rect::new(
        area.x,
        area.y + PANE_FRAME_HEIGHT / 2,
        area.width,
        area.height.saturating_sub(PANE_FRAME_HEIGHT),
    )
}

/// The file list is shown, and its column, padded separator, and at least one diff column
/// fit.
fn shows_file_list(width: u16, file_list: FileList) -> bool {
    let fits = width > FILE_COLUMN_WIDTH;

    match file_list {
        FileList::Shown => fits,
        FileList::Hidden => false,
    }
}

/// Rows of each diff pane, frame included; stacked panes share the height around a
/// one-row separator.
fn pane_height(screen_height: u16, layout: DiffLayout) -> u16 {
    match layout {
        DiffLayout::Unified | DiffLayout::Vertical => screen_height,
        DiffLayout::Stacked => screen_height.saturating_sub(STACKED_SEPARATOR_HEIGHT) / 2,
    }
}

/// The largest horizontal offset that keeps content `content_width` cells wide filling a
/// pane `pane_width` cells wide.
pub fn maximum_horizontal_offset(content_width: usize, pane_width: u16) -> u16 {
    match pane_width {
        0 => 0,
        _ => content_width
            .saturating_sub(usize::from(pane_width))
            .try_into()
            .unwrap_or(u16::MAX),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::{Rect, Size};

    use super::{FileColumn, PaneGeometry, TooSmall, maximum_horizontal_offset};
    use crate::preferences::{DiffLayout, FileList, ViewPreferences};

    fn geometry(width: u16, height: u16, preferences: ViewPreferences) -> PaneGeometry {
        PaneGeometry::of_size(Size::new(width, height), preferences)
    }

    /// What a screen shows: a message, or its panes with or without the file list.
    #[derive(Debug, PartialEq, Eq)]
    enum Shown {
        Message(TooSmall),
        DiffOnly,
        Split,
    }

    #[test]
    fn derives_compact_pane_visibility_from_terminal_size() {
        let cases = [
            (
                (19, 3),
                DiffLayout::Unified,
                Shown::Message(TooSmall::Narrow),
            ),
            (
                (20, 2),
                DiffLayout::Unified,
                Shown::Message(TooSmall::Short),
            ),
            ((27, 3), DiffLayout::Unified, Shown::DiffOnly),
            ((28, 3), DiffLayout::Unified, Shown::Split),
            ((28, 3), DiffLayout::Vertical, Shown::Split),
            (
                (28, 6),
                DiffLayout::Stacked,
                Shown::Message(TooSmall::Short),
            ),
            ((28, 7), DiffLayout::Stacked, Shown::Split),
        ];

        // architecture.md: the minimum layout shows one content line in each diff pane.
        for ((width, height), layout, expected) in cases {
            let panes = geometry(width, height, ViewPreferences::initial(layout));
            let shown = match (panes.too_small(), panes.file_column()) {
                (Some(too_small), _) => Shown::Message(too_small),
                (None, None) => Shown::DiffOnly,
                (None, Some(_)) => Shown::Split,
            };

            assert_eq!(shown, expected, "{width}×{height} {layout:?}");
        }
    }

    /// architecture.md (Interaction model): one vertical separator, padded by one space on
    /// each side, follows the file list.
    #[test]
    fn pads_the_separator_after_the_file_list() {
        let panes = geometry(28, 3, ViewPreferences::initial(DiffLayout::Unified));

        assert_eq!(
            panes.file_column(),
            Some(FileColumn {
                list: Rect::new(0, 0, 24, 3),
                separator: Rect::new(25, 0, 1, 3),
            })
        );
        assert_eq!(panes.diff(), Rect::new(27, 0, 1, 3));
    }

    #[test]
    fn gives_the_diff_the_whole_width_while_the_file_list_is_hidden() {
        let hidden = ViewPreferences {
            file_list: FileList::Hidden,
            ..ViewPreferences::initial(DiffLayout::Unified)
        };
        let panes = geometry(80, 3, hidden);

        assert_eq!(panes.file_column(), None);
        assert_eq!(panes.diff(), Rect::new(0, 0, 80, 3));
        assert_eq!(panes.content_width(), 78);
    }

    #[test]
    fn derives_the_narrowest_content_pane_width_for_each_layout() {
        let width = |screen_width, layout| {
            geometry(screen_width, 30, ViewPreferences::initial(layout)).content_width()
        };

        assert_eq!(width(80, DiffLayout::Unified), 51);
        assert_eq!(width(80, DiffLayout::Vertical), 24);
        assert_eq!(width(80, DiffLayout::Stacked), 51);
        assert_eq!(width(23, DiffLayout::Unified), 21);
        assert_eq!(width(23, DiffLayout::Vertical), 9);
    }

    #[test]
    fn leaves_each_pane_frame_out_of_its_content_height() {
        let height = |screen_height, layout| {
            geometry(80, screen_height, ViewPreferences::initial(layout)).content_height()
        };

        assert_eq!(height(36, DiffLayout::Unified), 34);
        assert_eq!(height(36, DiffLayout::Vertical), 34);
        assert_eq!(height(36, DiffLayout::Stacked), 15);
        assert_eq!(height(2, DiffLayout::Unified), 0);
        assert_eq!(height(6, DiffLayout::Stacked), 0);
    }

    #[test]
    fn bounds_horizontal_movement_to_visible_content() {
        assert_eq!(maximum_horizontal_offset(40, 20), 20);
        assert_eq!(maximum_horizontal_offset(10, 20), 0);
        assert_eq!(maximum_horizontal_offset(40, 0), 0);
        assert_eq!(maximum_horizontal_offset(usize::MAX, 1), u16::MAX);
    }
}
