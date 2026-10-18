//! Key bindings and the keymap popup that lists them, grouped under headings. Rules:
//! `docs/spec/architecture.md` (Interaction model, Keyboard bindings).

use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout as RatatuiLayout, Rect, Size},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use crate::{
    interaction::{Command, Event, KeymapPopup, Movement, PageStep},
    preferences::PreferenceChange,
    theme::Theme,
};

/// The session event a terminal event stands for, if any.
pub(super) fn event_for(event: TerminalEvent) -> Option<Event> {
    match event {
        TerminalEvent::Key(key) if key.kind == KeyEventKind::Press => event_for_key(key),
        TerminalEvent::Resize(width, height) => Some(Event::Resize(Size::new(width, height))),
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
    GROUPS
        .iter()
        .flat_map(|group| group.shortcuts)
        .flat_map(Shortcut::bindings)
        .find_map(|binding| match binding {
            Binding::Bound(bound, event) if bound == key => Some(event),
            Binding::Bound(..) | Binding::Shown(_) => None,
        })
}

/// A key as the bindings name it: Shift folds into the character it types, and only a
/// Control chord of a character stays distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
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

    fn label(self) -> String {
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

/// A key a shortcut row shows: bound there to an event, or bound in another row and acting
/// here through the keymap popup.
#[derive(Debug, Clone, Copy)]
enum Binding {
    Bound(Key, Event),
    Shown(Key),
}

impl Binding {
    fn key(self) -> Key {
        match self {
            Self::Bound(key, _) | Self::Shown(key) => key,
        }
    }
}

/// One row of the popup: its keys and the action they take. A diamond spreads four keys
/// over three lines, its action beside the middle one.
enum Shortcut {
    Keys {
        keys: &'static [Binding],
        action: &'static str,
    },
    Diamond {
        up: Binding,
        left: Binding,
        right: Binding,
        down: Binding,
        action: &'static str,
    },
}

impl Shortcut {
    fn bindings(&self) -> impl Iterator<Item = Binding> {
        let (keys, diamond) = match self {
            Self::Keys { keys, .. } => (*keys, None),
            Self::Diamond {
                up,
                left,
                right,
                down,
                ..
            } => (&[][..], Some([*up, *left, *right, *down])),
        };

        keys.iter().copied().chain(diamond.into_iter().flatten())
    }

    /// The popup lines of this row: keys padded to one column, then the action.
    fn lines(&self, styles: &PopupStyles) -> Vec<Line<'static>> {
        let keys_span = |keys: String| Span::styled(format!("  {keys:<KEYS_WIDTH$} "), styles.keys);
        let with_action = |keys: String, action: &'static str| {
            Line::from(vec![keys_span(keys), Span::styled(action, styles.action)])
        };
        let keys_only = |keys: String| Line::from(keys_span(keys));

        match self {
            Self::Keys { keys, action } => {
                let labels = keys.iter().map(|binding| binding.key().label());
                vec![with_action(labels.collect::<Vec<_>>().join(" "), action)]
            }
            Self::Diamond {
                up,
                left,
                right,
                down,
                action,
            } => vec![
                keys_only(format!("  {}", up.key().label())),
                with_action(
                    format!("{} · {}", left.key().label(), right.key().label()),
                    action,
                ),
                keys_only(format!("  {}", down.key().label())),
            ],
        }
    }
}

struct ShortcutGroup {
    heading: &'static str,
    shortcuts: &'static [Shortcut],
}

const fn binds(key: Key, command: Command) -> Binding {
    Binding::Bound(key, Event::Command(command))
}

const fn movement(key: Key, movement: Movement) -> Binding {
    binds(key, Command::Movement(movement))
}

const fn preference(key: Key, change: PreferenceChange) -> Binding {
    binds(key, Command::Preference(change))
}

const GROUPS: [ShortcutGroup; 4] = [
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
                keys: &[binds(Key::Char('?'), Command::Keymap(KeymapPopup::Open))],
                action: "open keymap",
            },
            Shortcut::Keys {
                keys: &[
                    Binding::Shown(Key::Char('?')),
                    binds(Key::Esc, Command::Keymap(KeymapPopup::Closed)),
                    Binding::Shown(Key::Char('q')),
                ],
                action: "close keymap",
            },
            Shortcut::Keys {
                keys: &[
                    binds(Key::Char('q'), Command::Quit),
                    Binding::Bound(Key::Control('c'), Event::Interrupt),
                ],
                action: "exit",
            },
        ],
    },
];

const KEYS_WIDTH: usize = 9;
const ACTION_WIDTH: u16 = 21;
/// A shortcut row: a two-cell indent under its heading, the keys, a space, and the action.
const COLUMN_WIDTH: u16 = 2 + KEYS_WIDTH as u16 + 1 + ACTION_WIDTH;
const COLUMN_GAP: u16 = 3;
/// A border and a padding column on each side.
const FRAME_WIDTH: u16 = 4;
/// A border row above and below.
const FRAME_HEIGHT: u16 = 2;
const TWO_COLUMN_WIDTH: u16 = 2 * COLUMN_WIDTH + COLUMN_GAP + FRAME_WIDTH;

/// The popup, centered over `area` and clipped to it: two columns of groups when they fit
/// the width, one otherwise.
pub(super) fn render_keymap_popup(frame: &mut Frame, area: Rect, theme: &Theme) {
    let groups_per_column = match area.width >= TWO_COLUMN_WIDTH {
        true => GROUPS.len().div_ceil(2),
        false => GROUPS.len(),
    };
    let styles = PopupStyles::of(theme);
    let columns = GROUPS
        .chunks(groups_per_column)
        .map(|groups| column_lines(groups, &styles))
        .collect::<Vec<_>>();

    let column_count = columns.len() as u16;
    let width = column_count * COLUMN_WIDTH + (column_count - 1) * COLUMN_GAP + FRAME_WIDTH;
    let tallest = columns.iter().map(Vec::len).max().unwrap_or(0);
    let height = u16::try_from(tallest).unwrap_or(u16::MAX) + FRAME_HEIGHT;
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
    .intersection(area);
    let border = Block::bordered()
        .border_style(Style::default().fg(theme.chrome.separator))
        .padding(Padding::horizontal(1));
    let column_areas =
        RatatuiLayout::horizontal(vec![Constraint::Length(COLUMN_WIDTH); columns.len()])
            .spacing(COLUMN_GAP)
            .split(border.inner(popup));

    frame.render_widget(Clear, popup);
    frame.render_widget(border, popup);
    for (lines, column_area) in columns.into_iter().zip(column_areas.iter()) {
        frame.render_widget(Paragraph::new(lines), *column_area);
    }
}

/// Headings take the hunk-header tone, since both head sections, and keys the syntax
/// function color.
struct PopupStyles {
    heading: Style,
    keys: Style,
    action: Style,
}

impl PopupStyles {
    fn of(theme: &Theme) -> Self {
        Self {
            heading: Style::default()
                .fg(theme.rows.hunk_header)
                .add_modifier(Modifier::BOLD),
            keys: Style::default().fg(theme.syntax.function),
            action: Style::default().fg(theme.chrome.file_name),
        }
    }
}

/// Each group's heading over its shortcuts, with a blank line between groups.
fn column_lines(groups: &[ShortcutGroup], styles: &PopupStyles) -> Vec<Line<'static>> {
    let group_lines = groups.iter().map(|group| {
        let heading = Line::styled(group.heading, styles.heading);
        let shortcuts = group
            .shortcuts
            .iter()
            .flat_map(|shortcut| shortcut.lines(styles));

        std::iter::once(heading)
            .chain(shortcuts)
            .collect::<Vec<_>>()
    });

    group_lines.collect::<Vec<_>>().join(&Line::default())
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Size, style::Modifier};

    use super::{Binding, GROUPS, Shortcut, event_for, render_keymap_popup};
    use crate::{
        interaction::{Command, Event, KeymapPopup, Movement, PageStep},
        preferences::PreferenceChange,
        theme::{ColorDepth, Theme},
    };

    const THEME: &Theme = Theme::for_depth(ColorDepth::TrueColor);

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
            (KeyCode::Char('?'), Command::Keymap(KeymapPopup::Open)),
            (KeyCode::Esc, Command::Keymap(KeymapPopup::Closed)),
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
            Some(Event::Resize(Size::new(100, 30)))
        );
        assert_eq!(
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Some(Event::Interrupt)
        );
    }

    /// architecture.md (Keyboard bindings): every key the popup shows acts, and each key
    /// has one binding.
    #[test]
    fn binds_every_shown_key_exactly_once() {
        let bindings = GROUPS
            .iter()
            .flat_map(|group| group.shortcuts)
            .flat_map(Shortcut::bindings)
            .collect::<Vec<_>>();
        let bound_keys = bindings
            .iter()
            .filter_map(|binding| match binding {
                Binding::Bound(key, _) => Some(*key),
                Binding::Shown(_) => None,
            })
            .collect::<Vec<_>>();

        for binding in &bindings {
            let key = binding.key();
            let count = bound_keys.iter().filter(|bound| **bound == key).count();
            assert_eq!(count, 1, "{key:?}");
        }
    }

    fn render_popup(width: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, 30)).expect("test terminal");
        terminal
            .draw(|frame| render_keymap_popup(frame, frame.area(), THEME))
            .expect("render popup");
        terminal.backend().buffer().clone()
    }

    fn popup_text(width: u16) -> String {
        let buffer = render_popup(width);

        (0..buffer.area.height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// architecture.md (Interaction model, Keyboard bindings): a bordered popup lists every
    /// shortcut under four headings, in two columns when they fit.
    #[test]
    fn groups_every_shortcut_under_headings_in_columns_that_fit() {
        let wide = popup_text(120);
        let narrow = popup_text(60);

        for text in [
            "Scroll",
            "Files",
            "View",
            "Keymap and exit",
            "scroll",
            "down / up",
            "half page down / up",
            "top / bottom",
            "far left",
            "far right",
            "next / previous file",
            "show / hide file list",
            "cycle layout",
            "character detail",
            "wrap",
            "more / less context",
            "open keymap",
            "close keymap",
            "exit",
        ] {
            assert!(
                wide.contains(text) && narrow.contains(text),
                "popup lists {text}"
            );
        }
        assert!(wide.contains('┌') && wide.contains('┘'), "popup border");
        assert_diamond(&render_popup(120));
        assert_diamond(&render_popup(60));
        assert!(
            wide.lines()
                .any(|row| row.contains("Scroll") && row.contains("View")),
            "two columns when wide"
        );
        assert!(
            !narrow
                .lines()
                .any(|row| row.contains("Scroll") && row.contains("View")),
            "one column when narrow"
        );
    }

    /// architecture.md (Keyboard bindings): Scroll shows `h`, `j`, `k`, and `l` as a diamond.
    fn assert_diamond(buffer: &Buffer) {
        let middle = buffer
            .area
            .positions()
            .find(|start| {
                ["h", " ", "·", " ", "l"]
                    .iter()
                    .enumerate()
                    .all(|(offset, symbol)| {
                        let x = start.x + offset as u16;
                        x < buffer.area.right() && buffer[(x, start.y)].symbol() == *symbol
                    })
            })
            .expect("h · l is drawn");
        let center = middle.x + 2;

        assert_eq!(buffer[(center, middle.y - 1)].symbol(), "k", "k above");
        assert_eq!(buffer[(center, middle.y + 1)].symbol(), "j", "j below");
    }

    /// architecture.md (Interaction model): headings take the hunk-header tone in bold, keys
    /// the syntax function color, and actions the file-name tone.
    #[test]
    fn colors_headings_keys_and_actions_from_the_existing_palettes() {
        let buffer = render_popup(60);
        let position = |text: &str| {
            let symbols = text.chars().map(String::from).collect::<Vec<_>>();
            buffer
                .area
                .positions()
                .find(|start| {
                    symbols.iter().enumerate().all(|(offset, symbol)| {
                        let x = start.x + offset as u16;
                        x < buffer.area.right() && buffer[(x, start.y)].symbol() == symbol
                    })
                })
                .unwrap_or_else(|| panic!("{text} is drawn"))
        };

        let heading = &buffer[position("Scroll")];
        let key = &buffer[position("^D ^U")];
        let action = &buffer[position("half page")];

        assert_eq!(heading.fg, THEME.rows.hunk_header, "heading");
        assert!(heading.modifier.contains(Modifier::BOLD), "bold heading");
        assert_eq!(key.fg, THEME.syntax.function, "key");
        assert_eq!(action.fg, THEME.chrome.file_name, "action");
    }
}
