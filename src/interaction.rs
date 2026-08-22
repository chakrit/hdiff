#[derive(Debug, PartialEq, Eq)]
pub struct Viewport {
    pub offset: usize,
    pub height: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Down,
    Up,
    HalfPageDown,
    HalfPageUp,
    Top,
    Bottom,
    Quit,
    Interrupt,
    Resize { width: u16, height: u16 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Transition {
    Redraw(Viewport),
    Exit,
}

pub fn transition(viewport: &Viewport, input: Input, line_count: usize) -> Transition {
    match input {
        Input::Quit | Input::Interrupt => Transition::Exit,
        Input::Down => redraw(viewport.offset.saturating_add(1), viewport.height, line_count),
        Input::Up => redraw(viewport.offset.saturating_sub(1), viewport.height, line_count),
        Input::HalfPageDown => {
            let distance = viewport.height.saturating_div(2).max(1);

            redraw(
                viewport.offset.saturating_add(distance),
                viewport.height,
                line_count,
            )
        }
        Input::HalfPageUp => {
            let distance = viewport.height.saturating_div(2).max(1);

            redraw(
                viewport.offset.saturating_sub(distance),
                viewport.height,
                line_count,
            )
        }
        Input::Top => redraw(0, viewport.height, line_count),
        Input::Bottom => redraw(line_count, viewport.height, line_count),
        Input::Resize { height, .. } => redraw(viewport.offset, usize::from(height), line_count),
    }
}

fn redraw(offset: usize, height: usize, line_count: usize) -> Transition {
    let maximum_offset = line_count.saturating_sub(height);
    let bounded_offset = offset.min(maximum_offset);

    Transition::Redraw(Viewport {
        offset: bounded_offset,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::{Input, Transition, Viewport, transition};

    #[test]
    fn moves_and_clamps_the_vertical_viewport() {
        let viewport = Viewport {
            offset: 4,
            height: 3,
        };

        let next = transition(&viewport, Input::Down, 10);

        assert_eq!(
            next,
            Transition::Redraw(Viewport {
                offset: 5,
                height: 3,
            })
        );
    }

    #[test]
    fn exits_for_quit_and_interrupt() {
        let viewport = Viewport {
            offset: 0,
            height: 1,
        };

        assert_eq!(transition(&viewport, Input::Quit, 1), Transition::Exit);
        assert_eq!(transition(&viewport, Input::Interrupt, 1), Transition::Exit);
    }

    #[test]
    fn resize_clamps_the_offset_for_the_new_height() {
        let viewport = Viewport {
            offset: 2,
            height: 3,
        };

        let next = transition(
            &viewport,
            Input::Resize {
                width: 80,
                height: 9,
            },
            10,
        );

        assert_eq!(
            next,
            Transition::Redraw(Viewport {
                offset: 1,
                height: 9,
            })
        );
    }
}
