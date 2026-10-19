//! Scroll positions and their bounds: a position is in bounds only when the bounds of
//! what it scrolls built it. Rules: docs/spec/architecture.md#viewport-coordinates and
//! docs/spec/architecture.md#keyboard-bindings.

use crate::screen::{Cells, ScreenLines};

/// The first visible screen line and the synchronized horizontal offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollPosition {
    pub vertical: ScreenLines,
    pub horizontal: Cells,
}

/// A scroll position within the bounds of what it scrolls. Only the start of a page and
/// [`ScrollBounds::clamp`], which only navigation reaches, build one.
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
    pub(super) fn moved(self, movement: Movement, bounds: ScrollBounds) -> Self {
        let scroll = self.0;
        let at_column = |column| ScrollPosition {
            horizontal: column,
            ..scroll
        };
        let at_line = |line| ScrollPosition {
            vertical: line,
            ..scroll
        };
        let half_page = bounds.pane_lines.half().max(ScreenLines::ONE);

        let asked = match movement {
            Movement::Left => at_column(scroll.horizontal.saturating_sub(Cells::ONE)),
            Movement::Right => at_column(scroll.horizontal.saturating_add(Cells::ONE)),
            Movement::FarLeft => at_column(Cells::ZERO),
            Movement::FarRight => at_column(bounds.maximum_horizontal_offset),
            Movement::Down => at_line(scroll.vertical.saturating_add(ScreenLines::ONE)),
            Movement::Up => at_line(scroll.vertical.saturating_sub(ScreenLines::ONE)),
            Movement::HalfPageDown => at_line(scroll.vertical.saturating_add(half_page)),
            Movement::HalfPageUp => at_line(scroll.vertical.saturating_sub(half_page)),
            Movement::Top => at_line(ScreenLines::ZERO),
            Movement::Bottom => at_line(bounds.maximum_vertical_offset),
        };

        bounds.clamp(asked)
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
#[derive(Clone, Copy)]
pub(super) struct ScrollBounds {
    /// The furthest line the top of the pane may scroll to.
    pub(super) maximum_vertical_offset: ScreenLines,
    /// The furthest column the left of the pane may scroll to.
    pub(super) maximum_horizontal_offset: Cells,
    /// The lines one pane shows, which half-page movement halves.
    pub(super) pane_lines: ScreenLines,
}

impl ScrollBounds {
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
    use super::{BoundedScroll, Movement, ScrollBounds, ScrollPosition};
    use crate::screen::{Cells, ScreenLines};

    fn scroll(vertical: ScreenLines, horizontal: Cells) -> ScrollPosition {
        ScrollPosition {
            vertical,
            horizontal,
        }
    }

    /// Bounds with these maximum offsets, for a pane eight lines tall.
    fn bounds(
        maximum_vertical_offset: ScreenLines,
        maximum_horizontal_offset: Cells,
    ) -> ScrollBounds {
        ScrollBounds {
            maximum_vertical_offset,
            maximum_horizontal_offset,
            pane_lines: ScreenLines::new(8),
        }
    }

    /// `start` within `bounds`, moved by `movement`.
    fn after_moving(
        movement: Movement,
        start: ScrollPosition,
        bounds: ScrollBounds,
    ) -> ScrollPosition {
        bounds.clamp(start).moved(movement, bounds).position()
    }

    #[test]
    fn moves_and_clamps_the_vertical_viewport() {
        let limits = bounds(ScreenLines::new(5), Cells::new(4));

        let down = after_moving(
            Movement::Down,
            scroll(ScreenLines::new(4), Cells::new(2)),
            limits,
        );
        let clamped = after_moving(Movement::Down, down, limits);

        assert_eq!(down, scroll(ScreenLines::new(5), Cells::new(2)));
        assert_eq!(clamped, scroll(ScreenLines::new(5), Cells::new(2)));
    }

    #[test]
    fn moves_and_clamps_the_horizontal_viewport() {
        let limits = bounds(ScreenLines::new(7), Cells::new(3));

        let right = after_moving(
            Movement::Right,
            scroll(ScreenLines::new(4), Cells::new(2)),
            limits,
        );
        let clamped = after_moving(Movement::Right, right, limits);
        let left = after_moving(Movement::Left, clamped, limits);

        assert_eq!(right, scroll(ScreenLines::new(4), Cells::new(3)));
        assert_eq!(clamped, scroll(ScreenLines::new(4), Cells::new(3)));
        assert_eq!(left, scroll(ScreenLines::new(4), Cells::new(2)));
    }

    #[test]
    fn jumps_to_horizontal_boundaries() {
        let limits = bounds(ScreenLines::new(7), Cells::new(12));
        let start = scroll(ScreenLines::new(4), Cells::new(2));

        assert_eq!(
            after_moving(Movement::FarLeft, start, limits),
            scroll(ScreenLines::new(4), Cells::ZERO)
        );
        assert_eq!(
            after_moving(Movement::FarRight, start, limits),
            scroll(ScreenLines::new(4), Cells::new(12))
        );
    }

    /// docs/spec/architecture.md#keyboard-bindings: half-page movement moves half of a
    /// pane's lines.
    #[test]
    fn moves_half_a_pane_by_half_page() {
        let limits = ScrollBounds {
            pane_lines: ScreenLines::new(15),
            ..bounds(ScreenLines::new(100), Cells::ZERO)
        };
        let start = scroll(ScreenLines::new(4), Cells::ZERO);

        let down = after_moving(Movement::HalfPageDown, start, limits);
        let up = after_moving(Movement::HalfPageUp, down, limits);

        assert_eq!(down, scroll(ScreenLines::new(11), Cells::ZERO));
        assert_eq!(up, start);
    }

    /// docs/spec/architecture.md#viewport-coordinates: no terminal offset type
    /// caps how far a row reaches.
    #[test]
    fn bounds_horizontal_movement_to_the_widest_row() {
        let widest = Cells::new(usize::MAX - 1);
        let wide = ScrollBounds {
            maximum_horizontal_offset: widest,
            ..bounds(ScreenLines::ZERO, Cells::ZERO)
        };

        assert_eq!(
            BoundedScroll::START
                .moved(Movement::FarRight, wide)
                .position(),
            scroll(ScreenLines::ZERO, widest)
        );
    }
}
