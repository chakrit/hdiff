//! Key bindings: the event each key stands for, grouped under the keymap popup's headings.
//! Rules: `docs/spec/architecture.md` (Interaction model, Keyboard bindings).

use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    interaction::{Command, Event, KeymapCommand, KeymapPopup, PageStep},
    preferences::PreferenceChange,
    scroll::Movement,
};

/// The session event a terminal event stands for, if any.
pub(super) fn event_for(event: TerminalEvent) -> Option<Event> {
    match event {
        TerminalEvent::Key(key) if key.kind == KeyEventKind::Press => event_for_key(key),
        TerminalEvent::Resize(..) => Some(Event::Resize),
        _ => None,
    }
}

/// A Control chord without a binding of its own acts as its plain key.
fn event_for_key(key: KeyEvent) -> Option<Event> {
    let key = Key::of(key)?;
    let chord = event_bound_to(key);
    let plain = || key.without_control().and_then(event_bound_to);

    chord.or_else(plain)
}

fn event_bound_to(key: Key) -> Option<Event> {
    all_bindings()
        .find(|binding| binding.key == key)
        .map(|binding| binding.action.event())
}

fn all_bindings() -> impl Iterator<Item = Binding> {
    GROUPS
        .iter()
        .flat_map(|group| group.shortcuts)
        .flat_map(Shortcut::bindings)
}

/// The first plain character, in table order, bound to `command`.
pub(crate) fn character_for(command: Command) -> Option<char> {
    all_bindings().find_map(|binding| match (binding.key, binding.action) {
        (Key::Char(character), KeyAction::Command(bound)) if bound == command => Some(character),
        _ => None,
    })
}

/// The keys that close the open keymap popup, in table order: every key whose command the
/// open popup turns into closing it.
pub(super) fn keymap_closing_keys() -> impl Iterator<Item = Key> {
    let close = Command::Keymap(KeymapCommand::Close);

    all_bindings()
        .filter(move |binding| match binding.action {
            KeyAction::Command(command) => KeymapPopup::Open.admit(command) == Some(close),
            KeyAction::Interrupt => false,
        })
        .map(|binding| binding.key)
}

/// A key as the bindings name it: Shift folds into the character it types, and only a
/// Control chord of a character stays distinct.
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
    fn of(key: KeyEvent) -> Option<Self> {
        match (key.code, key.modifiers) {
            (KeyCode::Char(character), KeyModifiers::CONTROL) => Some(Self::Control(character)),
            (KeyCode::Char(character), _) => Some(Self::Char(character)),
            (KeyCode::Tab, KeyModifiers::SHIFT) | (KeyCode::BackTab, _) => Some(Self::ShiftTab),
            (KeyCode::Tab, _) => Some(Self::Tab),
            (KeyCode::Esc, _) => Some(Self::Esc),
            (KeyCode::Down, _) => Some(Self::Down),
            (KeyCode::Up, _) => Some(Self::Up),
            _ => None,
        }
    }

    fn without_control(self) -> Option<Self> {
        match self {
            Self::Control(character) => Some(Self::Char(character)),
            Self::Char(_) | Self::Tab | Self::ShiftTab | Self::Esc | Self::Down | Self::Up => None,
        }
    }

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
#[derive(Debug, Clone, Copy)]
pub(super) struct Binding {
    pub(super) key: Key,
    action: KeyAction,
}

/// What a key can stand for: a command, or the interrupt. A resize is never a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    Command(Command),
    Interrupt,
}

impl KeyAction {
    const fn event(self) -> Event {
        match self {
            Self::Command(command) => Event::Command(command),
            Self::Interrupt => Event::Interrupt,
        }
    }
}

/// One row of the popup: its keys and the action they take.
pub(super) enum Shortcut {
    Keys {
        keys: &'static [Binding],
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
    /// The keys that close the open keymap popup: `bound`, and the keys bound elsewhere that
    /// close it while it is open.
    ClosesKeymap {
        bound: &'static [Binding],
        action: &'static str,
    },
}

impl Shortcut {
    /// The bindings this row makes.
    fn bindings(&self) -> ShortcutBindings {
        match self {
            Self::Keys { keys, .. } | Self::ClosesKeymap { bound: keys, .. } => {
                ShortcutBindings::Keys(keys.iter())
            }
            Self::Diamond {
                up,
                left,
                right,
                down,
                ..
            } => ShortcutBindings::Diamond([*up, *left, *right, *down].into_iter()),
        }
    }
}

/// The bindings of one shortcut row, in row order.
enum ShortcutBindings {
    Keys(std::slice::Iter<'static, Binding>),
    Diamond(std::array::IntoIter<Binding, 4>),
}

impl Iterator for ShortcutBindings {
    type Item = Binding;

    fn next(&mut self) -> Option<Binding> {
        match self {
            Self::Keys(keys) => keys.next().copied(),
            Self::Diamond(keys) => keys.next(),
        }
    }
}

pub(super) struct ShortcutGroup {
    pub(super) heading: &'static str,
    pub(super) shortcuts: &'static [Shortcut],
}

const fn binds(key: Key, command: Command) -> Binding {
    Binding {
        key,
        action: KeyAction::Command(command),
    }
}

const fn movement(key: Key, movement: Movement) -> Binding {
    binds(key, Command::Movement(movement))
}

const fn preference(key: Key, change: PreferenceChange) -> Binding {
    binds(key, Command::Preference(change))
}

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
                keys: &[
                    movement(Key::Down, Movement::Down),
                    movement(Key::Up, Movement::Up),
                ],
                action: "down / up",
            },
            Shortcut::Keys {
                keys: &[
                    movement(Key::Control('d'), Movement::HalfPageDown),
                    movement(Key::Control('u'), Movement::HalfPageUp),
                ],
                action: "half page down / up",
            },
            Shortcut::Keys {
                keys: &[
                    movement(Key::Char('g'), Movement::Top),
                    movement(Key::Char('G'), Movement::Bottom),
                ],
                action: "top / bottom",
            },
            Shortcut::Keys {
                keys: &[
                    movement(Key::Char('0'), Movement::FarLeft),
                    movement(Key::Char('^'), Movement::FarLeft),
                ],
                action: "far left",
            },
            Shortcut::Keys {
                keys: &[movement(Key::Char('$'), Movement::FarRight)],
                action: "far right",
            },
        ],
    },
    ShortcutGroup {
        heading: "Files",
        shortcuts: &[
            Shortcut::Keys {
                keys: &[
                    binds(Key::Tab, Command::Page(PageStep::Next)),
                    binds(Key::ShiftTab, Command::Page(PageStep::Previous)),
                ],
                action: "next / previous file",
            },
            Shortcut::Keys {
                keys: &[preference(Key::Char('e'), PreferenceChange::ToggleFileList)],
                action: "show / hide file list",
            },
        ],
    },
    ShortcutGroup {
        heading: "View",
        shortcuts: &[
            Shortcut::Keys {
                keys: &[preference(Key::Char('v'), PreferenceChange::NextLayout)],
                action: "cycle layout",
            },
            Shortcut::Keys {
                keys: &[preference(
                    Key::Char('c'),
                    PreferenceChange::ToggleGranularity,
                )],
                action: "character detail",
            },
            Shortcut::Keys {
                keys: &[preference(Key::Char('w'), PreferenceChange::ToggleWrapping)],
                action: "wrap",
            },
            Shortcut::Keys {
                keys: &[
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
                keys: &[binds(
                    Key::Char('?'),
                    Command::Keymap(KeymapCommand::Toggle),
                )],
                action: "open keymap",
            },
            Shortcut::ClosesKeymap {
                bound: &[binds(Key::Esc, Command::Keymap(KeymapCommand::Close))],
                action: "close keymap",
            },
            Shortcut::Keys {
                keys: &[
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
        interaction::{Command, Event, KeymapCommand, PageStep},
        preferences::PreferenceChange,
        scroll::Movement,
    };

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<Event> {
        event_for(TerminalEvent::Key(KeyEvent::new(code, modifiers)))
    }

    fn command(command: Command) -> Option<Event> {
        Some(Event::Command(command))
    }

    #[test]
    fn maps_pager_keys_and_resize_events() {
        let plain = |character| key(KeyCode::Char(character), KeyModifiers::NONE);

        assert_eq!(plain('j'), command(Command::Movement(Movement::Down)));
        for (character, expected) in [
            ('h', Movement::Left),
            ('l', Movement::Right),
            ('0', Movement::FarLeft),
            ('^', Movement::FarLeft),
            ('$', Movement::FarRight),
        ] {
            assert_eq!(
                plain(character),
                command(Command::Movement(expected)),
                "{character} horizontal movement"
            );
        }
        assert_eq!(
            key(KeyCode::Tab, KeyModifiers::NONE),
            command(Command::Page(PageStep::Next))
        );
        assert_eq!(
            plain('v'),
            command(Command::Preference(PreferenceChange::NextLayout))
        );
        assert_eq!(
            plain('+'),
            command(Command::Preference(PreferenceChange::IncreaseContext))
        );
        assert_eq!(
            plain('-'),
            command(Command::Preference(PreferenceChange::DecreaseContext))
        );
        assert_eq!(
            key(KeyCode::BackTab, KeyModifiers::SHIFT),
            command(Command::Page(PageStep::Previous))
        );
        assert_eq!(
            key(KeyCode::Tab, KeyModifiers::SHIFT),
            command(Command::Page(PageStep::Previous))
        );
        for (code, expected) in [
            (
                KeyCode::Char('e'),
                Command::Preference(PreferenceChange::ToggleFileList),
            ),
            (KeyCode::Char('?'), Command::Keymap(KeymapCommand::Toggle)),
            (KeyCode::Esc, Command::Keymap(KeymapCommand::Close)),
        ] {
            assert_eq!(key(code, KeyModifiers::NONE), command(expected), "{code:?}");
        }
        for character in ['{', '}'] {
            assert_eq!(
                plain(character),
                None,
                "{character} has no cursor-visible destination"
            );
        }
        assert_eq!(
            event_for(TerminalEvent::Resize(100, 30)),
            Some(Event::Resize)
        );
        assert_eq!(
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Some(Event::Interrupt)
        );
    }

    /// architecture.md (Keyboard bindings): each key has one binding.
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

    /// architecture.md (Keyboard bindings): `?`, `Esc`, and `q` close the keymap popup.
    #[test]
    fn lists_the_keys_that_close_the_open_keymap() {
        assert_eq!(
            keymap_closing_keys().collect::<Vec<_>>(),
            [Key::Char('?'), Key::Esc, Key::Char('q')]
        );
    }
}
