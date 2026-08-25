#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewport {
    pub offset: usize,
    pub height: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Interaction {
    pub selected_file: usize,
    pub preferences: ViewPreferences,
    pub viewport: Viewport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLayout {
    Unified,
    Vertical,
    Stacked,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ViewPreferences {
    pub layout: DiffLayout,
    pub granularity: DiffGranularity,
    pub context_lines: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffGranularity {
    Line,
    Character,
}

impl ViewPreferences {
    pub const fn line(layout: DiffLayout) -> Self {
        Self {
            layout,
            granularity: DiffGranularity::Line,
            context_lines: 3,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct NavigationBounds {
    pub file_count: usize,
    pub unified_line_count: usize,
    pub paired_line_count: usize,
}

impl NavigationBounds {
    fn line_count(&self, layout: DiffLayout) -> usize {
        match layout {
            DiffLayout::Unified => self.unified_line_count,
            DiffLayout::Vertical | DiffLayout::Stacked => self.paired_line_count,
        }
    }
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
    NextLayout,
    ToggleGranularity,
    IncreaseContext,
    DecreaseContext,
    Quit,
    Interrupt,
    Resize { width: u16, height: u16 },
}

pub fn transition_interaction(
    interaction: &Interaction,
    input: Input,
    bounds: &NavigationBounds,
) -> Transition {
    match input {
        Input::NextFile => return select_next_file(interaction, bounds.file_count),
        Input::PreviousFile => return select_previous_file(interaction, bounds.file_count),
        Input::NextLayout => return select_next_layout(interaction, bounds),
        Input::ToggleGranularity => return toggle_granularity(interaction),
        Input::IncreaseContext => return change_context(interaction, ContextChange::Increase),
        Input::DecreaseContext => return change_context(interaction, ContextChange::Decrease),
        _ => {}
    }
    let transition = transition(
        &interaction.viewport,
        input,
        bounds.line_count(interaction.preferences.layout),
    );

    match transition {
        Transition::Exit => Transition::Exit,
        Transition::Redraw(viewport) => Transition::RedrawInteraction(Interaction {
            selected_file: interaction.selected_file,
            preferences: ViewPreferences {
                layout: interaction.preferences.layout,
                granularity: interaction.preferences.granularity,
                context_lines: interaction.preferences.context_lines,
            },
            viewport,
        }),
        Transition::RedrawInteraction(_) => unreachable!("viewport transition cannot nest"),
    }
}

fn select_next_layout(interaction: &Interaction, bounds: &NavigationBounds) -> Transition {
    let layout = match interaction.preferences.layout {
        DiffLayout::Unified => DiffLayout::Vertical,
        DiffLayout::Vertical => DiffLayout::Stacked,
        DiffLayout::Stacked => DiffLayout::Unified,
    };

    let maximum_offset = bounds
        .line_count(layout)
        .saturating_sub(interaction.viewport.height);
    let offset = interaction.viewport.offset.min(maximum_offset);

    Transition::RedrawInteraction(Interaction {
        selected_file: interaction.selected_file,
        preferences: ViewPreferences {
            layout,
            granularity: interaction.preferences.granularity,
            context_lines: interaction.preferences.context_lines,
        },
        viewport: Viewport {
            offset,
            height: interaction.viewport.height,
        },
    })
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
        preferences: ViewPreferences {
            layout: interaction.preferences.layout,
            granularity: interaction.preferences.granularity,
            context_lines: interaction.preferences.context_lines,
        },
        viewport: Viewport {
            offset: 0,
            height: interaction.viewport.height,
        },
    })
}

enum ContextChange {
    Increase,
    Decrease,
}

fn toggle_granularity(interaction: &Interaction) -> Transition {
    let granularity = match interaction.preferences.granularity {
        DiffGranularity::Line => DiffGranularity::Character,
        DiffGranularity::Character => DiffGranularity::Line,
    };

    redraw_preferences(
        interaction,
        granularity,
        interaction.preferences.context_lines,
    )
}

fn change_context(interaction: &Interaction, change: ContextChange) -> Transition {
    let context_lines = match change {
        ContextChange::Increase => interaction.preferences.context_lines.saturating_add(1),
        ContextChange::Decrease => interaction.preferences.context_lines.saturating_sub(1),
    };

    redraw_preferences(
        interaction,
        interaction.preferences.granularity,
        context_lines,
    )
}

fn redraw_preferences(
    interaction: &Interaction,
    granularity: DiffGranularity,
    context_lines: usize,
) -> Transition {
    Transition::RedrawInteraction(Interaction {
        selected_file: interaction.selected_file,
        preferences: ViewPreferences {
            layout: interaction.preferences.layout,
            granularity,
            context_lines,
        },
        viewport: interaction.viewport.clone(),
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
        Input::NextFile
        | Input::PreviousFile
        | Input::NextLayout
        | Input::ToggleGranularity
        | Input::IncreaseContext
        | Input::DecreaseContext => Transition::Redraw(viewport.clone()),
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
        DiffGranularity, DiffLayout, Input, Interaction, NavigationBounds, Transition,
        ViewPreferences, Viewport, transition, transition_interaction,
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
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                offset: 4,
                height: 3,
            },
        };
        let bounds = NavigationBounds {
            file_count: 2,
            unified_line_count: 10,
            paired_line_count: 10,
        };

        let next = transition_interaction(&interaction, Input::NextFile, &bounds);

        assert_eq!(
            next,
            Transition::RedrawInteraction(Interaction {
                selected_file: 0,
                preferences: ViewPreferences::line(DiffLayout::Unified),
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
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                offset: 4,
                height: 3,
            },
        };
        let bounds = NavigationBounds {
            file_count: 2,
            unified_line_count: 10,
            paired_line_count: 10,
        };

        let next = transition_interaction(&interaction, Input::PreviousFile, &bounds);

        assert_eq!(
            next,
            Transition::RedrawInteraction(Interaction {
                selected_file: 1,
                preferences: ViewPreferences::line(DiffLayout::Unified),
                viewport: Viewport {
                    offset: 0,
                    height: 3,
                },
            })
        );
    }

    #[test]
    fn cycles_layouts_without_changing_the_reader_position() {
        let interaction = Interaction {
            selected_file: 1,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                offset: 4,
                height: 3,
            },
        };
        let bounds = NavigationBounds {
            file_count: 2,
            unified_line_count: 10,
            paired_line_count: 10,
        };

        let Transition::RedrawInteraction(vertical) =
            transition_interaction(&interaction, Input::NextLayout, &bounds)
        else {
            panic!("layout change redraws interaction");
        };
        let Transition::RedrawInteraction(stacked) =
            transition_interaction(&vertical, Input::NextLayout, &bounds)
        else {
            panic!("layout change redraws interaction");
        };
        let Transition::RedrawInteraction(unified) =
            transition_interaction(&stacked, Input::NextLayout, &bounds)
        else {
            panic!("layout change redraws interaction");
        };

        assert_eq!(vertical.preferences.layout, DiffLayout::Vertical);
        assert_eq!(stacked.preferences.layout, DiffLayout::Stacked);
        assert_eq!(unified, interaction);
    }

    #[test]
    fn toggles_detail_and_bounds_context_without_moving_the_viewport() {
        let interaction = Interaction {
            selected_file: 1,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                offset: 4,
                height: 3,
            },
        };
        let bounds = NavigationBounds {
            file_count: 2,
            unified_line_count: 10,
            paired_line_count: 10,
        };

        let Transition::RedrawInteraction(character) =
            transition_interaction(&interaction, Input::ToggleGranularity, &bounds)
        else {
            panic!("detail toggle redraws interaction");
        };
        let Transition::RedrawInteraction(one_more_context) =
            transition_interaction(&character, Input::IncreaseContext, &bounds)
        else {
            panic!("context increase redraws interaction");
        };

        assert_eq!(
            character.preferences.granularity,
            DiffGranularity::Character
        );
        assert_eq!(one_more_context.preferences.context_lines, 4);
        assert_eq!(one_more_context.viewport, interaction.viewport);
    }
}
