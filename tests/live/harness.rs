//! The live-test harness: a `Tui` on a test terminal, and the layout and detail modes the
//! scenario matrix runs.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use hdiff::{
    effective_view::EffectiveView,
    parser::{parse_comparison, parse_patch},
    preferences::{DiffGranularity, DiffLayout, ViewPreferences},
    prepared::PreparedDocument,
    screen::Cells,
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
use unicode_segmentation::UnicodeSegmentation;

pub const WIDTH: u16 = 120;
pub const HEIGHT: u16 = 36;
/// The screen a test runs on unless it names a size.
pub const SCREEN: Size = Size::new(WIDTH, HEIGHT);
/// The screen row of a diff pane's first content line, below its top frame row.
pub const FIRST_CONTENT_LINE: usize = 1;
pub const THEME: &Theme = Theme::for_depth(ColorDepth::TrueColor);

pub struct Live {
    tui: Tui,
    terminal: Terminal<TestBackend>,
}

impl Live {
    pub fn load(patch: &str, mode: Mode) -> Self {
        Self::sized(patch, mode, SCREEN)
    }

    /// `patch` on a terminal of `size`.
    pub fn sized(patch: &str, mode: Mode, size: Size) -> Self {
        let document = parse_patch(patch.as_bytes()).expect("valid fixture patch");
        Self::start(PreparedDocument::prepare(&document), mode, size)
    }

    /// The output of comparing file operands.
    pub fn comparison(output: &str, mode: Mode) -> Self {
        let document = parse_comparison(output.as_bytes()).expect("valid comparison output");
        Self::start(PreparedDocument::prepare(&document), mode, SCREEN)
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

    pub fn preferences(&self) -> ViewPreferences {
        self.tui.preferences()
    }

    pub fn enter(&mut self, mode: Mode) {
        while self.tui.preferences().layout != mode.layout {
            self.press('v');
        }
        if self.tui.preferences().granularity != mode.granularity {
            self.press('c');
        }
    }

    pub fn press(&mut self, key: char) -> Step {
        self.send(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))
    }

    pub fn send(&mut self, key: KeyEvent) -> Step {
        self.event(Event::Key(key))
    }

    pub fn next_file(&mut self) -> Step {
        self.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
    }

    pub fn previous_file(&mut self) -> Step {
        self.send(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
    }

    /// Tab with the Shift modifier, as terminals that do not report BackTab send it.
    pub fn shift_tab(&mut self) -> Step {
        self.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT))
    }

    /// `key` with the Control modifier.
    pub fn control(&mut self, key: char) -> Step {
        self.send(KeyEvent::new(KeyCode::Char(key), KeyModifiers::CONTROL))
    }

    /// Ctrl-C.
    pub fn interrupt(&mut self) -> Step {
        self.control('c')
    }

    pub fn escape(&mut self) -> Step {
        self.send(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
    }

    pub fn resize(&mut self, size: Size) -> Step {
        self.resize_backend_only(size);
        self.event(Event::Resize(size.width, size.height))
    }

    /// Changes the terminal's size without sending the resize event that follows it.
    pub fn resize_backend_only(&mut self, size: Size) {
        self.terminal.backend_mut().resize(size.width, size.height);
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

    /// Whether `text` is drawn anywhere, matched cell by cell as `find` matches it.
    pub fn shows(&self, text: &str) -> bool {
        positions(self.terminal.backend().buffer(), text)
            .next()
            .is_some()
    }

    /// The marker and name foregrounds of the file row `row`, such as `"~ lib.rs"`; the
    /// name must be drawn in one color.
    pub fn file_row_colors(&self, row: &str) -> (Color, Color) {
        let (marker_label, _) = row.split_once(' ').expect("a marker, a space, and a name");
        let start = self.find(row);
        let buffer = self.buffer();
        let marker = buffer[(start.x, start.y)].fg;
        let name_start = start.x + cells(marker_label) + cells(" ");
        let name = (name_start..start.x + cells(row))
            .map(|x| buffer[(x, start.y)].fg)
            .collect::<Vec<_>>();

        assert!(
            name.windows(2).all(|pair| pair[0] == pair[1]),
            "{row:?} draws its name in one color: {name:?}"
        );
        (marker, name[0])
    }

    /// Screen rows whose first change label is an `old_N` label: with every diff-pane
    /// line of the before pane labeled, the lines that pane shows.
    pub fn old_label_lines(&self) -> usize {
        self.rows()
            .iter()
            .filter(|row| change_label(row).is_some_and(|label| label.starts_with("old_")))
            .count()
    }

    /// The screen row of the diff pane's middle line, where anchored rows stay: the
    /// pane's first content line plus half the lines the current view gives each diff
    /// pane.
    pub fn pane_middle_line(&self) -> usize {
        let screen = self.terminal.backend().buffer().area.as_size();
        let view = EffectiveView::of(self.tui.preferences(), screen);
        FIRST_CONTENT_LINE + view.pane_content().height.half().get()
    }

    /// Cell position of the first occurrence of `text`.
    pub fn find(&self, text: &str) -> Position {
        positions(&self.buffer(), text)
            .next()
            .unwrap_or_else(|| panic!("{text:?} is not on screen:\n{}", self.rows().join("\n")))
    }
}

/// Where `text` is drawn in `buffer`: each position starts a run of cells in one row that
/// draws `text`'s graphemes in order, a wide grapheme covering its cells.
pub fn positions<'a>(buffer: &'a Buffer, text: &'a str) -> impl Iterator<Item = Position> + 'a {
    let graphemes = text.graphemes(true).collect::<Vec<_>>();

    buffer.area.positions().filter(move |start| {
        let drawn_through = graphemes.iter().try_fold(start.x, |x, grapheme| {
            let drawn = x < buffer.area.right() && buffer[(x, start.y)].symbol() == *grapheme;
            drawn.then(|| x + cells(grapheme))
        });
        drawn_through.is_some()
    })
}

/// Screen columns `text` covers.
pub fn cells(text: &str) -> u16 {
    Cells::of_text(text).frame_columns()
}

#[derive(Debug, Clone, Copy)]
pub struct Mode {
    pub layout: DiffLayout,
    pub granularity: DiffGranularity,
}

impl Mode {
    pub const fn line(layout: DiffLayout) -> Self {
        Self {
            layout,
            granularity: DiffGranularity::Line,
        }
    }

    pub const fn character(layout: DiffLayout) -> Self {
        Self {
            layout,
            granularity: DiffGranularity::Character,
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

/// The part of a screen row right of its first `│`, its leading blanks trimmed: the diff
/// panes, while the file list is shown and draws its separator there.
pub fn right_of_first_separator(row: &str) -> String {
    row.split_once('│')
        .map_or(row, |(_, pane)| pane)
        .trim_start_matches(' ')
        .to_owned()
}
