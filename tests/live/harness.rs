//! The live-test harness: a `Tui` on a test terminal, and the layout and detail modes the
//! scenario matrix runs.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use hdiff::{
    parser::parse_patch,
    preferences::{DiffGranularity, DiffLayout},
    prepared::PreparedDocument,
    terminal::{Step, Tui},
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::{Position, Size},
    style::Color,
};

pub const WIDTH: u16 = 120;
pub const HEIGHT: u16 = 36;
/// The screen row of a diff pane's first content line, below its top frame row.
pub const FIRST_CONTENT_LINE: usize = 1;

pub struct Live {
    pub tui: Tui,
    terminal: Terminal<TestBackend>,
}

impl Live {
    pub fn load(patch: &str, mode: Mode) -> Self {
        let document = parse_patch(patch.as_bytes()).expect("valid fixture patch");
        let prepared = PreparedDocument::prepare(&document);
        let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("test terminal");
        let tui = Tui::new(
            prepared,
            Size::new(WIDTH, HEIGHT),
            hdiff::theme::ColorDepth::TrueColor,
        );
        tui.draw(&mut terminal).expect("initial frame");

        let mut live = Self { tui, terminal };
        live.enter(mode);
        live
    }

    pub fn enter(&mut self, mode: Mode) {
        while self.tui.preferences().layout != mode.layout {
            self.press('v');
        }
        if self.tui.preferences().granularity != mode.detail {
            self.press('c');
        }
    }

    pub fn press(&mut self, key: char) -> Step {
        self.send(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))
    }

    pub fn send(&mut self, key: KeyEvent) -> Step {
        self.event(Event::Key(key))
    }

    pub fn resize(&mut self, width: u16, height: u16) -> Step {
        self.terminal.backend_mut().resize(width, height);
        self.event(Event::Resize(width, height))
    }

    pub fn event(&mut self, event: Event) -> Step {
        let Ok(step) = self.tui.step(&mut self.terminal, event);
        step
    }

    pub fn buffer(&self) -> Buffer {
        self.terminal.backend().buffer().clone()
    }

    pub fn rows(&self) -> Vec<String> {
        let buffer = self.terminal.backend().buffer();
        let area = buffer.area;

        (area.top()..area.bottom())
            .map(|y| {
                (area.left()..area.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    /// The file-list column: each row's text left of the pane separator.
    pub fn file_list(&self) -> Vec<String> {
        self.rows()
            .iter()
            .map(|row| {
                let column = row
                    .split_once('│')
                    .map_or(row.as_str(), |(column, _)| column);
                column.trim_end().to_owned()
            })
            .collect()
    }

    /// The background of the cell where `text` first appears.
    pub fn background(&self, text: &str) -> Color {
        let position = self.find(text);
        self.buffer()[(position.x, position.y)].bg
    }

    pub fn count(&self, symbol: char) -> usize {
        self.rows()
            .iter()
            .map(|row| row.matches(symbol).count())
            .sum()
    }

    pub fn shows(&self, text: &str) -> bool {
        self.rows().iter().any(|row| row.contains(text))
    }

    /// The marker and name foregrounds of the file row `row`, such as `"~ lib.rs"`; the
    /// name must be drawn in one color.
    pub fn file_row_colors(&self, row: &str) -> (Color, Color) {
        let start = self.find(row);
        let buffer = self.buffer();
        let marker = buffer[(start.x, start.y)].fg;
        let name = (start.x + 2..)
            .take(row.chars().count() - 2)
            .map(|x| buffer[(x, start.y)].fg)
            .collect::<Vec<_>>();

        assert!(
            name.windows(2).all(|pair| pair[0] == pair[1]),
            "{row:?} draws its name in one color: {name:?}"
        );
        (marker, name[0])
    }

    /// Cell position of the first occurrence of `text`.
    pub fn find(&self, text: &str) -> Position {
        self.rows()
            .iter()
            .enumerate()
            .find_map(|(y, row)| {
                let byte = row.find(text)?;
                let x = row[..byte].chars().count();
                Some(Position::new(x as u16, y as u16))
            })
            .unwrap_or_else(|| panic!("{text:?} is not on screen:\n{}", self.rows().join("\n")))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Mode {
    pub layout: DiffLayout,
    pub detail: DiffGranularity,
}

pub fn unified_line() -> Mode {
    Mode {
        layout: DiffLayout::Unified,
        detail: DiffGranularity::Line,
    }
}

/// The diff pane's part of a screen row, right of the file-list separator.
pub fn diff_pane(row: &str) -> String {
    row.split_once('│')
        .map_or(row, |(_, pane)| pane)
        .trim_start_matches(' ')
        .to_owned()
}
