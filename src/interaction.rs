//! Interaction state and its transitions. Rules: `docs/spec/architecture.md` (Core model,
//! Interaction model, View model).

use ratatui::layout::Size;

use crate::{
    geometry::{PaneContent, PaneShape, ScreenGeometry},
    preferences::{PreferenceChange, ViewPreferences},
    prepared::{PageCount, PageIndex},
    screen::{Cells, ScreenLines},
};

/// The selected page, what the screen shows, the scroll position, and the keymap popup.
/// Only [`Interaction::start`] and transitions build one, so its scroll position is
/// always within its display's bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interaction {
    page: PageIndex,
    display: Display,
    scroll: ScrollPosition,
    keymap: KeymapPopup,
}

impl Interaction {
    /// The first page of a document on a `screen`-sized terminal, scrolled to its start.
    pub fn start(preferences: ViewPreferences, screen: Size) -> Self {
        Self {
            page: PageIndex::FIRST,
            display: Display::new(preferences, screen),
            scroll: ScrollPosition::START,
            keymap: KeymapPopup::Closed,
        }
    }

    pub fn page(&self) -> PageIndex {
        self.page
    }

    pub fn display(&self) -> &Display {
        &self.display
    }

    pub fn preferences(&self) -> ViewPreferences {
        self.display.preferences
    }

    pub fn scroll(&self) -> ScrollPosition {
        self.scroll
    }

    pub fn keymap(&self) -> KeymapPopup {
        self.keymap
    }

    /// What `event` does among `pages` pages, before navigation places the scroll position.
    pub fn effect(&self, event: Event, pages: PageCount) -> Effect {
        match event {
            Event::Interrupt | Event::Command(Command::Quit) => Effect::Exit,
            Event::Command(Command::Movement(movement)) => Effect::Scroll(movement),
            Event::Command(Command::Page(step)) => Effect::TurnPage(step.applied(self.page, pages)),
            Event::Command(Command::Keymap(command)) => Effect::Keymap(self.keymap.after(command)),
            Event::Command(Command::Preference(change)) => Effect::Redisplay {
                display: self
                    .display
                    .with_preferences(self.display.preferences.after(change)),
                placement: placement_after(change),
            },
            Event::Resize(screen) => Effect::Redisplay {
                display: self.display.resized(screen),
                placement: Placement::Anchor(AnchorLine::PaneTop),
            },
        }
    }
}

/// Where the reader stays after a preference change. Rules: `docs/spec/architecture.md`
/// (View model).
const fn placement_after(change: PreferenceChange) -> Placement {
    match change {
        PreferenceChange::ToggleWrapping
        | PreferenceChange::IncreaseContext
        | PreferenceChange::DecreaseContext
        | PreferenceChange::ToggleFileList => Placement::Anchor(AnchorLine::PaneMiddle),
        PreferenceChange::NextLayout | PreferenceChange::ToggleGranularity => Placement::Keep,
    }
}

/// The view preferences laid out on a screen: the screen's geometry and the content each
/// diff pane of the preferred layout shows, derived once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Display {
    preferences: ViewPreferences,
    geometry: ScreenGeometry,
    pane_content: PaneContent,
}

impl Display {
    pub fn new(preferences: ViewPreferences, screen: Size) -> Self {
        let geometry = ScreenGeometry::of(screen, preferences.file_list);

        Self {
            preferences,
            geometry,
            pane_content: geometry.pane_content(PaneShape::of(preferences.layout)),
        }
    }

    pub fn preferences(&self) -> ViewPreferences {
        self.preferences
    }

    pub fn geometry(&self) -> &ScreenGeometry {
        &self.geometry
    }

    /// The pane shape of the preferred layout.
    pub fn shape(&self) -> PaneShape {
        PaneShape::of(self.preferences.layout)
    }

    /// The content each diff pane of the preferred layout shows.
    pub fn pane_content(&self) -> PaneContent {
        self.pane_content
    }

    fn with_preferences(&self, preferences: ViewPreferences) -> Self {
        Self::new(preferences, self.geometry.screen().as_size())
    }

    fn resized(&self, screen: Size) -> Self {
        Self::new(self.preferences, screen)
    }
}

/// The first visible screen line and the synchronized horizontal offset. Rules:
/// `docs/spec/architecture.md` (Viewport coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollPosition {
    pub vertical: ScreenLines,
    pub horizontal: Cells,
}

impl ScrollPosition {
    pub const START: Self = Self {
        vertical: ScreenLines::ZERO,
        horizontal: Cells::ZERO,
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
    /// it; while closed, there is nothing to close.
    pub fn admit(self, command: Command) -> Option<Command> {
        match (self, command) {
            (Self::Closed, Command::Keymap(KeymapCommand::Close)) => None,
            (Self::Closed, command) => Some(command),
            (Self::Open, Command::Keymap(_) | Command::Quit) => {
                Some(Command::Keymap(KeymapCommand::Close))
            }
            (Self::Open, Command::Movement(_) | Command::Page(_) | Command::Preference(_)) => None,
        }
    }

    const fn after(self, command: KeymapCommand) -> Self {
        match (command, self) {
            (KeymapCommand::Toggle, Self::Closed) => Self::Open,
            (KeymapCommand::Toggle, Self::Open) | (KeymapCommand::Close, _) => Self::Closed,
        }
    }
}

/// What a keymap key asks of the popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeymapCommand {
    Toggle,
    Close,
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
}

/// What a bound key asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Movement(Movement),
    Page(PageStep),
    Preference(PreferenceChange),
    Keymap(KeymapCommand),
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
    /// `scroll` after this movement, within `bounds`.
    fn applied(self, scroll: ScrollPosition, bounds: DisplayBounds) -> ScrollPosition {
        let horizontal = |horizontal| ScrollPosition {
            horizontal,
            ..scroll
        };
        let vertical = |vertical| ScrollPosition { vertical, ..scroll };
        let one_cell = Cells::new(1);
        let one_line = ScreenLines::new(1);
        let half_page = bounds.pane_lines.half().max(one_line);

        let moved = match self {
            Self::Left => horizontal(scroll.horizontal.saturating_sub(one_cell)),
            Self::Right => horizontal(scroll.horizontal.saturating_add(one_cell)),
            Self::FarLeft => horizontal(Cells::ZERO),
            Self::FarRight => horizontal(bounds.maximum_horizontal_offset),
            Self::Down => vertical(scroll.vertical.saturating_add(one_line)),
            Self::Up => vertical(scroll.vertical.saturating_sub(one_line)),
            Self::HalfPageDown => vertical(scroll.vertical.saturating_add(half_page)),
            Self::HalfPageUp => vertical(scroll.vertical.saturating_sub(half_page)),
            Self::Top => vertical(ScreenLines::ZERO),
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
    fn applied(self, page: PageIndex, pages: PageCount) -> PageIndex {
        match self {
            Self::Next => page.next(pages),
            Self::Previous => page.previous(pages),
        }
    }
}

/// What an event does to the interaction, before navigation places the scroll position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Exit,
    /// Moves within the display shown now.
    Scroll(Movement),
    /// Shows another page from its start.
    TurnPage(PageIndex),
    /// Shows the same page as another display.
    Redisplay {
        display: Display,
        placement: Placement,
    },
    Keymap(KeymapPopup),
}

/// Where a redisplayed page places its first line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The current position, within the new display's bounds.
    Keep,
    /// The position that keeps the row on this diff-pane line on that line.
    Anchor(AnchorLine),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorLine {
    PaneTop,
    PaneMiddle,
}

/// The limits of a display's scroll position, and the lines one pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayBounds {
    pub maximum_vertical_offset: ScreenLines,
    pub maximum_horizontal_offset: Cells,
    pub pane_lines: ScreenLines,
}

/// The interaction an event leads to, resolved by navigation against its display's bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Destination(Resolved);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolved {
    Exit,
    Scroll {
        movement: Movement,
        bounds: DisplayBounds,
    },
    Show {
        page: PageIndex,
        display: Display,
        start: ScrollPosition,
    },
    Keymap(KeymapPopup),
}

impl Destination {
    pub const EXIT: Self = Self(Resolved::Exit);

    /// `movement` within the shown display's `bounds`.
    pub fn scroll(movement: Movement, bounds: DisplayBounds) -> Self {
        Self(Resolved::Scroll { movement, bounds })
    }

    /// `page` as `display`, from `start` moved within `bounds`.
    pub fn show(
        page: PageIndex,
        display: Display,
        start: ScrollPosition,
        bounds: DisplayBounds,
    ) -> Self {
        Self(Resolved::Show {
            page,
            display,
            start: start.clamped(bounds),
        })
    }

    pub fn keymap(keymap: KeymapPopup) -> Self {
        Self(Resolved::Keymap(keymap))
    }

    /// The interaction after `interaction` reaches this destination.
    pub fn transition(self, interaction: &Interaction) -> Transition {
        let next = match self.0 {
            Resolved::Exit => return Transition::Exit,
            Resolved::Scroll { movement, bounds } => Interaction {
                scroll: movement.applied(interaction.scroll, bounds),
                ..*interaction
            },
            Resolved::Show {
                page,
                display,
                start,
            } => Interaction {
                page,
                display,
                scroll: start,
                ..*interaction
            },
            Resolved::Keymap(keymap) => Interaction {
                keymap,
                ..*interaction
            },
        };

        Transition::Redraw(next)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Transition {
    Redraw(Interaction),
    Exit,
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Size;

    use super::{
        AnchorLine, Command, Destination, Display, DisplayBounds, Effect, Event, Interaction,
        KeymapCommand, KeymapPopup, Movement, PageStep, Placement, ScrollPosition, Transition,
    };
    use crate::{
        preferences::{
            DiffGranularity, DiffLayout, FileList, PreferenceChange, ViewPreferences, Wrapping,
        },
        prepared::{PageCount, PageIndex},
        screen::{Cells, ScreenLines},
    };

    const TWO_PAGES: PageCount = PageCount::of_test(2);

    fn second_page() -> PageIndex {
        PageIndex::FIRST.next(TWO_PAGES)
    }

    fn scroll(vertical: usize, horizontal: usize) -> ScrollPosition {
        ScrollPosition {
            vertical: ScreenLines::new(vertical),
            horizontal: Cells::new(horizontal),
        }
    }

    /// An interaction scrolled to `position` on an 80-by-9 screen.
    fn interaction(page: PageIndex, layout: DiffLayout, position: ScrollPosition) -> Interaction {
        let preferences = ViewPreferences {
            layout,
            ..ViewPreferences::INITIAL
        };

        Interaction {
            page,
            scroll: position,
            ..Interaction::start(preferences, Size::new(80, 9))
        }
    }

    fn bounds(maximum_vertical_offset: usize, maximum_horizontal_offset: usize) -> DisplayBounds {
        DisplayBounds {
            maximum_vertical_offset: ScreenLines::new(maximum_vertical_offset),
            maximum_horizontal_offset: Cells::new(maximum_horizontal_offset),
            pane_lines: ScreenLines::new(8),
        }
    }

    fn redrawn(transition: Transition) -> Interaction {
        let Transition::Redraw(next) = transition else {
            panic!("the event redraws the interaction");
        };
        next
    }

    fn moved(start: &Interaction, movement: Movement, bounds: DisplayBounds) -> Interaction {
        redrawn(Destination::scroll(movement, bounds).transition(start))
    }

    fn command(command: Command) -> Event {
        Event::Command(command)
    }

    fn preference(change: PreferenceChange) -> Event {
        command(Command::Preference(change))
    }

    #[test]
    fn moves_and_clamps_the_vertical_viewport() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, scroll(4, 2));

        let down = moved(&start, Movement::Down, bounds(5, 4));
        let clamped = moved(&down, Movement::Down, bounds(5, 4));

        assert_eq!(down.scroll, scroll(5, 2));
        assert_eq!(clamped.scroll, scroll(5, 2));
    }

    #[test]
    fn moves_and_clamps_the_horizontal_viewport() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, scroll(4, 2));
        let display = bounds(7, 3);

        let right = moved(&start, Movement::Right, display);
        let clamped = moved(&right, Movement::Right, display);
        let left = moved(&clamped, Movement::Left, display);

        assert_eq!(right.scroll, scroll(4, 3));
        assert_eq!(clamped.scroll, scroll(4, 3));
        assert_eq!(left.scroll, scroll(4, 2));
    }

    #[test]
    fn jumps_to_horizontal_boundaries() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, scroll(4, 2));
        let display = bounds(7, 12);

        assert_eq!(
            moved(&start, Movement::FarLeft, display).scroll,
            scroll(4, 0)
        );
        assert_eq!(
            moved(&start, Movement::FarRight, display).scroll,
            scroll(4, 12)
        );
    }

    /// architecture.md (Keyboard bindings): half-page movement moves half of a pane's lines.
    #[test]
    fn moves_half_a_pane_by_half_page() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, scroll(4, 0));
        let display = DisplayBounds {
            pane_lines: ScreenLines::new(15),
            ..bounds(100, 0)
        };

        let down = moved(&start, Movement::HalfPageDown, display);
        let up = moved(&down, Movement::HalfPageUp, display);

        assert_eq!(down.scroll, scroll(11, 0));
        assert_eq!(up.scroll, scroll(4, 0));
    }

    #[test]
    fn exits_for_quit_and_interrupt() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, ScrollPosition::START);

        for event in [command(Command::Quit), Event::Interrupt] {
            assert_eq!(start.effect(event, TWO_PAGES), Effect::Exit);
        }
        assert_eq!(Destination::EXIT.transition(&start), Transition::Exit);
    }

    #[test]
    fn resizes_at_the_pane_top_anchor_clamped_for_the_new_size() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified, scroll(2, 4));
        let screen = Size::new(80, 20);

        let Effect::Redisplay { display, placement } =
            start.effect(Event::Resize(screen), TWO_PAGES)
        else {
            panic!("a resize redisplays the page");
        };
        let resized = |anchored| {
            let destination = Destination::show(start.page, display, anchored, bounds(5, 3));
            redrawn(destination.transition(&start))
        };

        assert_eq!(placement, Placement::Anchor(AnchorLine::PaneTop));
        assert_eq!(display, Display::new(start.preferences(), screen));
        assert_eq!(resized(scroll(4, 4)).scroll, scroll(4, 3));
        assert_eq!(resized(scroll(7, 4)).scroll, scroll(5, 3));
    }

    /// architecture.md (View model): wrapping, context changes, and showing or hiding the
    /// file list keep the row on the diff pane's middle line on that line; cycling layouts
    /// and toggling detail keep the position.
    #[test]
    fn anchors_preference_changes_that_move_rows_on_the_middle_line() {
        let start = interaction(second_page(), DiffLayout::Vertical, scroll(2, 4));
        let placement = |change| {
            let Effect::Redisplay { placement, .. } = start.effect(preference(change), TWO_PAGES)
            else {
                panic!("{change:?} redisplays the page");
            };
            placement
        };

        for change in [
            PreferenceChange::ToggleWrapping,
            PreferenceChange::IncreaseContext,
            PreferenceChange::DecreaseContext,
            PreferenceChange::ToggleFileList,
        ] {
            assert_eq!(
                placement(change),
                Placement::Anchor(AnchorLine::PaneMiddle),
                "{change:?}"
            );
        }
        for change in [
            PreferenceChange::NextLayout,
            PreferenceChange::ToggleGranularity,
        ] {
            assert_eq!(placement(change), Placement::Keep, "{change:?}");
        }
    }

    #[test]
    fn redisplays_the_same_page_with_the_changed_preference() {
        let start = interaction(second_page(), DiffLayout::Unified, scroll(4, 3));
        let after = |interaction: &Interaction, change| {
            let Effect::Redisplay { display, .. } =
                interaction.effect(preference(change), TWO_PAGES)
            else {
                panic!("{change:?} redisplays the page");
            };
            let destination =
                Destination::show(interaction.page, display, interaction.scroll, bounds(7, 12));
            redrawn(destination.transition(interaction))
        };

        let vertical = after(&start, PreferenceChange::NextLayout);
        let stacked = after(&vertical, PreferenceChange::NextLayout);
        let unified = after(&stacked, PreferenceChange::NextLayout);
        let character = after(&start, PreferenceChange::ToggleGranularity);
        let wrapped = after(&start, PreferenceChange::ToggleWrapping);
        let more_context = after(&start, PreferenceChange::IncreaseContext);
        let hidden = after(&start, PreferenceChange::ToggleFileList);

        assert_eq!(vertical.preferences().layout, DiffLayout::Vertical);
        assert_eq!(stacked.preferences().layout, DiffLayout::Stacked);
        assert_eq!(unified, start, "three layout changes return to the start");
        assert_eq!(
            character.preferences().granularity,
            DiffGranularity::Character
        );
        assert_eq!(wrapped.preferences().wrapping, Wrapping::Enabled);
        assert_eq!(more_context.preferences().context_lines, 4);
        assert_eq!(hidden.preferences().file_list, FileList::Hidden);
        assert_eq!(character.page, start.page);
        assert_eq!(character.scroll, start.scroll);
    }

    #[test]
    fn turns_pages_with_wrapping() {
        let first = interaction(PageIndex::FIRST, DiffLayout::Unified, scroll(4, 3));
        let second = interaction(second_page(), DiffLayout::Unified, scroll(4, 3));

        assert_eq!(
            second.effect(command(Command::Page(PageStep::Next)), TWO_PAGES),
            Effect::TurnPage(PageIndex::FIRST)
        );
        assert_eq!(
            first.effect(command(Command::Page(PageStep::Previous)), TWO_PAGES),
            Effect::TurnPage(second_page())
        );
    }

    #[test]
    fn shows_a_turned_page_from_its_start() {
        let start = interaction(second_page(), DiffLayout::Unified, scroll(4, 3));

        let next = redrawn(
            Destination::show(
                PageIndex::FIRST,
                start.display,
                ScrollPosition::START,
                bounds(7, 12),
            )
            .transition(&start),
        );

        assert_eq!(
            next,
            interaction(PageIndex::FIRST, DiffLayout::Unified, ScrollPosition::START)
        );
    }

    /// architecture.md (Interaction model): while the keymap popup is open, `?`, `Esc`,
    /// and `q` close it without exiting, `Ctrl-C` exits, and every other key does nothing.
    #[test]
    fn admits_only_closing_keys_interrupts_and_resizes_while_the_keymap_is_open() {
        let open = KeymapPopup::Open;
        let close = Command::Keymap(KeymapCommand::Close);
        let resize = Event::Resize(Size::new(80, 9));

        for closing in [Command::Keymap(KeymapCommand::Toggle), close, Command::Quit] {
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
    fn toggles_and_closes_the_keymap_without_moving_the_viewport() {
        let start = interaction(second_page(), DiffLayout::Unified, scroll(4, 3));
        let after = |interaction: &Interaction, keymap| {
            let Effect::Keymap(popup) =
                interaction.effect(command(Command::Keymap(keymap)), TWO_PAGES)
            else {
                panic!("a keymap command changes the popup");
            };
            redrawn(Destination::keymap(popup).transition(interaction))
        };

        let open = after(&start, KeymapCommand::Toggle);
        let toggled_closed = after(&open, KeymapCommand::Toggle);
        let closed = after(&open, KeymapCommand::Close);

        assert_eq!(open.keymap, KeymapPopup::Open);
        assert_eq!(open.scroll, start.scroll);
        assert_eq!(toggled_closed, start);
        assert_eq!(closed, start);
    }
}
