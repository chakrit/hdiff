//! The keymap popup: every shortcut of the bindings, grouped under headings. Rules:
//! `docs/spec/architecture.md` (Interaction model, Keyboard bindings).

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use super::bindings::{GROUPS, Key, Shortcut, ShortcutGroup, keymap_closing_keys};
use crate::theme::Theme;

const KEYS_WIDTH: u16 = 9;
const ACTION_WIDTH: u16 = 21;
/// A shortcut row: a two-cell indent under its heading, the keys, a space, and the action.
const COLUMN_WIDTH: u16 = 2 + KEYS_WIDTH + 1 + ACTION_WIDTH;
const COLUMN_GAP: u16 = 3;
/// A border and a padding column on each side.
const FRAME_WIDTH: u16 = 4;
/// A border row above and below.
const FRAME_HEIGHT: u16 = 2;

/// How many columns of groups the popup lays out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Columns {
    One,
    Two,
}

impl Columns {
    /// Two columns when they fit `width`, one otherwise.
    fn of(width: u16) -> Self {
        match width >= Self::Two.width() {
            true => Self::Two,
            false => Self::One,
        }
    }

    fn count(self) -> u16 {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }

    fn groups_per_column(self) -> usize {
        GROUPS.len().div_ceil(usize::from(self.count()))
    }

    /// The framed popup's width: its columns and the gaps between them.
    fn width(self) -> u16 {
        match self {
            Self::One => COLUMN_WIDTH + FRAME_WIDTH,
            Self::Two => 2 * COLUMN_WIDTH + COLUMN_GAP + FRAME_WIDTH,
        }
    }
}

/// The popup, centered over `area` and clipped to it: two columns of groups when they fit
/// the width, one otherwise.
pub(super) fn render_keymap_popup(frame: &mut Frame, area: Rect, theme: &Theme) {
    let columns = Columns::of(area.width);
    let styles = PopupStyles::of(theme);
    let group_columns = GROUPS
        .chunks(columns.groups_per_column())
        .map(|groups| column_lines(groups, &styles))
        .collect::<Vec<_>>();
    let tallest = group_columns
        .iter()
        .map(Vec::len)
        .max()
        .expect("the keymap has groups");

    let popup = popup_area(area, columns, tallest);
    let border = Block::bordered()
        .border_style(Style::default().fg(theme.chrome.separator))
        .padding(Padding::horizontal(1));
    let column_areas = Layout::horizontal(vec![
        Constraint::Length(COLUMN_WIDTH);
        usize::from(columns.count())
    ])
    .spacing(COLUMN_GAP)
    .split(border.inner(popup));

    frame.render_widget(Clear, popup);
    frame.render_widget(border, popup);
    for (lines, column_area) in group_columns.into_iter().zip(column_areas.iter()) {
        frame.render_widget(Paragraph::new(lines), *column_area);
    }
}

/// The framed popup of `columns`, the tallest `tallest` lines, centered over `area` and
/// clipped to it.
fn popup_area(area: Rect, columns: Columns, tallest: usize) -> Rect {
    let width = columns.width();
    let height =
        u16::try_from(tallest).expect("the keymap fits a terminal's height") + FRAME_HEIGHT;

    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
    .intersection(area)
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

    /// Keys padded to one column, with the row indent.
    fn keys_span(&self, keys: String) -> Span<'static> {
        let width = usize::from(KEYS_WIDTH);
        Span::styled(format!("  {keys:<width$} "), self.keys)
    }

    fn with_action(&self, keys: String, action: &'static str) -> Line<'static> {
        Line::from(vec![
            self.keys_span(keys),
            Span::styled(action, self.action),
        ])
    }
}

/// Each group's heading over its shortcuts, with a blank line between groups.
fn column_lines(groups: &[ShortcutGroup], styles: &PopupStyles) -> Vec<Line<'static>> {
    let group_lines = groups.iter().map(|group| {
        let heading = Line::styled(group.heading, styles.heading);
        let shortcuts = group
            .shortcuts
            .iter()
            .flat_map(|shortcut| shortcut_lines(shortcut, styles));

        std::iter::once(heading)
            .chain(shortcuts)
            .collect::<Vec<_>>()
    });

    group_lines.collect::<Vec<_>>().join(&Line::default())
}

/// The popup lines of one shortcut row.
fn shortcut_lines(shortcut: &Shortcut, styles: &PopupStyles) -> Vec<Line<'static>> {
    match shortcut {
        Shortcut::Keys { keys, action } => {
            let keys = keys.iter().map(|binding| binding.key);
            vec![styles.with_action(key_labels(keys), action)]
        }
        Shortcut::ClosesKeymap { action, .. } => {
            vec![styles.with_action(key_labels(keymap_closing_keys()), action)]
        }
        Shortcut::Diamond {
            up,
            left,
            right,
            down,
            action,
        } => vec![
            Line::from(styles.keys_span(format!("  {}", up.key.label()))),
            styles.with_action(
                format!("{} · {}", left.key.label(), right.key.label()),
                action,
            ),
            Line::from(styles.keys_span(format!("  {}", down.key.label()))),
        ],
    }
}

fn key_labels(keys: impl Iterator<Item = Key>) -> String {
    keys.map(Key::label).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal, backend::TestBackend, buffer::Buffer, layout::Position, style::Modifier,
    };

    use super::render_keymap_popup;
    use crate::theme::{ColorDepth, Theme};

    const THEME: &Theme = Theme::for_depth(ColorDepth::TrueColor);

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

    /// The first position where `symbols` are drawn in consecutive cells of one row.
    fn position(buffer: &Buffer, symbols: &[&str]) -> Option<Position> {
        buffer.area.positions().find(|start| {
            let row = (start.x..buffer.area.right()).map(|x| buffer[(x, start.y)].symbol());
            symbols.len() <= row.len() && row.zip(symbols).all(|(drawn, symbol)| drawn == *symbol)
        })
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
            "? Esc q",
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
        let middle = position(buffer, &["h", " ", "·", " ", "l"]).expect("h · l is drawn");
        let center = middle.x + 2;

        assert_eq!(buffer[(center, middle.y - 1)].symbol(), "k", "k above");
        assert_eq!(buffer[(center, middle.y + 1)].symbol(), "j", "j below");
    }

    /// architecture.md (Interaction model): headings take the hunk-header tone in bold, keys
    /// the syntax function color, and actions the file-name tone.
    #[test]
    fn colors_headings_keys_and_actions_from_the_existing_palettes() {
        let buffer = render_popup(60);
        let at = |text: &str| {
            let symbols = text.chars().map(String::from).collect::<Vec<_>>();
            let symbols = symbols.iter().map(String::as_str).collect::<Vec<_>>();
            position(&buffer, &symbols).unwrap_or_else(|| panic!("{text} is drawn"))
        };

        let heading = &buffer[at("Scroll")];
        let key = &buffer[at("^D ^U")];
        let action = &buffer[at("half page")];

        assert_eq!(heading.fg, THEME.rows.hunk_header, "heading");
        assert!(heading.modifier.contains(Modifier::BOLD), "bold heading");
        assert_eq!(key.fg, THEME.syntax.function, "key");
        assert_eq!(action.fg, THEME.chrome.file_name, "action");
    }
}
