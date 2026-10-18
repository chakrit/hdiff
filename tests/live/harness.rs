//! The live-test harness: a `Tui` on a test terminal, and the layout and detail modes the
//! scenario matrix runs.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use hdiff::{
    effective_view::EffectiveView,
    parser::{parse_comparison, parse_patch},
    preferences::{DiffGranularity, DiffLayout},
    prepared::PreparedDocument,
    terminal::{Step, Tui},
    theme::{ColorDepth, Theme},
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
pub const THEME: &Theme = Theme::for_depth(ColorDepth::TrueColor);

pub struct Live {
    pub tui: Tui,
    terminal: Terminal<TestBackend>,
}

impl Live {
    pub fn load(patch: &str, mode: Mode) -> Self {
        Self::sized(patch, mode, Size::new(WIDTH, HEIGHT))
    }

    /// `patch` on a terminal of `size`.
    pub fn sized(patch: &str, mode: Mode, size: Size) -> Self {
        let document = parse_patch(patch.as_bytes()).expect("valid fixture patch");
        Self::start(PreparedDocument::prepare(&document), mode, size)
    }

    /// The output of comparing file operands.
    pub fn comparison(output: &str, mode: Mode) -> Self {
        let document = parse_comparison(output.as_bytes()).expect("valid comparison output");
        Self::start(
            PreparedDocument::prepare(&document),
            mode,
            Size::new(WIDTH, HEIGHT),
        )
    }

    fn start(prepared: PreparedDocument, mode: Mode, size: Size) -> Self {
        let mut terminal =
            Terminal::new(TestBackend::new(size.width, size.height)).expect("test terminal");
        let mut tui = Tui::new(prepared, size, ColorDepth::TrueColor);
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
        self.resize_backend_only(width, height);
        self.event(Event::Resize(width, height))
    }

    /// Changes the terminal's size without sending the resize event that follows it.
    pub fn resize_backend_only(&mut self, width: u16, height: u16) {
        self.terminal.backend_mut().resize(width, height);
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

    /// Screen rows whose first change label is an `old_N` label: with every diff-pane line
    /// of the before pane labeled, the lines that pane shows.
    pub fn old_label_lines(&self) -> usize {
        self.rows()
            .iter()
            .filter(|row| change_label(row).is_some_and(|label| label.starts_with("old_")))
            .count()
    }

    /// The screen row of the diff pane's middle line, where anchored rows stay: the pane's
    /// first content line plus half the lines the current view gives each diff pane.
    pub fn pane_middle_line(&self) -> usize {
        let screen = self.terminal.backend().buffer().area.as_size();
        let view = EffectiveView::of(self.tui.preferences(), screen);
        FIRST_CONTENT_LINE + view.pane_content().height.half().get()
    }

    /// Cell position of the first occurrence of `text`.
    pub fn find(&self, text: &str) -> Position {
        self.rows()
            .iter()
            .enumerate()
            .find_map(|(y, row)| {
                let byte = row.find(text)?;
                let x = row[..byte].chars().count();
                let column = u16::try_from(x).expect("a screen column fits u16");
                let row = u16::try_from(y).expect("a screen row fits u16");
                Some(Position::new(column, row))
            })
            .unwrap_or_else(|| panic!("{text:?} is not on screen:\n{}", self.rows().join("\n")))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Mode {
    pub layout: DiffLayout,
    pub detail: DiffGranularity,
}

impl Mode {
    pub const fn line(layout: DiffLayout) -> Self {
        Self {
            layout,
            detail: DiffGranularity::Line,
        }
    }

    pub const fn character(layout: DiffLayout) -> Self {
        Self {
            layout,
            detail: DiffGranularity::Character,
        }
    }
}

/// The leftmost `old_N` or `new_N` label in `row`.
pub fn change_label(row: &str) -> Option<String> {
    let old = row.find("old_");
    let new = row.find("new_");
    let start = old.into_iter().chain(new).min()?;
    let label = row[start..]
        .chars()
        .take_while(|symbol| symbol.is_alphanumeric() || *symbol == '_')
        .collect();

    Some(label)
}

/// The diff pane's part of a screen row, right of the file-list separator.
pub fn diff_pane(row: &str) -> String {
    row.split_once('│')
        .map_or(row, |(_, pane)| pane)
        .trim_start_matches(' ')
        .to_owned()
}
