//! Live interaction tests. Contract: docs/spec/architecture.md#interactive-loop; each
//! test drives a `Tui` one event at a time on a 120-by-36 `TestBackend` and asserts on
//! the buffer after each step.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use hdiff::{
    interaction::{DiffGranularity, DiffLayout, ViewPreferences},
    parser::parse_unified_diff,
    prepared::PreparedDocument,
    terminal::{Step, Tui},
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::{Position, Size},
};

const WIDTH: u16 = 120;
const HEIGHT: u16 = 36;
const MIXED_LANGUAGES: &str = include_str!("fixtures/mixed-language-layout.patch");

struct Live {
    tui: Tui,
    terminal: Terminal<TestBackend>,
}

impl Live {
    fn load(patch: &str, mode: Mode) -> Self {
        let document = parse_unified_diff(patch.as_bytes()).expect("valid fixture patch");
        let prepared = PreparedDocument::prepare(&document);
        let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("test terminal");
        let tui = Tui::new(prepared, Size::new(WIDTH, HEIGHT), u16::MAX);
        tui.draw(&mut terminal).expect("initial frame");

        let mut live = Self { tui, terminal };
        live.enter(mode);
        live
    }

    fn enter(&mut self, mode: Mode) {
        while self.tui.preferences().layout != mode.layout {
            self.press('v');
        }
        if self.tui.preferences().granularity != mode.detail {
            self.press('c');
        }
    }

    fn press(&mut self, key: char) -> Step {
        self.send(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))
    }

    fn send(&mut self, key: KeyEvent) -> Step {
        self.event(Event::Key(key))
    }

    fn resize(&mut self, width: u16, height: u16) -> Step {
        self.terminal.backend_mut().resize(width, height);
        self.event(Event::Resize(width, height))
    }

    fn event(&mut self, event: Event) -> Step {
        let Ok(step) = self.tui.step(&mut self.terminal, event);
        step
    }

    fn buffer(&self) -> Buffer {
        self.terminal.backend().buffer().clone()
    }

    fn rows(&self) -> Vec<String> {
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

    fn shows(&self, text: &str) -> bool {
        self.rows().iter().any(|row| row.contains(text))
    }

    /// Cell position of the first occurrence of `text`.
    fn find(&self, text: &str) -> Position {
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
struct Mode {
    layout: DiffLayout,
    detail: DiffGranularity,
}

macro_rules! matrix {
    ($($scenario:ident),* $(,)?) => {$(
        mod $scenario {
            use super::{DiffGranularity::*, DiffLayout::*, Mode};

            #[test]
            fn unified_line() {
                super::$scenario(Mode { layout: Unified, detail: Line });
            }

            #[test]
            fn unified_character() {
                super::$scenario(Mode { layout: Unified, detail: Character });
            }

            #[test]
            fn vertical_line() {
                super::$scenario(Mode { layout: Vertical, detail: Line });
            }

            #[test]
            fn vertical_character() {
                super::$scenario(Mode { layout: Vertical, detail: Character });
            }

            #[test]
            fn stacked_line() {
                super::$scenario(Mode { layout: Stacked, detail: Line });
            }

            #[test]
            fn stacked_character() {
                super::$scenario(Mode { layout: Stacked, detail: Character });
            }
        }
    )*};
}

matrix!(
    vertical_movement,
    horizontal_movement,
    file_rotation,
    layout_cycle,
    detail_toggle,
    context_adjustment,
    resize,
    quit,
    leading_spaces,
    leading_tabs,
);

fn vertical_movement(mode: Mode) {
    let mut live = Live::load(&long_patch(), mode);
    let top = live.buffer();

    live.press('j');
    assert_ne!(live.buffer(), top, "j scrolls down");
    live.press('k');
    assert_eq!(live.buffer(), top, "k scrolls back");

    live.press('G');
    assert!(live.shows("row_199"), "G reaches the end");
    live.press('g');
    assert_eq!(live.buffer(), top, "g returns to the top");
}

fn horizontal_movement(mode: Mode) {
    let mut live = Live::load(&long_patch(), mode);
    let start = live.buffer();
    let before = live.find("wide_");

    live.press('l');
    assert_eq!(
        live.find("ide_").x,
        before.x,
        "l shifts content one column left"
    );
    live.press('h');
    assert_eq!(live.buffer(), start, "h shifts back");

    live.press('$');
    assert!(live.shows("_end"), "$ reveals the end of the widest row");
    live.press('0');
    assert_eq!(live.buffer(), start, "0 returns to the first column");
}

fn file_rotation(mode: Mode) {
    let mut live = Live::load(MIXED_LANGUAGES, mode);
    assert!(live.shows("pub fn greeting()"));

    for expected in [
        "export const greeting",
        "def greeting()",
        "func greeting() string",
        "const char *greeting(void)",
        "pub fn greeting()",
    ] {
        live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(
            live.shows(expected),
            "Tab selects the file showing {expected}"
        );
    }

    live.send(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(
        live.shows("const char *greeting(void)"),
        "Shift-Tab wraps to the last file"
    );
    live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));
    assert!(
        live.shows("func greeting() string"),
        "Shift-Tab selects the previous file"
    );
}

fn layout_cycle(mode: Mode) {
    let mut live = Live::load(MIXED_LANGUAGES, mode);
    let start = live.buffer();
    let old = "pub fn greeting() -> &'static str {";
    let new = "pub fn greeting(name: &str) -> String {";

    let side_by_side = live.find(old).y == live.find(new).y;
    assert_eq!(side_by_side, mode.layout == DiffLayout::Vertical);

    for _ in 0..3 {
        let layout = live.tui.preferences().layout;
        live.press('v');
        assert_eq!(live.tui.preferences().layout, layout.next());
        assert!(live.shows(old) && live.shows(new));
    }
    assert_eq!(
        live.buffer(),
        start,
        "three presses of v return to the same layout"
    );
}

fn detail_toggle(mode: Mode) {
    let mut live = Live::load(MIXED_LANGUAGES, mode);
    let start = live.buffer();

    live.press('c');
    assert_ne!(live.tui.preferences().granularity, mode.detail);
    assert_ne!(live.buffer(), start, "c changes change emphasis");
    live.press('c');
    assert_eq!(live.buffer(), start, "c toggles back");
}

fn context_adjustment(mode: Mode) {
    let mut live = Live::load(&context_patch(), mode);
    assert!(live.shows("context_3") && !live.shows("context_4"));

    live.press('+');
    assert!(live.shows("context_4"), "+ widens context by one record");
    live.press('-');
    live.press('-');
    assert!(
        live.shows("context_2") && !live.shows("context_3"),
        "- narrows context"
    );
    for _ in 0..3 {
        live.press('-');
    }
    assert!(
        !live.shows("context_1") && live.shows("changed_new"),
        "context stops at zero"
    );
}

fn resize(mode: Mode) {
    let mut live = Live::load(MIXED_LANGUAGES, mode);
    let start = live.buffer();

    assert_eq!(live.resize(80, 20), Step::Continue);
    assert_eq!(live.rows().len(), 20);
    assert!(live.rows().iter().all(|row| row.chars().count() == 80));
    assert!(live.shows("pub fn greeting("));

    live.resize(WIDTH, HEIGHT);
    assert_eq!(
        live.buffer(),
        start,
        "restoring the size restores the frame"
    );
}

fn quit(mode: Mode) {
    let mut live = Live::load(MIXED_LANGUAGES, mode);
    assert_eq!(live.press('x'), Step::Continue, "unmapped keys are ignored");
    assert_eq!(live.press('q'), Step::Exit);

    let mut live = Live::load(MIXED_LANGUAGES, mode);
    let interrupt = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(live.send(interrupt), Step::Exit);
}

fn leading_spaces(mode: Mode) {
    let live = Live::load(INDENTED, mode);
    let flush = live.find("added_flush").x;

    assert_eq!(live.find("added_spaces").x, flush + 4);
}

fn leading_tabs(mode: Mode) {
    let live = Live::load(INDENTED, mode);
    let flush = live.find("added_flush").x;

    assert!(
        live.find("added_tab").x > flush,
        "a leading tab indents its row"
    );
}

const INDENTED: &str = "--- a/indent.rs\n+++ b/indent.rs\n@@ -1 +1,3 @@\n-removed\n\
                        +added_flush\n+    added_spaces\n+\tadded_tab\n";

/// 200 added rows with one row wider than every pane.
fn long_patch() -> String {
    let rows = (0..200)
        .map(|row| match row {
            1 => format!("+wide_{}_end\n", "x".repeat(200)),
            _ => format!("+row_{row}\n"),
        })
        .collect::<String>();
    format!("--- a/long.rs\n+++ b/long.rs\n@@ -0,0 +1,200 @@\n{rows}")
}

/// One change followed by six context records.
fn context_patch() -> String {
    let context = (1..=6)
        .map(|row| format!(" context_{row}\n"))
        .collect::<String>();
    format!("--- a/ctx.rs\n+++ b/ctx.rs\n@@ -1,7 +1,7 @@\n-changed_old\n+changed_new\n{context}")
}

#[test]
fn preferences_start_in_unified_line_mode() {
    let live = Live::load(
        MIXED_LANGUAGES,
        Mode {
            layout: DiffLayout::Unified,
            detail: DiffGranularity::Line,
        },
    );

    assert_eq!(
        live.tui.preferences(),
        ViewPreferences::line(DiffLayout::Unified)
    );
}
