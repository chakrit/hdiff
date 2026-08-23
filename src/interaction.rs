#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewport {
    pub offset: usize,
    pub height: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Interaction {
    pub selected_file: usize,
    pub viewport: Viewport,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NavigationBounds<'a> {
    pub file_count: usize,
    pub line_count: usize,
    pub hunk_offsets: &'a [usize],
}

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Down,
    Up,
    HalfPageDown,
    HalfPageUp,
    Top,
    Bottom,
    NextFile,
    PreviousFile,
    PreviousHunk,
    NextHunk,
    Quit,
    Interrupt,
    Resize { width: u16, height: u16 },
}

pub fn transition_interaction(
    interaction: &Interaction,
    input: Input,
    bounds: &NavigationBounds<'_>,
) -> Transition {
    match input {
        Input::NextFile => return select_next_file(interaction, bounds.file_count),
        Input::PreviousFile => return select_previous_file(interaction, bounds.file_count),
        Input::PreviousHunk => return jump_to_previous_hunk(interaction, bounds),
        Input::NextHunk => return jump_to_next_hunk(interaction, bounds),
        _ => {}
    }
    let transition = transition(&interaction.viewport, input, bounds.line_count);

    match transition {
        Transition::Exit => Transition::Exit,
        Transition::Redraw(viewport) => Transition::RedrawInteraction(Interaction {
            selected_file: interaction.selected_file,
            viewport,
        }),
        Transition::RedrawInteraction(_) => unreachable!("viewport transition cannot nest"),
    }
}

fn select_next_file(interaction: &Interaction, file_count: usize) -> Transition {
    let selected_file = match file_count {
        0 => 0,
        _ => (interaction.selected_file + 1) % file_count,
    };

    redraw_selected_file(interaction, selected_file)
}

fn select_previous_file(interaction: &Interaction, file_count: usize) -> Transition {
    let selected_file = match file_count {
        0 => 0,
        _ => interaction
            .selected_file
            .checked_sub(1)
            .unwrap_or(file_count - 1),
    };

    redraw_selected_file(interaction, selected_file)
}

fn redraw_selected_file(interaction: &Interaction, selected_file: usize) -> Transition {
    Transition::RedrawInteraction(Interaction {
        selected_file,
        viewport: Viewport {
            offset: 0,
            height: interaction.viewport.height,
        },
    })
}

fn jump_to_previous_hunk(interaction: &Interaction, bounds: &NavigationBounds<'_>) -> Transition {
    let offset = bounds
        .hunk_offsets
        .iter()
        .rev()
        .find(|offset| **offset < interaction.viewport.offset)
        .copied()
        .or_else(|| bounds.hunk_offsets.first().copied())
        .unwrap_or(0);

    redraw_interaction(interaction, offset, bounds.line_count)
}

fn jump_to_next_hunk(interaction: &Interaction, bounds: &NavigationBounds<'_>) -> Transition {
    let offset = bounds
        .hunk_offsets
        .iter()
        .find(|offset| **offset > interaction.viewport.offset)
        .copied()
        .unwrap_or(interaction.viewport.offset);

    redraw_interaction(interaction, offset, bounds.line_count)
}

fn redraw_interaction(interaction: &Interaction, offset: usize, line_count: usize) -> Transition {
    let maximum_offset = line_count.saturating_sub(interaction.viewport.height);

    Transition::RedrawInteraction(Interaction {
        selected_file: interaction.selected_file,
        viewport: Viewport {
            offset: offset.min(maximum_offset),
            height: interaction.viewport.height,
        },
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum Transition {
    Redraw(Viewport),
    RedrawInteraction(Interaction),
    Exit,
}

pub fn transition(viewport: &Viewport, input: Input, line_count: usize) -> Transition {
    match input {
        Input::Quit | Input::Interrupt => Transition::Exit,
        Input::NextFile | Input::PreviousFile | Input::PreviousHunk | Input::NextHunk => {
            Transition::Redraw(viewport.clone())
        }
        Input::Down => redraw(
            viewport.offset.saturating_add(1),
            viewport.height,
            line_count,
        ),
        Input::Up => redraw(
            viewport.offset.saturating_sub(1),
            viewport.height,
            line_count,
        ),
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
    use super::{
        Input, Interaction, NavigationBounds, Transition, Viewport, transition,
        transition_interaction,
    };

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

    #[test]
    fn rotates_files_and_resets_the_viewport() {
        let interaction = Interaction {
            selected_file: 1,
            viewport: Viewport {
                offset: 4,
                height: 3,
            },
        };
        let bounds = NavigationBounds {
            file_count: 2,
            line_count: 10,
            hunk_offsets: &[2, 6],
        };

        let next = transition_interaction(&interaction, Input::NextFile, &bounds);

        assert_eq!(
            next,
            Transition::RedrawInteraction(Interaction {
                selected_file: 0,
                viewport: Viewport {
                    offset: 0,
                    height: 3,
                },
            })
        );
    }

    #[test]
    fn rotates_to_the_previous_file_and_resets_the_viewport() {
        let interaction = Interaction {
            selected_file: 0,
            viewport: Viewport {
                offset: 4,
                height: 3,
            },
        };
        let bounds = NavigationBounds {
            file_count: 2,
            line_count: 10,
            hunk_offsets: &[2, 6],
        };

        let next = transition_interaction(&interaction, Input::PreviousFile, &bounds);

        assert_eq!(
            next,
            Transition::RedrawInteraction(Interaction {
                selected_file: 1,
                viewport: Viewport {
                    offset: 0,
                    height: 3,
                },
            })
        );
    }

    #[test]
    fn jumps_between_hunks_without_leaving_the_selected_file() {
        let interaction = Interaction {
            selected_file: 1,
            viewport: Viewport {
                offset: 3,
                height: 2,
            },
        };
        let bounds = NavigationBounds {
            file_count: 2,
            line_count: 10,
            hunk_offsets: &[2, 6],
        };

        let next = transition_interaction(&interaction, Input::NextHunk, &bounds);

        assert_eq!(
            next,
            Transition::RedrawInteraction(Interaction {
                selected_file: 1,
                viewport: Viewport {
                    offset: 6,
                    height: 2,
                },
            })
        );
    }

    #[test]
    fn keeps_the_first_hunk_visible_when_moving_to_the_previous_hunk() {
        let interaction = Interaction {
            selected_file: 0,
            viewport: Viewport {
                offset: 2,
                height: 2,
            },
        };
        let bounds = NavigationBounds {
            file_count: 1,
            line_count: 10,
            hunk_offsets: &[2, 6],
        };

        let previous = transition_interaction(&interaction, Input::PreviousHunk, &bounds);

        assert_eq!(
            previous,
            Transition::RedrawInteraction(Interaction {
                selected_file: 0,
                viewport: Viewport {
                    offset: 2,
                    height: 2,
                },
            })
        );
    }
}
