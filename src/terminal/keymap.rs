//! The keymap popup: every shortcut, grouped under headings. Rules:
//! `docs/spec/architecture.md` (Interaction model, Keyboard bindings).

use ratatui::{
    Frame,
    layout::{Constraint, Layout as RatatuiLayout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use super::{
    chrome::{ChromePalette, chrome_palette},
    rows::hunk_header_color,
};
use crate::{syntax::SyntaxClass, theme::DEFAULT_SYNTAX_THEME};

/// Shortcuts that belong together, as keys and the action they take.
struct ShortcutGroup {
    heading: &'static str,
    shortcuts: &'static [(&'static str, &'static str)],
}

const GROUPS: [ShortcutGroup; 4] = [
    ShortcutGroup {
        heading: "Scroll",
        shortcuts: &[
            ("j k ↓ ↑", "down / up"),
            ("h l", "left / right"),
            ("^D ^U", "half page down / up"),
            ("g G", "top / bottom"),
            ("0 ^", "left edge"),
            ("$", "right end"),
        ],
    },
    ShortcutGroup {
        heading: "Files",
        shortcuts: &[
            ("Tab ⇧Tab", "next / previous file"),
            ("e", "show / hide file list"),
        ],
    },
    ShortcutGroup {
        heading: "View",
        shortcuts: &[
            ("v", "cycle layout"),
            ("c", "character detail"),
            ("w", "wrap"),
            ("+ -", "more / less context"),
        ],
    },
    ShortcutGroup {
        heading: "Keymap and exit",
        shortcuts: &[
            ("?", "open keymap"),
            ("? Esc q", "close keymap"),
            ("q ^C", "exit"),
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
pub(super) fn render_keymap_popup(frame: &mut Frame, area: Rect, color_count: u16) {
    let palette = chrome_palette(color_count);
    let groups_per_column = match area.width >= TWO_COLUMN_WIDTH {
        true => GROUPS.len().div_ceil(2),
        false => GROUPS.len(),
    };
    let columns = GROUPS
        .chunks(groups_per_column)
        .map(|groups| column_lines(groups, palette, color_count))
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
        .border_style(Style::default().fg(palette.separator))
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

/// Each group's heading over its shortcuts, with a blank line between groups. Headings take
/// the hunk-header tone, since both head sections, and keys the syntax function color.
fn column_lines(
    groups: &[ShortcutGroup],
    palette: ChromePalette,
    color_count: u16,
) -> Vec<Line<'static>> {
    let heading_style = Style::default()
        .fg(hunk_header_color(color_count))
        .add_modifier(Modifier::BOLD);
    let keys_style =
        Style::default().fg(DEFAULT_SYNTAX_THEME.color(SyntaxClass::Function, color_count));
    let action_style = Style::default().fg(palette.file_name);

    let group_lines = groups.iter().map(|group| {
        let heading = Line::styled(group.heading, heading_style);
        let shortcuts = group.shortcuts.iter().map(|(keys, action)| {
            Line::from(vec![
                Span::styled(format!("  {keys:<KEYS_WIDTH$} "), keys_style),
                Span::styled(*action, action_style),
            ])
        });

        std::iter::once(heading)
            .chain(shortcuts)
            .collect::<Vec<_>>()
    });

    group_lines.collect::<Vec<_>>().join(&Line::default())
}

#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal,
        backend::TestBackend,
        buffer::Buffer,
        style::{Color, Modifier},
    };

    use super::render_keymap_popup;

    fn render_popup(width: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, 30)).expect("test terminal");
        terminal
            .draw(|frame| render_keymap_popup(frame, frame.area(), u16::MAX))
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
            "down / up",
            "left / right",
            "half page down / up",
            "top / bottom",
            "left edge",
            "right end",
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

        assert_eq!(heading.fg, Color::Rgb(100, 155, 175), "heading");
        assert!(heading.modifier.contains(Modifier::BOLD), "bold heading");
        assert_eq!(key.fg, Color::Rgb(220, 220, 170), "key");
        assert_eq!(action.fg, Color::Rgb(160, 160, 160), "action");
    }
}
