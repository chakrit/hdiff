//! Interaction state and its transitions. Rules: `docs/spec/architecture.md` (Core model,
//! Interaction model, View model).

use std::num::NonZeroUsize;

use ratatui::layout::Size;

use crate::{
    preferences::{PreferenceChange, ViewPreferences},
    prepared::PageIndex,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interaction {
    pub page: PageIndex,
    pub preferences: ViewPreferences,
    pub viewport: Viewport,
    pub keymap: KeymapPopup,
}

impl Interaction {
    /// The first page of a document on a `screen`-sized terminal, scrolled to its start.
    pub const fn start(preferences: ViewPreferences, screen: Size) -> Self {
        Self {
            page: PageIndex::FIRST,
            preferences,
            viewport: Viewport::at_start(screen),
            keymap: KeymapPopup::Closed,
        }
    }

    /// The preferences and screen the display shows now.
    pub const fn shown(&self) -> Target {
        Target {
            preferences: self.preferences,
            screen: self.viewport.screen,
        }
    }

    /// The preferences and screen the display shows after `event`.
    pub fn target(&self, event: &Event) -> Target {
        let current = self.shown();

        match event {
            Event::Command(Command::Preference(change)) => Target {
                preferences: self.preferences.after(*change),
                ..current
            },
            Event::Resize(screen) => Target {
                screen: *screen,
                ..current
            },
            Event::Command(
                Command::Movement(_) | Command::Page(_) | Command::Keymap(_) | Command::Quit,
            )
            | Event::Interrupt => current,
        }
    }
}

/// The terminal screen and the reader's position in the active layout's screen lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub screen: Size,
    pub scroll: ScrollPosition,
}

impl Viewport {
    pub const fn at_start(screen: Size) -> Self {
        Self {
            screen,
            scroll: ScrollPosition::START,
        }
    }
}

/// The first visible screen line and the synchronized horizontal offset in cells. Rules:
/// `docs/spec/architecture.md` (Viewport coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollPosition {
    pub vertical: usize,
    pub horizontal: u16,
}

impl ScrollPosition {
    pub const START: Self = Self {
        vertical: 0,
        horizontal: 0,
    };

    fn clamped(self, bounds: DisplayBounds) -> Self {
        Self {
            vertical: self.vertical.min(bounds.maximum_vertical_offset),
            horizontal: self.horizontal.min(bounds.maximum_horizontal_offset),
        }
    }
}

/// The popup listing every shortcut. Rules: `docs/spec/architecture.md` (Interaction model).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeymapPopup {
    Closed,
    Open,
}

impl KeymapPopup {
    /// The command this popup lets through: while open, only its close keys act, all closing
    /// it.
    pub fn admit(self, command: Command) -> Option<Command> {
        match (self, command) {
            (Self::Closed, Command::Keymap(Self::Closed)) => None,
            (Self::Closed, command) => Some(command),
            (Self::Open, Command::Keymap(_) | Command::Quit) => Some(Command::Keymap(Self::Closed)),
            (Self::Open, Command::Movement(_) | Command::Page(_) | Command::Preference(_)) => None,
        }
    }
}

/// One event of the interactive session: a bound command, or a session event that applies
/// in every mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Command(Command),
    Interrupt,
    Resize(Size),
}

impl Event {
    /// The event once `keymap` filters bound commands; session events always apply.
    pub fn admitted(self, keymap: KeymapPopup) -> Option<Self> {
        match self {
            Self::Command(command) => keymap.admit(command).map(Self::Command),
            Self::Interrupt | Self::Resize(_) => Some(self),
        }
    }

    /// Where the display after this event starts. See `docs/spec/architecture.md` (View
    /// model).
    pub const fn viewport_policy(&self) -> ViewportPolicy {
        match self {
            Self::Resize(_) => ViewportPolicy::Anchor(AnchorLine::PaneTop),
            Self::Command(Command::Preference(
                PreferenceChange::ToggleWrapping
                | PreferenceChange::IncreaseContext
                | PreferenceChange::DecreaseContext
                | PreferenceChange::ToggleFileList,
            )) => ViewportPolicy::Anchor(AnchorLine::PaneMiddle),
            Self::Command(Command::Page(_)) => ViewportPolicy::Reset,
            Self::Command(
                Command::Preference(
                    PreferenceChange::NextLayout | PreferenceChange::ToggleGranularity,
                )
                | Command::Movement(_)
                | Command::Keymap(_)
                | Command::Quit,
            )
            | Self::Interrupt => ViewportPolicy::Keep,
        }
    }
}

/// What a bound key asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Movement(Movement),
    Page(PageStep),
    Preference(PreferenceChange),
    /// Opens or closes the keymap popup.
    Keymap(KeymapPopup),
    Quit,
}

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

impl Movement {
    /// `viewport`'s scroll position after this movement, within `bounds`.
    fn applied(self, viewport: Viewport, bounds: DisplayBounds) -> ScrollPosition {
        let scroll = viewport.scroll;
        let horizontal = |horizontal| ScrollPosition {
            horizontal,
            ..scroll
        };
        let vertical = |vertical| ScrollPosition { vertical, ..scroll };
        let half_page = usize::from(viewport.screen.height / 2).max(1);

        let moved = match self {
            Self::Left => horizontal(scroll.horizontal.saturating_sub(1)),
            Self::Right => horizontal(scroll.horizontal.saturating_add(1)),
            Self::FarLeft => horizontal(0),
            Self::FarRight => horizontal(bounds.maximum_horizontal_offset),
            Self::Down => vertical(scroll.vertical.saturating_add(1)),
            Self::Up => vertical(scroll.vertical.saturating_sub(1)),
            Self::HalfPageDown => vertical(scroll.vertical.saturating_add(half_page)),
            Self::HalfPageUp => vertical(scroll.vertical.saturating_sub(half_page)),
            Self::Top => vertical(0),
            Self::Bottom => vertical(bounds.maximum_vertical_offset),
        };

        moved.clamped(bounds)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageStep {
    Next,
    Previous,
}

impl PageStep {
    fn applied(self, page: PageIndex, page_count: NonZeroUsize) -> PageIndex {
        match self {
            Self::Next => page.next(page_count),
            Self::Previous => page.previous(page_count),
        }
    }
}

/// How the display after an event places its first line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportPolicy {
    /// The current position, within the new display's bounds.
    Keep,
    /// The start of the display.
    Reset,
    /// The position that keeps the row on this diff-pane line on that line.
    Anchor(AnchorLine),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorLine {
    PaneTop,
    PaneMiddle,
}

/// The preferences and screen of the display an event leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub preferences: ViewPreferences,
    pub screen: Size,
}

/// The display an event leads to: its target, page count, bounds, and the scroll position,
/// within those bounds, that the event's viewport policy places it at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Destination {
    target: Target,
    page_count: NonZeroUsize,
    bounds: DisplayBounds,
    start: ScrollPosition,
}

impl Destination {
    /// The display of `target` with `bounds`, starting at `start` moved within them.
    pub fn new(
        target: Target,
        page_count: NonZeroUsize,
        bounds: DisplayBounds,
        start: ScrollPosition,
    ) -> Self {
        Self {
            target,
            page_count,
            bounds,
            start: start.clamped(bounds),
        }
    }

    fn viewport(&self) -> Viewport {
        Viewport {
            screen: self.target.screen,
            scroll: self.start,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayBounds {
    pub maximum_vertical_offset: usize,
    pub maximum_horizontal_offset: u16,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Transition {
    Redraw(Interaction),
    Exit,
}

/// The interaction after `event`, which leads to `destination`.
pub fn transition(
    interaction: &Interaction,
    event: Event,
    destination: &Destination,
) -> Transition {
    let next = match event {
        Event::Interrupt | Event::Command(Command::Quit) => return Transition::Exit,
        Event::Command(Command::Movement(movement)) => Interaction {
            viewport: Viewport {
                scroll: movement.applied(interaction.viewport, destination.bounds),
                ..interaction.viewport
            },
            ..*interaction
        },
        Event::Command(Command::Page(step)) => Interaction {
            page: step.applied(interaction.page, destination.page_count),
            viewport: destination.viewport(),
            ..*interaction
        },
        Event::Command(Command::Keymap(keymap)) => Interaction {
            keymap,
            viewport: destination.viewport(),
            ..*interaction
        },
        Event::Command(Command::Preference(_)) | Event::Resize(_) => Interaction {
            preferences: destination.target.preferences,
            viewport: destination.viewport(),
            ..*interaction
        },
    };

    Transition::Redraw(next)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use ratatui::layout::Size;

    use super::{
        Command, Destination, DisplayBounds, Event, Interaction, KeymapPopup, Movement, PageStep,
        ScrollPosition, Transition, Viewport, transition,
    };
    use crate::{
        preferences::{
            DiffGranularity, DiffLayout, FileList, PreferenceChange, ViewPreferences, Wrapping,
        },
        prepared::PageIndex,
    };

    const TWO_PAGES: NonZeroUsize = NonZeroUsize::new(2).expect("two is nonzero");

    fn second_page() -> PageIndex {
        PageIndex::FIRST.next(TWO_PAGES)
    }

    fn viewport(width: u16, height: u16, vertical: usize, horizontal: u16) -> Viewport {
        Viewport {
            screen: Size::new(width, height),
            scroll: ScrollPosition {
                vertical,
                horizontal,
            },
        }
    }

    fn interaction(page: PageIndex, layout: DiffLayout, viewport: Viewport) -> Interaction {
        Interaction {
            page,
            preferences: ViewPreferences::initial(layout),
            viewport,
            keymap: KeymapPopup::Closed,
        }
    }

    fn bounds(maximum_vertical_offset: usize, maximum_horizontal_offset: u16) -> DisplayBounds {
        DisplayBounds {
            maximum_vertical_offset,
            maximum_horizontal_offset,
        }
    }

    /// The interaction after `event` leads to a display with `bounds` among `page_count`
    /// pages, starting at `start`.
    fn after_starting_at(
        interaction: &Interaction,
        event: Event,
        page_count: NonZeroUsize,
        bounds: DisplayBounds,
        start: ScrollPosition,
    ) -> Interaction {
        let destination = Destination::new(interaction.target(&event), page_count, bounds, start);
        let Transition::Redraw(next) = transition(interaction, event, &destination) else {
            panic!("{event:?} redraws the interaction");
        };
        next
    }

    /// The interaction after `event`, whose display keeps the current position.
    fn after(interaction: &Interaction, event: Event, bounds: DisplayBounds) -> Interaction {
        let start = interaction.viewport.scroll;
        after_starting_at(interaction, event, TWO_PAGES, bounds, start)
    }

    fn movement(movement: Movement) -> Event {
        Event::Command(Command::Movement(movement))
    }

    fn preference(change: PreferenceChange) -> Event {
        Event::Command(Command::Preference(change))
    }

    #[test]
    fn moves_and_clamps_the_vertical_viewport() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, viewport(80, 3, 4, 2));

        let next = after(&start, movement(Movement::Down), bounds(7, 4));

        assert_eq!(next.viewport, viewport(80, 3, 5, 2));
    }

    #[test]
    fn moves_and_clamps_the_horizontal_viewport() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, viewport(80, 3, 4, 2));
        let display = bounds(7, 3);

        let right = after(&start, movement(Movement::Right), display);
        let at_end = after(&right, movement(Movement::Right), display);
        let clamped = after(&at_end, movement(Movement::Right), display);
        let left = after(&clamped, movement(Movement::Left), display);

        assert_eq!(right.viewport.scroll.horizontal, 3);
        assert_eq!(at_end.viewport.scroll.horizontal, 3);
        assert_eq!(clamped.viewport.scroll.horizontal, 3);
        assert_eq!(left.viewport.scroll.horizontal, 2);
        assert_eq!(left.viewport.scroll.vertical, 4);
    }

    #[test]
    fn jumps_to_horizontal_boundaries() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, viewport(80, 3, 4, 2));
        let display = bounds(7, 12);

        let far_left = after(&start, movement(Movement::FarLeft), display);
        let far_right = after(&start, movement(Movement::FarRight), display);

        assert_eq!(far_left.viewport.scroll.horizontal, 0);
        assert_eq!(far_right.viewport.scroll.horizontal, 12);
        assert_eq!(far_right.viewport.scroll.vertical, 4);
    }

    #[test]
    fn exits_for_quit_and_interrupt() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, viewport(80, 1, 0, 0));

        for event in [Event::Command(Command::Quit), Event::Interrupt] {
            let destination = Destination::new(
                start.target(&event),
                NonZeroUsize::MIN,
                bounds(0, 0),
                ScrollPosition::START,
            );
            assert_eq!(transition(&start, event, &destination), Transition::Exit);
        }
    }

    #[test]
    fn resize_moves_to_the_anchored_offset_clamped_for_the_new_size() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, viewport(40, 3, 2, 4));
        let resize = |anchored_vertical_offset| {
            let anchored = ScrollPosition {
                vertical: anchored_vertical_offset,
                ..start.viewport.scroll
            };
            let event = Event::Resize(Size::new(80, 9));
            after_starting_at(&start, event, NonZeroUsize::MIN, bounds(5, 3), anchored).viewport
        };

        assert_eq!(resize(4), viewport(80, 9, 4, 3));
        assert_eq!(resize(7).scroll.vertical, 5);
    }

    #[test]
    fn toggles_wrapping_at_the_anchored_offset() {
        let start = interaction(
            PageIndex::FIRST,
            DiffLayout::Vertical,
            viewport(80, 9, 2, 4),
        );
        let anchored = ScrollPosition {
            vertical: 11,
            ..start.viewport.scroll
        };

        let wrapped = after_starting_at(
            &start,
            preference(PreferenceChange::ToggleWrapping),
            NonZeroUsize::MIN,
            bounds(20, 0),
            anchored,
        );

        assert_eq!(wrapped.preferences.wrapping, Wrapping::Enabled);
        assert_eq!(wrapped.preferences.layout, DiffLayout::Vertical);
        assert_eq!(wrapped.viewport.scroll.vertical, 11);
        assert_eq!(wrapped.viewport.scroll.horizontal, 0);
    }

    #[test]
    fn rotates_files_and_resets_the_viewport() {
        let start = interaction(second_page(), DiffLayout::Unified, viewport(80, 3, 4, 3));

        let next = after_starting_at(
            &start,
            Event::Command(Command::Page(PageStep::Next)),
            TWO_PAGES,
            bounds(7, 12),
            ScrollPosition::START,
        );

        assert_eq!(
            next,
            interaction(PageIndex::FIRST, DiffLayout::Unified, viewport(80, 3, 0, 0))
        );
    }

    #[test]
    fn rotates_to_the_previous_file_and_resets_the_viewport() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, viewport(80, 3, 4, 3));

        let previous = after_starting_at(
            &start,
            Event::Command(Command::Page(PageStep::Previous)),
            TWO_PAGES,
            bounds(7, 12),
            ScrollPosition::START,
        );

        assert_eq!(
            previous,
            interaction(second_page(), DiffLayout::Unified, viewport(80, 3, 0, 0))
        );
    }

    #[test]
    fn cycles_layouts_without_changing_the_reader_position() {
        let start = interaction(second_page(), DiffLayout::Unified, viewport(80, 3, 4, 3));
        let next_layout = preference(PreferenceChange::NextLayout);
        let display = bounds(7, 12);

        let vertical = after(&start, next_layout, display);
        let stacked = after(&vertical, next_layout, display);
        let unified = after(&stacked, next_layout, display);

        assert_eq!(vertical.preferences.layout, DiffLayout::Vertical);
        assert_eq!(stacked.preferences.layout, DiffLayout::Stacked);
        assert_eq!(unified, start);
    }

    #[test]
    fn toggles_detail_and_bounds_context_without_moving_the_viewport() {
        let start = interaction(second_page(), DiffLayout::Unified, viewport(80, 3, 4, 3));
        let display = bounds(7, 12);

        let character = after(
            &start,
            preference(PreferenceChange::ToggleGranularity),
            display,
        );
        let one_more_context = after(
            &character,
            preference(PreferenceChange::IncreaseContext),
            display,
        );

        assert_eq!(
            character.preferences.granularity,
            DiffGranularity::Character
        );
        assert_eq!(one_more_context.preferences.context_lines, 4);
        assert_eq!(one_more_context.viewport, start.viewport);
    }

    /// architecture.md (Interaction model): while the keymap popup is open, `?`, `Esc`,
    /// and `q` close it without exiting, `Ctrl-C` exits, and every other key does nothing.
    #[test]
    fn admits_only_closing_keys_interrupts_and_resizes_while_the_keymap_is_open() {
        let open = KeymapPopup::Open;
        let close = Command::Keymap(KeymapPopup::Closed);
        let resize = Event::Resize(Size::new(80, 9));

        for closing in [Command::Keymap(KeymapPopup::Open), close, Command::Quit] {
            assert_eq!(open.admit(closing), Some(close));
        }
        for ignored in [
            Command::Movement(Movement::Down),
            Command::Page(PageStep::Next),
            Command::Preference(PreferenceChange::NextLayout),
            Command::Preference(PreferenceChange::ToggleFileList),
        ] {
            assert_eq!(open.admit(ignored), None);
        }
        assert_eq!(Event::Interrupt.admitted(open), Some(Event::Interrupt));
        assert_eq!(resize.admitted(open), Some(resize));
        assert_eq!(
            KeymapPopup::Closed.admit(Command::Quit),
            Some(Command::Quit)
        );
        assert_eq!(KeymapPopup::Closed.admit(close), None);
    }

    #[test]
    fn opens_and_closes_the_keymap_without_moving_the_viewport() {
        let start = interaction(second_page(), DiffLayout::Unified, viewport(80, 9, 4, 3));
        let display = bounds(7, 12);

        let open = after(
            &start,
            Event::Command(Command::Keymap(KeymapPopup::Open)),
            display,
        );
        let closed = after(
            &open,
            Event::Command(Command::Keymap(KeymapPopup::Closed)),
            display,
        );

        assert_eq!(open.keymap, KeymapPopup::Open);
        assert_eq!(open.viewport, start.viewport);
        assert_eq!(closed, start);
    }

    /// architecture.md (View model): showing or hiding the file list keeps the row on the
    /// diff pane's middle line on that line.
    #[test]
    fn toggles_the_file_list_at_the_anchored_offset() {
        let start = interaction(
            PageIndex::FIRST,
            DiffLayout::Vertical,
            viewport(80, 9, 2, 9),
        );
        let toggle = |interaction: &Interaction| {
            let anchored = ScrollPosition {
                vertical: 6,
                ..interaction.viewport.scroll
            };
            after_starting_at(
                interaction,
                preference(PreferenceChange::ToggleFileList),
                NonZeroUsize::MIN,
                bounds(20, 4),
                anchored,
            )
        };

        let hidden = toggle(&start);
        let shown = toggle(&hidden);

        assert_eq!(hidden.preferences.file_list, FileList::Hidden);
        assert_eq!(hidden.viewport.scroll.vertical, 6);
        assert_eq!(hidden.viewport.scroll.horizontal, 4);
        assert_eq!(shown.preferences.file_list, FileList::Shown);
    }
}
