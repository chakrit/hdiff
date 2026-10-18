use std::num::NonZeroUsize;

use crate::preferences::ViewPreferences;

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
    pub keymap: KeymapPopup,
}

/// The popup listing every shortcut. Rules: `docs/spec/architecture.md` (Interaction model).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeymapPopup {
    Closed,
    Open,
}

impl KeymapPopup {
    /// The input this popup lets through: while open, only its close keys act, all closing
    /// it, and interrupts and resizes still apply.
    pub fn admit(self, input: Input) -> Option<Input> {
        match (self, input) {
            (Self::Closed, Input::CloseKeymap) => None,
            (Self::Closed, input) => Some(input),
            (Self::Open, Input::OpenKeymap | Input::CloseKeymap | Input::Quit) => {
                Some(Input::CloseKeymap)
            }
            (Self::Open, input @ (Input::Interrupt | Input::Resize { .. })) => Some(input),
            (Self::Open, _) => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct NavigationBounds {
    pub page_count: NonZeroUsize,
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
    ToggleFileList,
    OpenKeymap,
    CloseKeymap,
    Quit,
    Interrupt,
    Resize { width: u16, height: u16 },
}

/// The preferences `input` selects; every other input keeps them.
pub fn preferences_after(preferences: ViewPreferences, input: &Input) -> ViewPreferences {
    match input {
        Input::NextLayout => ViewPreferences {
            layout: preferences.layout.next(),
            ..preferences
        },
        Input::ToggleGranularity => ViewPreferences {
            granularity: preferences.granularity.toggled(),
            ..preferences
        },
        Input::ToggleWrapping => ViewPreferences {
            wrapping: preferences.wrapping.toggled(),
            ..preferences
        },
        Input::IncreaseContext => ViewPreferences {
            context_lines: preferences.context_lines.saturating_add(1),
            ..preferences
        },
        Input::DecreaseContext => ViewPreferences {
            context_lines: preferences.context_lines.saturating_sub(1),
            ..preferences
        },
        Input::ToggleFileList => ViewPreferences {
            file_list: preferences.file_list.toggled(),
            ..preferences
        },
        _ => preferences,
    }
}

pub fn transition_interaction(
    interaction: &Interaction,
    input: Input,
    bounds: &NavigationBounds,
) -> Transition {
    let viewport = &interaction.viewport;
    let display = bounds.target_display;
    let preferences = preferences_after(interaction.preferences, &input);

    match input {
        Input::NextFile => return select_next_file(interaction, bounds.page_count),
        Input::PreviousFile => return select_previous_file(interaction, bounds.page_count),
        Input::NextLayout => {
            let viewport =
                bounded_viewport(viewport.vertical_offset, viewport.height, viewport, display);
            return redraw_preferences(interaction, preferences, viewport);
        }
        Input::ToggleGranularity => {
            return redraw_preferences(interaction, preferences, viewport.clone());
        }
        Input::ToggleWrapping
        | Input::IncreaseContext
        | Input::DecreaseContext
        | Input::ToggleFileList => {
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
        Input::OpenKeymap => return redraw_keymap(interaction, KeymapPopup::Open),
        Input::CloseKeymap => return redraw_keymap(interaction, KeymapPopup::Closed),
        _ => {}
    }

    match transition(viewport, input, display) {
        Transition::Exit => Transition::Exit,
        Transition::Redraw(viewport) => redraw_preferences(interaction, preferences, viewport),
        Transition::RedrawInteraction(_) => unreachable!("viewport transition cannot nest"),
    }
}

fn select_next_file(interaction: &Interaction, page_count: NonZeroUsize) -> Transition {
    let selected_file = (interaction.selected_file + 1) % page_count;

    redraw_selected_file(interaction, selected_file)
}

fn select_previous_file(interaction: &Interaction, page_count: NonZeroUsize) -> Transition {
    let last_page = page_count.get() - 1;
    let selected_file = interaction
        .selected_file
        .checked_sub(1)
        .unwrap_or(last_page);

    redraw_selected_file(interaction, selected_file)
}

fn redraw_selected_file(interaction: &Interaction, selected_file: usize) -> Transition {
    Transition::RedrawInteraction(Interaction {
        selected_file,
        preferences: interaction.preferences,
        viewport: Viewport::new(interaction.viewport.width, interaction.viewport.height),
        keymap: interaction.keymap,
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
        keymap: interaction.keymap,
    })
}

fn redraw_keymap(interaction: &Interaction, keymap: KeymapPopup) -> Transition {
    Transition::RedrawInteraction(Interaction {
        selected_file: interaction.selected_file,
        preferences: interaction.preferences,
        viewport: interaction.viewport.clone(),
        keymap,
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
        | Input::ToggleFileList
        | Input::OpenKeymap
        | Input::CloseKeymap
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
    use std::num::NonZeroUsize;

    use super::{
        DisplayBounds, Input, Interaction, KeymapPopup, NavigationBounds, Transition, Viewport,
        transition, transition_interaction,
    };
    use crate::preferences::{DiffGranularity, DiffLayout, FileList, ViewPreferences, Wrapping};

    const TWO_PAGES: NonZeroUsize = NonZeroUsize::new(2).expect("two is nonzero");

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
            keymap: KeymapPopup::Closed,
            selected_file: 0,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
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
            page_count: NonZeroUsize::MIN,
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
            keymap: KeymapPopup::Closed,
            selected_file: 0,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
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
            page_count: NonZeroUsize::MIN,
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
            keymap: KeymapPopup::Closed,
            selected_file: 0,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
            viewport: Viewport {
                vertical_offset: 2,
                horizontal_offset: 4,
                width: 40,
                height: 3,
            },
        };
        let resize = |anchored_vertical_offset| {
            let bounds = NavigationBounds {
                page_count: NonZeroUsize::MIN,
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
            keymap: KeymapPopup::Closed,
            selected_file: 0,
            preferences: ViewPreferences::initial(DiffLayout::Vertical),
            viewport: Viewport {
                vertical_offset: 2,
                horizontal_offset: 4,
                width: 80,
                height: 9,
            },
        };
        let bounds = NavigationBounds {
            page_count: NonZeroUsize::MIN,
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
            keymap: KeymapPopup::Closed,
            selected_file: 1,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
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
            page_count: TWO_PAGES,
            target_display: display,
            anchored_vertical_offset: 0,
        };

        let next = transition_interaction(&interaction, Input::NextFile, &bounds);

        assert_eq!(
            next,
            Transition::RedrawInteraction(Interaction {
                keymap: KeymapPopup::Closed,
                selected_file: 0,
                preferences: ViewPreferences::initial(DiffLayout::Unified),
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
            keymap: KeymapPopup::Closed,
            selected_file: 0,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
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
            page_count: TWO_PAGES,
            target_display: display,
            anchored_vertical_offset: 0,
        };

        let next = transition_interaction(&interaction, Input::PreviousFile, &bounds);

        assert_eq!(
            next,
            Transition::RedrawInteraction(Interaction {
                keymap: KeymapPopup::Closed,
                selected_file: 1,
                preferences: ViewPreferences::initial(DiffLayout::Unified),
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
            keymap: KeymapPopup::Closed,
            selected_file: 1,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
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
            page_count: TWO_PAGES,
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
            keymap: KeymapPopup::Closed,
            selected_file: 1,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
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
            page_count: TWO_PAGES,
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

    /// architecture.md (Interaction model): while the keymap popup is open, `?`, `Esc`,
    /// and `q` close it without exiting, `Ctrl-C` exits, and every other key does nothing.
    #[test]
    fn admits_only_closing_keys_interrupts_and_resizes_while_the_keymap_is_open() {
        let open = KeymapPopup::Open;
        let resize = || Input::Resize {
            width: 80,
            height: 9,
        };

        for closing in [Input::OpenKeymap, Input::CloseKeymap, Input::Quit] {
            assert_eq!(open.admit(closing), Some(Input::CloseKeymap));
        }
        for ignored in [
            Input::Down,
            Input::NextFile,
            Input::NextLayout,
            Input::ToggleFileList,
        ] {
            assert_eq!(open.admit(ignored), None);
        }
        assert_eq!(open.admit(Input::Interrupt), Some(Input::Interrupt));
        assert_eq!(open.admit(resize()), Some(resize()));
        assert_eq!(KeymapPopup::Closed.admit(Input::Quit), Some(Input::Quit));
        assert_eq!(KeymapPopup::Closed.admit(Input::CloseKeymap), None);
    }

    #[test]
    fn opens_and_closes_the_keymap_without_moving_the_viewport() {
        let interaction = Interaction {
            keymap: KeymapPopup::Closed,
            selected_file: 1,
            preferences: ViewPreferences::initial(DiffLayout::Unified),
            viewport: Viewport {
                vertical_offset: 4,
                horizontal_offset: 3,
                width: 80,
                height: 9,
            },
        };
        let bounds = NavigationBounds {
            page_count: TWO_PAGES,
            target_display: DisplayBounds {
                maximum_vertical_offset: 7,
                maximum_horizontal_offset: 12,
            },
            anchored_vertical_offset: 0,
        };

        let Transition::RedrawInteraction(open) =
            transition_interaction(&interaction, Input::OpenKeymap, &bounds)
        else {
            panic!("opening the keymap redraws interaction");
        };
        let Transition::RedrawInteraction(closed) =
            transition_interaction(&open, Input::CloseKeymap, &bounds)
        else {
            panic!("closing the keymap redraws interaction");
        };

        assert_eq!(open.keymap, KeymapPopup::Open);
        assert_eq!(open.viewport, interaction.viewport);
        assert_eq!(closed, interaction);
    }

    /// architecture.md (View model): showing or hiding the file list keeps the row on the
    /// diff pane's middle line on that line.
    #[test]
    fn toggles_the_file_list_at_the_anchored_offset() {
        let interaction = Interaction {
            keymap: KeymapPopup::Closed,
            selected_file: 0,
            preferences: ViewPreferences::initial(DiffLayout::Vertical),
            viewport: Viewport {
                vertical_offset: 2,
                horizontal_offset: 9,
                width: 80,
                height: 9,
            },
        };
        let bounds = NavigationBounds {
            page_count: NonZeroUsize::MIN,
            target_display: DisplayBounds {
                maximum_vertical_offset: 20,
                maximum_horizontal_offset: 4,
            },
            anchored_vertical_offset: 6,
        };

        let Transition::RedrawInteraction(hidden) =
            transition_interaction(&interaction, Input::ToggleFileList, &bounds)
        else {
            panic!("file-list toggle redraws interaction");
        };
        let Transition::RedrawInteraction(shown) =
            transition_interaction(&hidden, Input::ToggleFileList, &bounds)
        else {
            panic!("file-list toggle redraws interaction");
        };

        assert_eq!(hidden.preferences.file_list, FileList::Hidden);
        assert_eq!(hidden.viewport.vertical_offset, 6);
        assert_eq!(hidden.viewport.horizontal_offset, 4);
        assert_eq!(shown.preferences.file_list, FileList::Shown);
    }
}
