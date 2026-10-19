//! Key bindings: the event each key stands for, grouped under the keymap popup's
//! headings. Rules: docs/spec/architecture.md#interaction-model and
//! docs/spec/architecture.md#keyboard-bindings.

use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    navigation::{
        interaction::{Command, Event, KeymapCommand, KeymapPopup, PageStep},
        scroll::Movement,
    },
    one_of::OneOf,
    preferences::PreferenceChange,
};

/// The session event a terminal event stands for, if any.
pub(super) fn event_for(event: TerminalEvent) -> Option<Event> {
    match event {
        TerminalEvent::Key(key) if key.kind == KeyEventKind::Press => {
            Key::of(key).and_then(event_bound_to)
        }
        TerminalEvent::Resize(..) => Some(Event::Resize),
        _ => None,
    }
}

/// The event `key` is bound to; none when no binding names it.
fn event_bound_to(key: Key) -> Option<Event> {
    all_bindings()
        .find(|binding| binding.key == key)
        .map(|binding| binding.action.event())
}

/// Every binding the keymap popup lists, in table order.
fn all_bindings() -> impl Iterator<Item = Binding> {
    GROUPS
        .iter()
        .flat_map(|group| group.shortcuts)
        .flat_map(Shortcut::bindings)
}

/// The first plain character, in table order, bound to `command`.
pub(crate) fn character_for(command: Command) -> Option<char> {
    all_bindings()
        .filter(|binding| binding.action.command() == Some(command))
        .find_map(|binding| binding.key.character())
}

/// The keys that close the open keymap popup, in table order: every key whose command the
/// open popup turns into closing it.
pub(super) fn keymap_closing_keys() -> impl Iterator<Item = Key> {
    all_bindings()
        .filter(|binding| binding.action.command().is_some_and(KeymapPopup::closes))
        .map(|binding| binding.key)
}

/// A key as the bindings name it: Shift folds into the character it types, a Control
/// chord of a character stays distinct, and Shift turns Tab into Shift-Tab. Any other
/// modifier makes a key no binding's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Key {
    Char(char),
    Control(char),
    Tab,
    ShiftTab,
    Esc,
    Down,
    Up,
}

impl Key {
    /// The key `key` presses as the bindings name it; none for any other modifier set.
    fn of(key: KeyEvent) -> Option<Self> {
        const NONE: KeyModifiers = KeyModifiers::NONE;
        const SHIFT: KeyModifiers = KeyModifiers::SHIFT;
        const CONTROL: KeyModifiers = KeyModifiers::CONTROL;

        match (key.code, key.modifiers) {
            (KeyCode::Char(character), NONE | SHIFT) => Some(Self::Char(character)),
            (KeyCode::Char(character), CONTROL) => Some(Self::Control(character)),
            (KeyCode::Tab, NONE) => Some(Self::Tab),
            (KeyCode::Tab | KeyCode::BackTab, SHIFT) | (KeyCode::BackTab, NONE) => {
                Some(Self::ShiftTab)
            }
            (KeyCode::Esc, NONE) => Some(Self::Esc),
            (KeyCode::Down, NONE) => Some(Self::Down),
            (KeyCode::Up, NONE) => Some(Self::Up),
            _ => None,
        }
    }

    /// The plain character this key types; a chord or a named key types none.
    fn character(self) -> Option<char> {
        match self {
            Self::Char(character) => Some(character),
            Self::Control(_) | Self::Tab | Self::ShiftTab | Self::Esc | Self::Down | Self::Up => {
                None
            }
        }
    }

    /// The key as the keymap popup shows it.
    pub(super) fn label(self) -> String {
        match self {
            Self::Char(character) => character.to_string(),
            Self::Control(character) => format!("^{}", character.to_ascii_uppercase()),
            Self::Tab => "Tab".to_owned(),
            Self::ShiftTab => "⇧Tab".to_owned(),
            Self::Esc => "Esc".to_owned(),
            Self::Down => "↓".to_owned(),
            Self::Up => "↑".to_owned(),
        }
    }
}

/// A key and the action it takes.
#[derive(Clone, Copy)]
pub(super) struct Binding {
    pub(super) key: Key,
    action: KeyAction,
}

/// What a key can stand for: a command, or the interrupt. A resize is never a key.
#[derive(Clone, Copy)]
enum KeyAction {
    Command(Command),
    Interrupt,
}

impl KeyAction {
    /// The command this action takes; the interrupt is none.
    const fn command(self) -> Option<Command> {
        match self {
            Self::Command(command) => Some(command),
            Self::Interrupt => None,
        }
    }

    /// The session event this action sends.
    const fn event(self) -> Event {
        match self {
            Self::Command(command) => Event::Command(command),
            Self::Interrupt => Event::Interrupt,
        }
    }
}

/// One row of the popup: its keys and the action they take.
pub(super) enum Shortcut {
    /// Keys listed side by side on one line, the action after them.
    Keys {
        bindings: &'static [Binding],
        action: &'static str,
    },
    /// Four keys spread over three lines, the action beside the middle one.
    Diamond {
        up: Binding,
        left: Binding,
        right: Binding,
        down: Binding,
        action: &'static str,
    },
    /// The keys that close the open keymap popup: `bindings`, and the keys bound
    /// elsewhere that close it while it is open.
    ClosesKeymap {
        bindings: &'static [Binding],
        action: &'static str,
    },
}

impl Shortcut {
    /// The bindings this row makes.
    fn bindings(&self) -> impl Iterator<Item = Binding> {
        match self {
            Self::Keys { bindings, .. } | Self::ClosesKeymap { bindings, .. } => {
                OneOf::First(bindings.iter().copied())
            }
            Self::Diamond {
                up,
                left,
                right,
                down,
                ..
            } => OneOf::Second([*up, *left, *right, *down].into_iter()),
        }
    }
}

/// A heading in the keymap popup and the shortcuts listed under it.
pub(super) struct ShortcutGroup {
    pub(super) heading: &'static str,
    pub(super) shortcuts: &'static [Shortcut],
}

/// `key` bound to `command`.
const fn binds(key: Key, command: Command) -> Binding {
    Binding {
        key,
        action: KeyAction::Command(command),
    }
}

/// `key` bound to scrolling in `direction`.
const fn movement(key: Key, direction: Movement) -> Binding {
    binds(key, Command::Movement(direction))
}

/// `key` bound to the preference `change`.
const fn preference(key: Key, change: PreferenceChange) -> Binding {
    binds(key, Command::Preference(change))
}

/// The shortcuts the keymap popup lists, under their headings, in its order.
pub(super) const GROUPS: [ShortcutGroup; 4] = [
    ShortcutGroup {
        heading: "Scroll",
        shortcuts: &[
            Shortcut::Diamond {
                up: movement(Key::Char('k'), Movement::Up),
                left: movement(Key::Char('h'), Movement::Left),
                right: movement(Key::Char('l'), Movement::Right),
                down: movement(Key::Char('j'), Movement::Down),
                action: "scroll",
            },
            Shortcut::Keys {
                bindings: &[
                    movement(Key::Down, Movement::Down),
                    movement(Key::Up, Movement::Up),
                ],
                action: "down / up",
            },
            Shortcut::Keys {
                bindings: &[
                    movement(Key::Control('d'), Movement::HalfPageDown),
                    movement(Key::Control('u'), Movement::HalfPageUp),
                ],
                action: "half page down / up",
            },
            Shortcut::Keys {
                bindings: &[
                    movement(Key::Char('g'), Movement::Top),
                    movement(Key::Char('G'), Movement::Bottom),
                ],
                action: "top / bottom",
            },
            Shortcut::Keys {
                bindings: &[
                    movement(Key::Char('0'), Movement::FarLeft),
                    movement(Key::Char('^'), Movement::FarLeft),
                ],
                action: "far left",
            },
            Shortcut::Keys {
                bindings: &[movement(Key::Char('$'), Movement::FarRight)],
                action: "far right",
            },
        ],
    },
    ShortcutGroup {
        heading: "Files",
        shortcuts: &[
            Shortcut::Keys {
                bindings: &[
                    binds(Key::Tab, Command::Page(PageStep::Next)),
                    binds(Key::ShiftTab, Command::Page(PageStep::Previous)),
                ],
                action: "next / previous file",
            },
            Shortcut::Keys {
                bindings: &[preference(Key::Char('e'), PreferenceChange::ToggleFileList)],
                action: "show / hide file list",
            },
        ],
    },
    ShortcutGroup {
        heading: "View",
        shortcuts: &[
            Shortcut::Keys {
                bindings: &[preference(Key::Char('v'), PreferenceChange::NextLayout)],
                action: "cycle layout",
            },
            Shortcut::Keys {
                bindings: &[preference(
                    Key::Char('c'),
                    PreferenceChange::ToggleGranularity,
                )],
                action: "character detail",
            },
            Shortcut::Keys {
                bindings: &[preference(Key::Char('w'), PreferenceChange::ToggleWrapping)],
                action: "wrap",
            },
            Shortcut::Keys {
                bindings: &[
                    preference(Key::Char('+'), PreferenceChange::IncreaseContext),
                    preference(Key::Char('-'), PreferenceChange::DecreaseContext),
                ],
                action: "more / less context",
            },
        ],
    },
    ShortcutGroup {
        heading: "Keymap and exit",
        shortcuts: &[
            Shortcut::Keys {
                bindings: &[binds(
                    Key::Char('?'),
                    Command::Keymap(KeymapCommand::Toggle),
                )],
                action: "open keymap",
            },
            Shortcut::ClosesKeymap {
                bindings: &[binds(Key::Esc, Command::Keymap(KeymapCommand::Close))],
                action: "close keymap",
            },
            Shortcut::Keys {
                bindings: &[
                    binds(Key::Char('q'), Command::Quit),
                    Binding {
                        key: Key::Control('c'),
                        action: KeyAction::Interrupt,
                    },
                ],
                action: "exit",
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};

    use super::{Key, all_bindings, event_for, keymap_closing_keys};
    use crate::{
        navigation::{
            interaction::{Command, Event, KeymapCommand, PageStep},
            scroll::Movement,
        },
        preferences::PreferenceChange,
    };

    /// The event a press of `code` with `modifiers` is bound to, if any.
    fn bound_event(code: KeyCode, modifiers: KeyModifiers) -> Option<Event> {
        event_for(TerminalEvent::Key(KeyEvent::new(code, modifiers)))
    }

    /// The lookup result of a key bound to `command`.
    fn bound_to(command: Command) -> Option<Event> {
        Some(Event::Command(command))
    }

    #[test]
    fn maps_pager_keys_and_resize_events() {
        let plain = |character| bound_event(KeyCode::Char(character), KeyModifiers::NONE);

        assert_eq!(plain('j'), bound_to(Command::Movement(Movement::Down)));
        for (character, expected) in [
            ('h', Movement::Left),
            ('l', Movement::Right),
            ('0', Movement::FarLeft),
            ('^', Movement::FarLeft),
            ('$', Movement::FarRight),
        ] {
            assert_eq!(
                plain(character),
                bound_to(Command::Movement(expected)),
                "{character} horizontal movement"
            );
        }
        assert_eq!(
            bound_event(KeyCode::Tab, KeyModifiers::NONE),
            bound_to(Command::Page(PageStep::Next))
        );
        assert_eq!(
            plain('v'),
            bound_to(Command::Preference(PreferenceChange::NextLayout))
        );
        assert_eq!(
            plain('+'),
            bound_to(Command::Preference(PreferenceChange::IncreaseContext))
        );
        assert_eq!(
            plain('-'),
            bound_to(Command::Preference(PreferenceChange::DecreaseContext))
        );
        assert_eq!(
            bound_event(KeyCode::BackTab, KeyModifiers::SHIFT),
            bound_to(Command::Page(PageStep::Previous))
        );
        assert_eq!(
            bound_event(KeyCode::Tab, KeyModifiers::SHIFT),
            bound_to(Command::Page(PageStep::Previous))
        );
        for (code, expected) in [
            (
                KeyCode::Char('e'),
                Command::Preference(PreferenceChange::ToggleFileList),
            ),
            (KeyCode::Char('?'), Command::Keymap(KeymapCommand::Toggle)),
            (KeyCode::Esc, Command::Keymap(KeymapCommand::Close)),
        ] {
            assert_eq!(
                bound_event(code, KeyModifiers::NONE),
                bound_to(expected),
                "{code:?}"
            );
        }
        assert_eq!(
            event_for(TerminalEvent::Resize(100, 30)),
            Some(Event::Resize)
        );
    }

    /// docs/spec/architecture.md#keyboard-bindings: `Ctrl-C` exits as an interrupt, which
    /// applies in every mode.
    #[test]
    fn maps_control_c_to_an_interrupt() {
        assert_eq!(
            bound_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Some(Event::Interrupt)
        );
    }

    /// docs/spec/architecture.md#keyboard-bindings: the table names every bound key, so
    /// Shift-`G` is bound, and `{`, `}`, `Ctrl-Q`, `Ctrl-J` and `Alt-q` are not.
    #[test]
    fn binds_only_the_keys_the_table_names() {
        assert_eq!(
            bound_event(KeyCode::Char('G'), KeyModifiers::SHIFT),
            bound_to(Command::Movement(Movement::Bottom))
        );
        for (code, modifiers) in [
            (KeyCode::Char('{'), KeyModifiers::NONE),
            (KeyCode::Char('}'), KeyModifiers::NONE),
            (KeyCode::Char('q'), KeyModifiers::CONTROL),
            (KeyCode::Char('j'), KeyModifiers::CONTROL),
            (KeyCode::Char('q'), KeyModifiers::ALT),
            (
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            (KeyCode::Char('j'), KeyModifiers::SUPER),
            (KeyCode::Tab, KeyModifiers::CONTROL),
            (KeyCode::BackTab, KeyModifiers::SHIFT | KeyModifiers::ALT),
            (KeyCode::Esc, KeyModifiers::ALT),
            (KeyCode::Down, KeyModifiers::CONTROL),
            (KeyCode::Up, KeyModifiers::ALT),
        ] {
            assert_eq!(
                bound_event(code, modifiers),
                None,
                "{code:?} with {modifiers:?}"
            );
        }
    }

    /// docs/spec/architecture.md#keyboard-bindings: each key has one binding.
    #[test]
    fn binds_each_key_once() {
        let keys = all_bindings()
            .map(|binding| binding.key)
            .collect::<Vec<_>>();

        for key in &keys {
            let count = keys.iter().filter(|bound| *bound == key).count();
            assert_eq!(count, 1, "{key:?}");
        }
    }

    /// docs/spec/architecture.md#keyboard-bindings: `?`, `Esc`, and `q` close the keymap
    /// popup.
    #[test]
    fn lists_the_keys_that_close_the_open_keymap() {
        assert_eq!(
            keymap_closing_keys().collect::<Vec<_>>(),
            [Key::Char('?'), Key::Esc, Key::Char('q')]
        );
    }
}
