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
    fn bounds(maximum_vertical_offset: ScreenLines, maximum_horizontal_offset: Cells) -> ScrollBounds {
        ScrollBounds {
            maximum_vertical_offset,
            maximum_horizontal_offset,
            pane_lines: ScreenLines::new(8),
        }
    }

    /// `scroll` within `bounds`, moved by `movement`.
    fn moved(movement: Movement, scroll: ScrollPosition, bounds: ScrollBounds) -> ScrollPosition {
        bounds.clamp(scroll).moved(movement, bounds).position()
    }

    #[test]
    fn moves_and_clamps_the_vertical_viewport() {
        let limits = bounds(ScreenLines::new(5), Cells::new(4));

        let down = moved(
            Movement::Down,
            scroll(ScreenLines::new(4), Cells::new(2)),
            limits,
        );
        let clamped = moved(Movement::Down, down, limits);

        assert_eq!(down, scroll(ScreenLines::new(5), Cells::new(2)));
        assert_eq!(clamped, scroll(ScreenLines::new(5), Cells::new(2)));
    }

    #[test]
    fn moves_and_clamps_the_horizontal_viewport() {
        let limits = bounds(ScreenLines::new(7), Cells::new(3));

        let right = moved(
            Movement::Right,
            scroll(ScreenLines::new(4), Cells::new(2)),
            limits,
        );
        let clamped = moved(Movement::Right, right, limits);
        let left = moved(Movement::Left, clamped, limits);

        assert_eq!(right, scroll(ScreenLines::new(4), Cells::new(3)));
        assert_eq!(clamped, scroll(ScreenLines::new(4), Cells::new(3)));
        assert_eq!(left, scroll(ScreenLines::new(4), Cells::new(2)));
    }

    #[test]
    fn jumps_to_horizontal_boundaries() {
        let limits = bounds(ScreenLines::new(7), Cells::new(12));
        let start = scroll(ScreenLines::new(4), Cells::new(2));

        assert_eq!(
            moved(Movement::FarLeft, start, limits),
            scroll(ScreenLines::new(4), Cells::ZERO)
        );
        assert_eq!(
            moved(Movement::FarRight, start, limits),
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

        let down = moved(Movement::HalfPageDown, start, limits);
        let up = moved(Movement::HalfPageUp, down, limits);

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
