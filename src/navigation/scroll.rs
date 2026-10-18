//! Scroll positions and their bounds: a position is in bounds only when the bounds of what
//! it scrolls built it. Rules: `docs/spec/architecture.md` (Viewport coordinates, Keyboard
//! bindings).

use crate::screen::{Cells, ScreenLines};

/// The first visible screen line and the synchronized horizontal offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollPosition {
    pub vertical: ScreenLines,
    pub horizontal: Cells,
}

/// A scroll position within the bounds of what it scrolls. Only the start of a page and
/// [`DisplayBounds::clamp`], which only navigation reaches, build one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedScroll(ScrollPosition);

impl BoundedScroll {
    /// The start of a page, which is within every bound.
    pub(super) const START: Self = Self(ScrollPosition {
        vertical: ScreenLines::ZERO,
        horizontal: Cells::ZERO,
    });

    pub fn position(self) -> ScrollPosition {
        self.0
    }

    /// This position after `movement`, within `bounds`.
    pub(super) fn moved(self, movement: Movement, bounds: DisplayBounds) -> Self {
        let scroll = self.0;
        let horizontal = |horizontal| ScrollPosition {
            horizontal,
            ..scroll
        };
        let vertical = |vertical| ScrollPosition { vertical, ..scroll };
        let one_cell = Cells::ONE;
        let one_line = ScreenLines::ONE;
        let half_page = bounds.pane_lines.half().max(one_line);

        let moved = match movement {
            Movement::Left => horizontal(scroll.horizontal.saturating_sub(one_cell)),
            Movement::Right => horizontal(scroll.horizontal.saturating_add(one_cell)),
            Movement::FarLeft => horizontal(Cells::ZERO),
            Movement::FarRight => horizontal(bounds.maximum_horizontal_offset),
            Movement::Down => vertical(scroll.vertical.saturating_add(one_line)),
            Movement::Up => vertical(scroll.vertical.saturating_sub(one_line)),
            Movement::HalfPageDown => vertical(scroll.vertical.saturating_add(half_page)),
            Movement::HalfPageUp => vertical(scroll.vertical.saturating_sub(half_page)),
            Movement::Top => vertical(ScreenLines::ZERO),
            Movement::Bottom => vertical(bounds.maximum_vertical_offset),
        };

        bounds.clamp(moved)
    }
}

/// What a movement key asks of the scroll position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Movement {
    Left,
    Right,
    FarLeft,
    FarRight,
    Down,
    Up,
    HalfPageDown,
    HalfPageUp,
    Top,
    Bottom,
}

/// The limits of a scroll position, and the lines one pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayBounds {
    maximum_vertical_offset: ScreenLines,
    maximum_horizontal_offset: Cells,
    pane_lines: ScreenLines,
}

impl DisplayBounds {
    /// Bounds that scroll until `maximum_vertical_offset` and `maximum_horizontal_offset`,
    /// over panes of `pane_lines` lines.
    pub(super) const fn new(
        maximum_vertical_offset: ScreenLines,
        maximum_horizontal_offset: Cells,
        pane_lines: ScreenLines,
    ) -> Self {
        Self {
            maximum_vertical_offset,
            maximum_horizontal_offset,
            pane_lines,
        }
    }

    /// `scroll` brought within these bounds.
    pub(super) fn clamp(self, scroll: ScrollPosition) -> BoundedScroll {
        BoundedScroll(ScrollPosition {
            vertical: scroll.vertical.min(self.maximum_vertical_offset),
            horizontal: scroll.horizontal.min(self.maximum_horizontal_offset),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundedScroll, DisplayBounds, Movement, ScrollPosition};
    use crate::screen::{Cells, ScreenLines};

    fn scroll(vertical: usize, horizontal: usize) -> ScrollPosition {
        ScrollPosition {
            vertical: ScreenLines::new(vertical),
            horizontal: Cells::new(horizontal),
        }
    }

    fn bounds(maximum_vertical_offset: usize, maximum_horizontal_offset: usize) -> DisplayBounds {
        DisplayBounds {
            maximum_vertical_offset: ScreenLines::new(maximum_vertical_offset),
            maximum_horizontal_offset: Cells::new(maximum_horizontal_offset),
            pane_lines: ScreenLines::new(8),
        }
    }

    /// `scroll` within `bounds`, moved by `movement`.
    fn moved(movement: Movement, scroll: ScrollPosition, bounds: DisplayBounds) -> ScrollPosition {
        bounds.clamp(scroll).moved(movement, bounds).position()
    }

    #[test]
    fn moves_and_clamps_the_vertical_viewport() {
        let down = moved(Movement::Down, scroll(4, 2), bounds(5, 4));
        let clamped = moved(Movement::Down, down, bounds(5, 4));

        assert_eq!(down, scroll(5, 2));
        assert_eq!(clamped, scroll(5, 2));
    }

    #[test]
    fn moves_and_clamps_the_horizontal_viewport() {
        let limits = bounds(7, 3);

        let right = moved(Movement::Right, scroll(4, 2), limits);
        let clamped = moved(Movement::Right, right, limits);
        let left = moved(Movement::Left, clamped, limits);

        assert_eq!(right, scroll(4, 3));
        assert_eq!(clamped, scroll(4, 3));
        assert_eq!(left, scroll(4, 2));
    }

    #[test]
    fn jumps_to_horizontal_boundaries() {
        let limits = bounds(7, 12);

        assert_eq!(moved(Movement::FarLeft, scroll(4, 2), limits), scroll(4, 0));
        assert_eq!(
            moved(Movement::FarRight, scroll(4, 2), limits),
            scroll(4, 12)
        );
    }

    /// architecture.md (Keyboard bindings): half-page movement moves half of a pane's lines.
    #[test]
    fn moves_half_a_pane_by_half_page() {
        let limits = DisplayBounds {
            pane_lines: ScreenLines::new(15),
            ..bounds(100, 0)
        };

        let down = moved(Movement::HalfPageDown, scroll(4, 0), limits);
        let up = moved(Movement::HalfPageUp, down, limits);

        assert_eq!(down, scroll(11, 0));
        assert_eq!(up, scroll(4, 0));
    }

    /// architecture.md (Lossless and safe rendering): no terminal offset type caps how far a
    /// row reaches.
    #[test]
    fn bounds_horizontal_movement_to_the_widest_row() {
        let wide = DisplayBounds {
            maximum_horizontal_offset: Cells::new(usize::MAX - 1),
            ..bounds(0, 0)
        };

        assert_eq!(
            BoundedScroll::START
                .moved(Movement::FarRight, wide)
                .position(),
            scroll(0, usize::MAX - 1)
        );
    }
}
