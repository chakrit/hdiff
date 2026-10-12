#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewport {
    pub vertical_offset: usize,
    pub horizontal_offset: u16,
    pub width: u16,
    pub height: usize,
}

impl Viewport {
    pub const fn new(width: u16, height: usize) -> Self {
        Self {
            vertical_offset: 0,
            horizontal_offset: 0,
            width,
            height,
        }
    }
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

impl DiffLayout {
    pub const fn next(self) -> Self {
        match self {
            Self::Unified => Self::Vertical,
            Self::Vertical => Self::Stacked,
            Self::Stacked => Self::Unified,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Unified => "unified",
            Self::Vertical => "vertical",
            Self::Stacked => "stacked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewPreferences {
    pub layout: DiffLayout,
    pub granularity: DiffGranularity,
    pub wrapping: Wrapping,
    pub context_lines: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrapping {
    Disabled,
    Enabled,
}

impl Wrapping {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "off",
            Self::Enabled => "on",
        }
    }

    const fn toggled(self) -> Self {
        match self {
            Self::Disabled => Self::Enabled,
            Self::Enabled => Self::Disabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffGranularity {
    Line,
    Character,
}

impl DiffGranularity {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Character => "character",
        }
    }
}

impl ViewPreferences {
    pub const fn line(layout: DiffLayout) -> Self {
        Self {
            layout,
            granularity: DiffGranularity::Line,
            wrapping: Wrapping::Disabled,
            context_lines: 3,
        }
    }

    /// The preferences `input` selects; every other input keeps them.
    pub fn after(self, input: &Input) -> Self {
        match input {
            Input::NextLayout => Self {
                layout: self.layout.next(),
                ..self
            },
            Input::ToggleGranularity => Self {
                granularity: self.granularity.toggled(),
                ..self
            },
            Input::ToggleWrapping => Self {
                wrapping: self.wrapping.toggled(),
                ..self
            },
            Input::IncreaseContext => Self {
                context_lines: self.context_lines.saturating_add(1),
                ..self
            },
            Input::DecreaseContext => Self {
                context_lines: self.context_lines.saturating_sub(1),
                ..self
            },
            _ => self,
        }
    }
}

impl DiffGranularity {
    const fn toggled(self) -> Self {
        match self {
            Self::Line => Self::Character,
            Self::Character => Self::Line,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct NavigationBounds {
    pub file_count: usize,
    pub target_display: DisplayBounds,
    /// The target display's vertical offset that keeps the input's anchor line in place:
    /// the diff pane's top line for resizes, its middle line otherwise. Resizes, wrapping
    /// toggles, and context changes use it.
    pub anchored_vertical_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayBounds {
    pub maximum_vertical_offset: usize,
    pub maximum_horizontal_offset: u16,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Left,
    Right,
    HorizontalStart,
    HorizontalEnd,
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
    ToggleWrapping,
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
    let viewport = &interaction.viewport;
    let display = bounds.target_display;
    let preferences = interaction.preferences.after(&input);

    match input {
        Input::NextFile => return select_next_file(interaction, bounds.file_count),
        Input::PreviousFile => return select_previous_file(interaction, bounds.file_count),
        Input::NextLayout => {
            let viewport =
                bounded_viewport(viewport.vertical_offset, viewport.height, viewport, display);
            return redraw_preferences(interaction, preferences, viewport);
        }
        Input::ToggleGranularity => {
            return redraw_preferences(interaction, preferences, viewport.clone());
        }
        Input::ToggleWrapping | Input::IncreaseContext | Input::DecreaseContext => {
            let anchored = bounds.anchored_vertical_offset;
            let viewport = bounded_viewport(anchored, viewport.height, viewport, display);
            return redraw_preferences(interaction, preferences, viewport);
        }
        Input::Resize { width, height } => {
            let resized = Viewport {
                width,
                ..viewport.clone()
            };
            let anchored = bounds.anchored_vertical_offset;
            let viewport = bounded_viewport(anchored, usize::from(height), &resized, display);
            return redraw_preferences(interaction, preferences, viewport);
        }
        _ => {}
    }

    match transition(viewport, input, display) {
        Transition::Exit => Transition::Exit,
        Transition::Redraw(viewport) => redraw_preferences(interaction, preferences, viewport),
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
        preferences: interaction.preferences,
        viewport: Viewport::new(interaction.viewport.width, interaction.viewport.height),
    })
}

fn redraw_preferences(
    interaction: &Interaction,
    preferences: ViewPreferences,
    viewport: Viewport,
) -> Transition {
    Transition::RedrawInteraction(Interaction {
        selected_file: interaction.selected_file,
        preferences,
        viewport,
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum Transition {
    Redraw(Viewport),
    RedrawInteraction(Interaction),
    Exit,
}

fn transition(viewport: &Viewport, input: Input, bounds: DisplayBounds) -> Transition {
    match input {
        Input::Quit | Input::Interrupt => Transition::Exit,
        Input::Left => redraw_horizontal(
            viewport.horizontal_offset.saturating_sub(1),
            viewport,
            bounds.maximum_horizontal_offset,
        ),
        Input::Right => redraw_horizontal(
            viewport.horizontal_offset.saturating_add(1),
            viewport,
            bounds.maximum_horizontal_offset,
        ),
        Input::HorizontalStart => redraw_horizontal(0, viewport, bounds.maximum_horizontal_offset),
        Input::HorizontalEnd => redraw_horizontal(
            bounds.maximum_horizontal_offset,
            viewport,
            bounds.maximum_horizontal_offset,
        ),
        Input::NextFile
        | Input::PreviousFile
        | Input::NextLayout
        | Input::ToggleGranularity
        | Input::ToggleWrapping
        | Input::IncreaseContext
        | Input::DecreaseContext
        | Input::Resize { .. } => Transition::Redraw(viewport.clone()),
        Input::Down => redraw(
            viewport.vertical_offset.saturating_add(1),
            viewport.height,
            viewport,
            bounds,
        ),
        Input::Up => redraw(
            viewport.vertical_offset.saturating_sub(1),
            viewport.height,
            viewport,
            bounds,
        ),
        Input::HalfPageDown => {
            let distance = viewport.height.saturating_div(2).max(1);

            redraw(
                viewport.vertical_offset.saturating_add(distance),
                viewport.height,
                viewport,
                bounds,
            )
        }
        Input::HalfPageUp => {
            let distance = viewport.height.saturating_div(2).max(1);

            redraw(
                viewport.vertical_offset.saturating_sub(distance),
                viewport.height,
                viewport,
                bounds,
            )
        }
        Input::Top => redraw(0, viewport.height, viewport, bounds),
        Input::Bottom => redraw(
            bounds.maximum_vertical_offset,
            viewport.height,
            viewport,
            bounds,
        ),
    }
}

fn redraw(
    vertical_offset: usize,
    height: usize,
    viewport: &Viewport,
    bounds: DisplayBounds,
) -> Transition {
    Transition::Redraw(bounded_viewport(vertical_offset, height, viewport, bounds))
}

/// `viewport` at `vertical_offset` and `height`, with both offsets clamped to `bounds`.
fn bounded_viewport(
    vertical_offset: usize,
    height: usize,
    viewport: &Viewport,
    bounds: DisplayBounds,
) -> Viewport {
    Viewport {
        vertical_offset: vertical_offset.min(bounds.maximum_vertical_offset),
        horizontal_offset: viewport
            .horizontal_offset
            .min(bounds.maximum_horizontal_offset),
        width: viewport.width,
        height,
    }
}

fn redraw_horizontal(
    horizontal_offset: u16,
    viewport: &Viewport,
    maximum_horizontal_offset: u16,
) -> Transition {
    Transition::Redraw(Viewport {
        vertical_offset: viewport.vertical_offset,
        horizontal_offset: horizontal_offset.min(maximum_horizontal_offset),
        width: viewport.width,
        height: viewport.height,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DiffGranularity, DiffLayout, DisplayBounds, Input, Interaction, NavigationBounds,
        Transition, ViewPreferences, Viewport, Wrapping, transition, transition_interaction,
    };

    #[test]
    fn moves_and_clamps_the_vertical_viewport() {
        let viewport = Viewport {
            vertical_offset: 4,
            horizontal_offset: 2,
            width: 80,
            height: 3,
        };

        let next = transition(
            &viewport,
            Input::Down,
            DisplayBounds {
                maximum_vertical_offset: 7,
                maximum_horizontal_offset: 4,
            },
        );

        assert_eq!(
            next,
            Transition::Redraw(Viewport {
                vertical_offset: 5,
                horizontal_offset: 2,
                width: 80,
                height: 3,
            })
        );
    }

    #[test]
    fn moves_and_clamps_the_horizontal_viewport() {
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                vertical_offset: 4,
                horizontal_offset: 2,
                width: 80,
                height: 3,
            },
        };
        let display = DisplayBounds {
            maximum_vertical_offset: 7,
            maximum_horizontal_offset: 3,
        };
        let bounds = NavigationBounds {
            file_count: 1,
            target_display: display,
            anchored_vertical_offset: 0,
        };

        let Transition::RedrawInteraction(right) =
            transition_interaction(&interaction, Input::Right, &bounds)
        else {
            panic!("horizontal movement redraws interaction");
        };
        let Transition::RedrawInteraction(at_end) =
            transition_interaction(&right, Input::Right, &bounds)
        else {
            panic!("horizontal movement redraws interaction");
        };
        let Transition::RedrawInteraction(clamped) =
            transition_interaction(&at_end, Input::Right, &bounds)
        else {
            panic!("horizontal movement redraws interaction");
        };
        let Transition::RedrawInteraction(left) =
            transition_interaction(&clamped, Input::Left, &bounds)
        else {
            panic!("horizontal movement redraws interaction");
        };

        assert_eq!(right.viewport.horizontal_offset, 3);
        assert_eq!(at_end.viewport.horizontal_offset, 3);
        assert_eq!(clamped.viewport.horizontal_offset, 3);
        assert_eq!(left.viewport.horizontal_offset, 2);
        assert_eq!(left.viewport.vertical_offset, 4);
    }

    #[test]
    fn jumps_to_horizontal_boundaries() {
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                vertical_offset: 4,
                horizontal_offset: 2,
                width: 80,
                height: 3,
            },
        };
        let display = DisplayBounds {
            maximum_vertical_offset: 7,
            maximum_horizontal_offset: 12,
        };
        let bounds = NavigationBounds {
            file_count: 1,
            target_display: display,
            anchored_vertical_offset: 0,
        };

        let Transition::RedrawInteraction(start) =
            transition_interaction(&interaction, Input::HorizontalStart, &bounds)
        else {
            panic!("horizontal jump redraws interaction");
        };
        let Transition::RedrawInteraction(end) =
            transition_interaction(&interaction, Input::HorizontalEnd, &bounds)
        else {
            panic!("horizontal jump redraws interaction");
        };

        assert_eq!(start.viewport.horizontal_offset, 0);
        assert_eq!(end.viewport.horizontal_offset, 12);
        assert_eq!(end.viewport.vertical_offset, 4);
    }

    #[test]
    fn exits_for_quit_and_interrupt() {
        let viewport = Viewport {
            vertical_offset: 0,
            horizontal_offset: 0,
            width: 80,
            height: 1,
        };

        let bounds = DisplayBounds {
            maximum_vertical_offset: 0,
            maximum_horizontal_offset: 0,
        };

        assert_eq!(transition(&viewport, Input::Quit, bounds), Transition::Exit);
        assert_eq!(
            transition(&viewport, Input::Interrupt, bounds),
            Transition::Exit
        );
    }

    #[test]
    fn resize_moves_to_the_anchored_offset_clamped_for_the_new_size() {
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                vertical_offset: 2,
                horizontal_offset: 4,
                width: 40,
                height: 3,
            },
        };
        let resize = |anchored_vertical_offset| {
            let bounds = NavigationBounds {
                file_count: 1,
                target_display: DisplayBounds {
                    maximum_vertical_offset: 5,
                    maximum_horizontal_offset: 3,
                },
                anchored_vertical_offset,
            };
            let input = Input::Resize {
                width: 80,
                height: 9,
            };
            let Transition::RedrawInteraction(next) =
                transition_interaction(&interaction, input, &bounds)
            else {
                panic!("resize redraws interaction");
            };
            next.viewport
        };

        assert_eq!(
            resize(4),
            Viewport {
                vertical_offset: 4,
                horizontal_offset: 3,
                width: 80,
                height: 9,
            }
        );
        assert_eq!(resize(7).vertical_offset, 5);
    }

    #[test]
    fn toggles_wrapping_at_the_anchored_offset() {
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Vertical),
            viewport: Viewport {
                vertical_offset: 2,
                horizontal_offset: 4,
                width: 80,
                height: 9,
            },
        };
        let bounds = NavigationBounds {
            file_count: 1,
            target_display: DisplayBounds {
                maximum_vertical_offset: 20,
                maximum_horizontal_offset: 0,
            },
            anchored_vertical_offset: 11,
        };

        let Transition::RedrawInteraction(wrapped) =
            transition_interaction(&interaction, Input::ToggleWrapping, &bounds)
        else {
            panic!("wrapping toggle redraws interaction");
        };

        assert_eq!(wrapped.preferences.wrapping, Wrapping::Enabled);
        assert_eq!(wrapped.preferences.layout, DiffLayout::Vertical);
        assert_eq!(wrapped.viewport.vertical_offset, 11);
        assert_eq!(wrapped.viewport.horizontal_offset, 0);
    }

    #[test]
    fn rotates_files_and_resets_the_viewport() {
        let interaction = Interaction {
            selected_file: 1,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                vertical_offset: 4,
                horizontal_offset: 3,
                width: 80,
                height: 3,
            },
        };
        let display = DisplayBounds {
            maximum_vertical_offset: 7,
            maximum_horizontal_offset: 12,
        };
        let bounds = NavigationBounds {
            file_count: 2,
            target_display: display,
            anchored_vertical_offset: 0,
        };

        let next = transition_interaction(&interaction, Input::NextFile, &bounds);

        assert_eq!(
            next,
            Transition::RedrawInteraction(Interaction {
                selected_file: 0,
                preferences: ViewPreferences::line(DiffLayout::Unified),
                viewport: Viewport {
                    vertical_offset: 0,
                    horizontal_offset: 0,
                    width: 80,
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
                vertical_offset: 4,
                horizontal_offset: 3,
                width: 80,
                height: 3,
            },
        };
        let display = DisplayBounds {
            maximum_vertical_offset: 7,
            maximum_horizontal_offset: 12,
        };
        let bounds = NavigationBounds {
            file_count: 2,
            target_display: display,
            anchored_vertical_offset: 0,
        };

        let next = transition_interaction(&interaction, Input::PreviousFile, &bounds);

        assert_eq!(
            next,
            Transition::RedrawInteraction(Interaction {
                selected_file: 1,
                preferences: ViewPreferences::line(DiffLayout::Unified),
                viewport: Viewport {
                    vertical_offset: 0,
                    horizontal_offset: 0,
                    width: 80,
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
                vertical_offset: 4,
                horizontal_offset: 3,
                width: 80,
                height: 3,
            },
        };
        let display = DisplayBounds {
            maximum_vertical_offset: 7,
            maximum_horizontal_offset: 12,
        };
        let bounds = NavigationBounds {
            file_count: 2,
            target_display: display,
            anchored_vertical_offset: 0,
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
                vertical_offset: 4,
                horizontal_offset: 3,
                width: 80,
                height: 3,
            },
        };
        let display = DisplayBounds {
            maximum_vertical_offset: 7,
            maximum_horizontal_offset: 12,
        };
        let bounds = NavigationBounds {
            file_count: 2,
            target_display: display,
            anchored_vertical_offset: 4,
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
