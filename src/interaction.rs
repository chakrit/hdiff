//! Interaction state and what its events ask for. Rules: `docs/spec/architecture.md` (Core
//! model, Interaction model, View model).

use ratatui::layout::Size;

use crate::{
    preferences::{PreferenceChange, ViewPreferences},
    prepared::{PageCount, PageIndex},
    screen::{Cells, ScreenLines},
};

/// The selected page, the view preferences, the screen size, the scroll position, and the
/// keymap popup. Only [`Interaction::start`] and navigation build one, and navigation keeps
/// its scroll position within the bounds of what it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interaction {
    page: PageIndex,
    preferences: ViewPreferences,
    screen: Size,
    scroll: ScrollPosition,
    keymap: KeymapPopup,
}

impl Interaction {
    /// The first page of a document on a `screen`-sized terminal, scrolled to its start.
    pub fn start(preferences: ViewPreferences, screen: Size) -> Self {
        Self {
            page: PageIndex::FIRST,
            preferences,
            screen,
            scroll: ScrollPosition::START,
            keymap: KeymapPopup::Closed,
        }
    }

    pub fn page(&self) -> PageIndex {
        self.page
    }

    pub fn preferences(&self) -> ViewPreferences {
        self.preferences
    }

    pub fn screen(&self) -> Size {
        self.screen
    }

    pub fn scroll(&self) -> ScrollPosition {
        self.scroll
    }

    pub fn keymap(&self) -> KeymapPopup {
        self.keymap
    }

    /// What `event` asks of the session among `pages` pages.
    pub fn respond(&self, event: Event, pages: PageCount) -> Outcome {
        let effect = match event {
            Event::Interrupt | Event::Command(Command::Quit) => return Outcome::Exit,
            Event::Command(Command::Movement(movement)) => Effect::Scroll(movement),
            Event::Command(Command::Page(step)) => Effect::TurnPage(step.applied(self.page, pages)),
            Event::Command(Command::Keymap(command)) => Effect::Keymap(self.keymap.after(command)),
            Event::Command(Command::Preference(change)) => Effect::Redisplay {
                preferences: self.preferences.after(change),
                screen: self.screen,
                placement: placement_after(change),
            },
            Event::Resize(screen) => self.resize(screen),
        };

        Outcome::Effect(effect)
    }

    /// The same page shown on a `screen`-sized terminal, keeping the row on the diff pane's
    /// top line there.
    pub fn resize(&self, screen: Size) -> Effect {
        Effect::Redisplay {
            preferences: self.preferences,
            screen,
            placement: Placement::Anchor(AnchorLine::PaneTop),
        }
    }

    /// This interaction scrolled to `scroll`, which navigation keeps within bounds.
    pub(crate) fn scrolled(self, scroll: ScrollPosition) -> Self {
        Self { scroll, ..self }
    }

    /// `page` with `preferences` on a `screen`-sized terminal, scrolled to `scroll`, which
    /// navigation keeps within bounds.
    pub(crate) fn shown(
        self,
        page: PageIndex,
        preferences: ViewPreferences,
        screen: Size,
        scroll: ScrollPosition,
    ) -> Self {
        Self {
            page,
            preferences,
            screen,
            scroll,
            ..self
        }
    }

    pub(crate) fn with_keymap(self, keymap: KeymapPopup) -> Self {
        Self { keymap, ..self }
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

/// What an event asks of the session: to end it, or to change the interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Exit,
    Effect(Effect),
}

/// What an event does to the interaction, before navigation places the scroll position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Moves within what is shown now.
    Scroll(Movement),
    /// Shows another page from its start.
    TurnPage(PageIndex),
    /// Shows the same page with `preferences` on a `screen`-sized terminal.
    Redisplay {
        preferences: ViewPreferences,
        screen: Size,
        placement: Placement,
    },
    Keymap(KeymapPopup),
}

/// Where a redisplayed page places its first line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The current position, within the new bounds.
    Keep,
    /// The position that keeps the row on this diff-pane line on that line.
    Anchor(AnchorLine),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorLine {
    PaneTop,
    PaneMiddle,
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Size;

    use super::{
        AnchorLine, Command, Effect, Event, Interaction, KeymapCommand, KeymapPopup, Movement,
        Outcome, PageStep, Placement,
    };
    use crate::{
        preferences::{
            DiffGranularity, DiffLayout, FileList, PreferenceChange, ViewPreferences, Wrapping,
        },
        prepared::{PageCount, PageIndex},
    };

    const TWO_PAGES: PageCount = PageCount::of_test(2);
    const SCREEN: Size = Size::new(80, 9);

    fn second_page() -> PageIndex {
        PageIndex::FIRST.next(TWO_PAGES)
    }

    /// An interaction showing `page` in `layout` on an 80-by-9 screen.
    fn interaction(page: PageIndex, layout: DiffLayout) -> Interaction {
        let preferences = ViewPreferences {
            layout,
            ..ViewPreferences::INITIAL
        };

        Interaction {
            page,
            ..Interaction::start(preferences, SCREEN)
        }
    }

    fn command(command: Command) -> Event {
        Event::Command(command)
    }

    fn effect(interaction: &Interaction, event: Event) -> Effect {
        let Outcome::Effect(effect) = interaction.respond(event, TWO_PAGES) else {
            panic!("{event:?} changes the interaction");
        };
        effect
    }

    fn redisplay(
        interaction: &Interaction,
        change: PreferenceChange,
    ) -> (ViewPreferences, Placement) {
        let Effect::Redisplay {
            preferences,
            screen,
            placement,
        } = effect(interaction, command(Command::Preference(change)))
        else {
            panic!("{change:?} redisplays the page");
        };
        assert_eq!(screen, interaction.screen, "{change:?} keeps the screen");
        (preferences, placement)
    }

    #[test]
    fn exits_for_quit_and_interrupt() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified);

        for event in [command(Command::Quit), Event::Interrupt] {
            assert_eq!(start.respond(event, TWO_PAGES), Outcome::Exit, "{event:?}");
        }
    }

    #[test]
    fn resizes_at_the_pane_top_anchor() {
        let start = interaction(PageIndex::FIRST, DiffLayout::Unified);
        let screen = Size::new(80, 20);

        assert_eq!(
            effect(&start, Event::Resize(screen)),
            Effect::Redisplay {
                preferences: start.preferences,
                screen,
                placement: Placement::Anchor(AnchorLine::PaneTop),
            }
        );
    }

    /// architecture.md (View model): wrapping, context changes, and showing or hiding the
    /// file list keep the row on the diff pane's middle line on that line; cycling layouts
    /// and toggling detail keep the position.
    #[test]
    fn anchors_preference_changes_that_move_rows_on_the_middle_line() {
        let start = interaction(second_page(), DiffLayout::Vertical);
        let placement = |change| redisplay(&start, change).1;

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
        let start = interaction(second_page(), DiffLayout::Unified);
        let after = |change| redisplay(&start, change).0;

        assert_eq!(
            after(PreferenceChange::NextLayout).layout,
            DiffLayout::Vertical
        );
        assert_eq!(
            after(PreferenceChange::ToggleGranularity).granularity,
            DiffGranularity::Character
        );
        assert_eq!(
            after(PreferenceChange::ToggleWrapping).wrapping,
            Wrapping::Enabled
        );
        assert_eq!(after(PreferenceChange::IncreaseContext).context_lines, 4);
        assert_eq!(
            after(PreferenceChange::ToggleFileList).file_list,
            FileList::Hidden
        );
    }

    #[test]
    fn turns_pages_with_wrapping() {
        let first = interaction(PageIndex::FIRST, DiffLayout::Unified);
        let second = interaction(second_page(), DiffLayout::Unified);

        assert_eq!(
            effect(&second, command(Command::Page(PageStep::Next))),
            Effect::TurnPage(PageIndex::FIRST)
        );
        assert_eq!(
            effect(&first, command(Command::Page(PageStep::Previous))),
            Effect::TurnPage(second_page())
        );
    }

    /// architecture.md (Interaction model): while the keymap popup is open, `?`, `Esc`,
    /// and `q` close it without exiting, `Ctrl-C` exits, and every other key does nothing.
    #[test]
    fn admits_only_closing_keys_interrupts_and_resizes_while_the_keymap_is_open() {
        let open = KeymapPopup::Open;
        let close = Command::Keymap(KeymapCommand::Close);
        let resize = Event::Resize(SCREEN);

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
    fn toggles_and_closes_the_keymap() {
        let start = interaction(second_page(), DiffLayout::Unified);
        let after = |interaction: &Interaction, keymap| {
            let Effect::Keymap(popup) = effect(interaction, command(Command::Keymap(keymap)))
            else {
                panic!("a keymap command changes the popup");
            };
            interaction.with_keymap(popup)
        };

        let open = after(&start, KeymapCommand::Toggle);
        let toggled_closed = after(&open, KeymapCommand::Toggle);
        let closed = after(&open, KeymapCommand::Close);

        assert_eq!(open.keymap, KeymapPopup::Open);
        assert_eq!(toggled_closed, start);
        assert_eq!(closed, start);
    }
}
