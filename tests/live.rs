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
    style::Color,
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

    /// The file-list column: each row's text left of the pane separator.
    fn file_list(&self) -> Vec<String> {
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
    fn background(&self, text: &str) -> Color {
        let position = self.find(text);
        self.buffer()[(position.x, position.y)].bg
    }

    fn shows(&self, text: &str) -> bool {
        self.rows().iter().any(|row| row.contains(text))
    }

    /// The marker and name foregrounds of the file row `row`, such as `"~ lib.rs"`; the
    /// name must be drawn in one color.
    fn file_row_colors(&self, row: &str) -> (Color, Color) {
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
    bottom_past_hidden_context,
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
    assert!(
        live.shows("func greeting() string"),
        "cmd/sample.go sorts first"
    );

    for expected in [
        "const char *greeting(void)",
        "pub fn greeting()",
        "def greeting()",
        "export const greeting",
        "func greeting() string",
    ] {
        live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(
            live.shows(expected),
            "Tab selects the file showing {expected}"
        );
    }

    live.send(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(
        live.shows("export const greeting"),
        "Shift-Tab wraps to the last file"
    );
    live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));
    assert!(
        live.shows("def greeting()"),
        "Shift-Tab selects the previous file"
    );
}

fn layout_cycle(mode: Mode) {
    let mut live = Live::load(MIXED_LANGUAGES, mode);
    let start = live.buffer();
    let old = "func greeting() string {";
    let new = "func greeting(name string) string {";

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
    assert!(live.shows("func greeting("));

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

fn bottom_past_hidden_context(mode: Mode) {
    let mut live = Live::load(&hidden_context_patch(), mode);

    live.press('G');
    assert!(
        live.shows("new_39"),
        "G shows the last row when hidden context precedes it"
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

/// Forty hunks, each with eight leading context records, five of them hidden at the
/// default context count.
fn hidden_context_patch() -> String {
    let hunks = (0..40)
        .map(|hunk| {
            let start = hunk * 20 + 1;
            let context = (1..=8)
                .map(|row| format!(" context_{hunk}_{row}\n"))
                .collect::<String>();
            format!("@@ -{start},9 +{start},9 @@\n{context}-old_{hunk}\n+new_{hunk}\n")
        })
        .collect::<String>();
    format!("--- a/hidden.rs\n+++ b/hidden.rs\n{hunks}")
}

#[test]
fn context_changes_keep_the_middle_row() {
    let mut live = Live::load(
        &hidden_context_patch(),
        Mode {
            layout: DiffLayout::Unified,
            detail: DiffGranularity::Line,
        },
    );
    for _ in 0..14 {
        live.press('j');
    }
    // architecture.md: context changes keep the middle row on the middle line.
    let middle_line = usize::from(HEIGHT / 2);
    let middle = live.rows()[middle_line].clone();

    live.press('+');
    assert_eq!(live.rows()[middle_line], middle, "+ keeps the middle row");
    live.press('-');
    assert_eq!(live.rows()[middle_line], middle, "- keeps the middle row");
}

#[test]
fn keeps_a_selection_beyond_the_file_list_height_visible() {
    let patch = (0..40)
        .map(|file| {
            format!("--- a/file_{file:02}.rs\n+++ b/file_{file:02}.rs\n@@ -1 +1 @@\n-old\n+new\n")
        })
        .collect::<String>();
    let mut live = Live::load(
        &patch,
        Mode {
            layout: DiffLayout::Unified,
            detail: DiffGranularity::Line,
        },
    );

    for _ in 0..30 {
        live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    assert!(live.shows("~ file_30.rs"), "the selected file stays listed");
    assert!(
        !live.shows("file_00.rs"),
        "the list scrolls past the first file"
    );
}

const TREE_PATCH: &str = "\
diff --git a/src/terminal/view.rs b/src/terminal/view.rs
index 1111111..2222222 100644
--- a/src/terminal/view.rs
+++ b/src/terminal/view.rs
@@ -1 +1 @@
-view_old
+view_new
diff --git a/README.md b/README.md
deleted file mode 100644
--- a/README.md
+++ /dev/null
@@ -1 +0,0 @@
-readme_gone
diff --git a/src/new.rs b/src/new.rs
new file mode 100644
--- /dev/null
+++ b/src/new.rs
@@ -0,0 +1 @@
+added_file
diff --git a/old.sh b/bin/run.sh
similarity index 100%
rename from old.sh
rename to bin/run.sh
diff --git a/src/lib.rs b/src/lib.rs
old mode 100644
new mode 100755
";

fn unified_line() -> Mode {
    Mode {
        layout: DiffLayout::Unified,
        detail: DiffGranularity::Line,
    }
}

#[test]
fn lists_files_under_directory_headings_in_git_order() {
    let live = Live::load(TREE_PATCH, unified_line());
    let listed = [
        "- README.md",
        "bin/",
        "* run.sh",
        "src/",
        "~ lib.rs",
        "+ new.rs",
        "src/terminal/",
        "~ view.rs",
    ];

    assert_eq!(live.file_list()[..listed.len()], listed);
}

#[test]
fn tab_moves_between_files_and_skips_headings() {
    let mut live = Live::load(TREE_PATCH, unified_line());
    assert!(
        live.shows("readme_gone"),
        "the first sorted file is selected"
    );

    let unselected = live.background("src/terminal/");
    let mut previous = "- README.md";
    assert_ne!(
        live.background(previous),
        unselected,
        "README.md is highlighted"
    );

    for (expected_diff, expected_row) in [
        ("rename to bin/run.sh", "* run.sh"),
        ("new mode 100755", "~ lib.rs"),
        ("added_file", "+ new.rs"),
        ("view_new", "~ view.rs"),
        ("readme_gone", "- README.md"),
    ] {
        live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(live.shows(expected_diff), "Tab shows {expected_diff}");
        assert_ne!(
            live.background(expected_row),
            unselected,
            "{expected_row} is highlighted"
        );
        assert_eq!(
            live.background(previous),
            unselected,
            "{previous} is no longer highlighted"
        );
        previous = expected_row;
    }
    for heading in ["bin/", "src/", "src/terminal/"] {
        assert_eq!(
            live.background(heading),
            unselected,
            "{heading} is never selected"
        );
    }

    live.send(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(live.shows("view_new"), "Shift-Tab wraps to the last file");
}

/// Contract: docs/spec/architecture.md#file-tree (markers take their state's color).
#[test]
fn colors_markers_by_state() {
    let mut live = Live::load(TREE_PATCH, unified_line());
    live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    let (deleted, name) = live.file_row_colors("- README.md");
    let (modified, lib_name) = live.file_row_colors("~ lib.rs");
    let (added, new_name) = live.file_row_colors("+ new.rs");
    let (view_marker, view_name) = live.file_row_colors("~ view.rs");

    assert_eq!(view_marker, modified, "one color per state");
    assert_ne!(deleted, modified, "deleted and modified differ");
    assert_ne!(deleted, added, "deleted and added differ");
    assert_ne!(modified, added, "modified and added differ");

    assert_eq!(
        [lib_name, new_name, view_name],
        [name; 3],
        "every name shares one color"
    );
    assert_ne!(name, deleted, "names are not colored by state");
}

#[test]
fn groups_files_by_commit_under_commit_headings() {
    let file = |path: &str, marker: &str| {
        format!(
            "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+{marker}\n"
        )
    };
    let patch = format!(
        "commit 0123456789abcdef0123456789abcdef01234567\nAuthor: A <a@example.com>\n\n    Newer change\n\n{}{}\
         commit fedcba9876543210fedcba9876543210fedcba98\nAuthor: A <a@example.com>\n\n    Older change\n\n{}",
        file("src/z.rs", "newer_z"),
        file("src/a.rs", "newer_a"),
        file("src/m.rs", "older_m"),
    );
    let mut live = Live::load(&patch, unified_line());
    let listed = [
        "0123456 Newer change",
        "src/",
        "~ a.rs",
        "~ z.rs",
        "fedcba9 Older change",
        "src/",
        "~ m.rs",
    ];

    assert_eq!(live.file_list()[..listed.len()], listed);
    for expected in ["newer_a", "newer_z", "older_m"] {
        assert!(live.shows(expected), "{expected} in commit order");
        live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
}

#[test]
fn keeps_a_selection_below_its_headings_visible() {
    let patch = (0..30)
        .map(|file| {
            let path = format!("dir_{file:02}/file_{file:02}.rs");
            format!("--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n")
        })
        .collect::<String>();
    let mut live = Live::load(&patch, unified_line());

    for _ in 0..20 {
        live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    let list = live.file_list();
    let selected = list
        .iter()
        .position(|row| row == "~ file_20.rs")
        .expect("the selected file stays listed below its heading rows");
    assert_eq!(list[selected - 1], "dir_20/");
    assert!(
        !live.shows("dir_00/"),
        "the list scrolls past earlier headings"
    );
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
