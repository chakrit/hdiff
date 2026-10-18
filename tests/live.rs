//! Live interaction tests. Contract: docs/spec/architecture.md#interactive-loop; each
//! test drives a `Tui` one event at a time on a 120-by-36 `TestBackend` and asserts on
//! the buffer after each step.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use hdiff::{
    interaction::{DiffGranularity, DiffLayout, ViewPreferences},
    layout::content_pane_height,
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
/// The screen row of a diff pane's first content line, below its top frame row.
const FIRST_CONTENT_LINE: usize = 1;
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

    fn count(&self, symbol: char) -> usize {
        self.rows()
            .iter()
            .map(|row| row.matches(symbol).count())
            .sum()
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
    wrapping,
    continuity_markers,
    no_markers_when_content_fits,
    file_list_toggle,
    keymap_popup,
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
        live.shows("pub fn greeting()"),
        "src/sample.rs, the first input file, is selected"
    );

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

    for _ in 0..36 {
        live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    assert!(live.shows("~ file_36.rs"), "the selected file stays listed");
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
fn lists_top_level_files_first_then_directories_in_input_order() {
    let live = Live::load(TREE_PATCH, unified_line());
    let listed = [
        "- README.md",
        "src/terminal/",
        "~ view.rs",
        "src/",
        "+ new.rs",
        "bin/",
        "* run.sh",
        "src/",
        "~ lib.rs",
    ];

    let rows = FIRST_CONTENT_LINE..FIRST_CONTENT_LINE + listed.len();
    assert_eq!(live.file_list()[rows], listed);
}

#[test]
fn tab_moves_between_files_and_skips_headings() {
    let mut live = Live::load(TREE_PATCH, unified_line());
    assert!(
        live.shows("readme_gone"),
        "the first listed file is selected"
    );

    let unselected = live.background("src/terminal/");
    let mut previous = "- README.md";
    assert_ne!(
        live.background(previous),
        unselected,
        "README.md is highlighted"
    );

    for (expected_diff, expected_row) in [
        ("view_new", "~ view.rs"),
        ("added_file", "+ new.rs"),
        ("rename to bin/run.sh", "* run.sh"),
        ("new mode 100755", "~ lib.rs"),
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
    assert!(
        live.shows("new mode 100755"),
        "Shift-Tab wraps to the last file"
    );
}

/// Contract: docs/spec/architecture.md#file-tree (markers take their state's color).
#[test]
fn colors_markers_by_state() {
    let mut live = Live::load(TREE_PATCH, unified_line());
    live.send(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    live.send(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));

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
fn marks_files_beyond_the_file_list() {
    let patch = (0..40)
        .map(|file| {
            format!("--- a/file_{file:02}.rs\n+++ b/file_{file:02}.rs\n@@ -1 +1 @@\n-old\n+new\n")
        })
        .collect::<String>();
    let mut live = Live::load(&patch, unified_line());
    let markers = |live: &Live| {
        let list = live.file_list().join("");
        (list.matches('▴').count(), list.matches('▾').count())
    };

    // architecture.md (Continuity markers): the file list marks rows past each edge.
    assert_eq!(live.file_list()[FIRST_CONTENT_LINE], "~ file_00.rs");
    assert_eq!(markers(&live), (0, 1), "files below the list");
    for _ in 0..36 {
        live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    assert_eq!(markers(&live), (1, 1), "files above and below");
    // Back past the first file, wrapping to the last.
    for _ in 0..37 {
        live.send(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    }
    assert!(live.shows("~ file_39.rs"));
    assert_eq!(markers(&live), (1, 0), "the last file ends the list");
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

fn wrapping(mode: Mode) {
    let mut live = Live::load(&long_patch(), mode);
    let start = live.buffer();

    live.press('w');
    live.press('g');
    let wrapped = live.buffer();
    assert!(
        live.shows("wide_") && live.shows("_end"),
        "w wraps the wide row"
    );
    let shown_x = x_count(&live);
    live.press('G');
    assert_eq!(
        shown_x - x_count(&live),
        200,
        "every grapheme of the wide row shows once"
    );
    live.press('g');
    live.press('l');
    live.press('$');
    assert_eq!(
        live.buffer(),
        wrapped,
        "wrapped rows do not scroll horizontally"
    );

    live.press('w');
    assert_eq!(live.buffer(), start, "w restores unwrapped rows");
}

/// architecture.md (Continuity markers): each marker shows only while content continues
/// past its edge.
fn continuity_markers(mode: Mode) {
    let mut live = Live::load(&long_patch(), mode);
    let panes = match mode.layout {
        DiffLayout::Unified => 1,
        DiffLayout::Vertical | DiffLayout::Stacked => 2,
    };

    assert_eq!(live.count('▾'), panes, "lines below each pane");
    assert_eq!(live.count('▴'), 0, "nothing above the top");
    assert_eq!(live.count('◂'), 0, "nothing left of the first column");
    assert_eq!(live.count('▸'), 1, "the wide row continues right");

    live.press('j');
    assert_eq!(live.count('▴'), panes, "a line above each pane");
    assert_eq!(live.count('▾'), panes);

    live.press('G');
    assert_eq!(live.count('▴'), panes);
    assert_eq!(live.count('▾'), 0, "nothing below the bottom");

    live.press('g');
    live.press('l');
    assert_eq!(
        live.count('◂'),
        panes,
        "one marker per pane however many scrolled lines have cells to their left"
    );
    assert_eq!(live.count('▸'), 1);

    live.press('$');
    assert_eq!(
        live.find("_end").x + 4,
        WIDTH - 1,
        "the widest row ends beside the right frame column"
    );
    assert_eq!(live.count('▸'), 0, "$ shows the end of the widest row");

    live.press('w');
    assert_eq!(
        live.count('◂') + live.count('▸'),
        0,
        "wrapped rows never clip"
    );
}

fn no_markers_when_content_fits(mode: Mode) {
    let live = Live::load(INDENTED, mode);

    for marker in ['▴', '▾', '◂', '▸'] {
        assert_eq!(live.count(marker), 0, "{marker}");
    }
}

#[test]
fn frame_cells_never_hold_diff_content() {
    let mut live = Live::load(&long_patch(), unified_line());
    let rows = live.rows();
    let row_of = |text: &str| rows.iter().position(|row| row.contains(text));
    let first_content = row_of("--- a/long.rs").expect("file header");
    let wide = live.find("wide_");

    assert_eq!(
        first_content, FIRST_CONTENT_LINE,
        "the top frame row precedes the content"
    );
    assert!(
        rows[0].chars().skip(27).all(|cell| cell == ' '),
        "the top frame row holds no content: {:?}",
        rows[0]
    );
    assert_eq!(
        diff_pane(&rows[wide.y as usize]).chars().last(),
        Some(' '),
        "the right frame column holds no row text"
    );

    live.press('l');
    assert_eq!(
        live.find("ide_").x,
        wide.x,
        "content scrolls under a fixed frame"
    );
}

#[test]
fn stacked_panes_need_room_for_one_content_line_each() {
    let mut live = Live::load(
        INDENTED,
        Mode {
            layout: DiffLayout::Stacked,
            detail: DiffGranularity::Line,
        },
    );

    live.resize(80, 6);
    assert!(live.shows("screen too short"));
    live.resize(80, 7);
    assert_eq!(live.count('▾'), 2, "each pane shows a line");
    assert!(!live.shows("screen too short"));
}

/// Count of `x` on screen.
fn x_count(live: &Live) -> usize {
    live.rows().iter().map(|row| row.matches('x').count()).sum()
}

#[test]
fn wrapping_starts_disabled() {
    let live = Live::load(&long_patch(), unified_line());

    assert!(!live.shows("_end"), "the wide row is clipped");
}

#[test]
fn wrapping_pads_the_shorter_side_of_a_pair() {
    let patch = format!(
        "--- a/pair.rs\n+++ b/pair.rs\n@@ -1 +1 @@\n-short\n+long_{}_end\n",
        "y".repeat(121)
    );
    let mut live = Live::load(
        &patch,
        Mode {
            layout: DiffLayout::Vertical,
            detail: DiffGranularity::Line,
        },
    );

    live.press('w');
    let short = live.find("short");
    let end = live.find("_end");
    let below_short = live.buffer()[(short.x, end.y)].clone();

    assert!(end.y > short.y, "the long side wraps below the short side");
    assert_eq!(below_short.symbol(), " ");
    assert_eq!(
        below_short.bg,
        live.background("short"),
        "the short side pads in its own row style"
    );
}

#[test]
fn resizing_keeps_the_top_line_of_a_wrapped_record() {
    let mut live = Live::load(&long_patch(), unified_line());
    live.press('w');
    live.press('g');
    // Lines: three headers, row_0, then the wide row's first and second lines.
    for _ in 0..5 {
        live.press('j');
    }
    let top = diff_pane(&live.rows()[FIRST_CONTENT_LINE]);
    assert!(top.starts_with('x'), "the wide row's second line: {top:?}");

    live.resize(100, 20);

    // architecture.md: resizing keeps the top record on the same line within it.
    let top = diff_pane(&live.rows()[FIRST_CONTENT_LINE]);
    assert!(top.starts_with('x'), "still the second line: {top:?}");
    assert!(!top.contains("wide_") && !top.starts_with('+'));
}

#[test]
fn toggling_wrapping_keeps_the_middle_row() {
    let mut live = Live::load(&long_patch(), unified_line());
    for _ in 0..20 {
        live.press('j');
    }
    // architecture.md: toggling wrapping keeps the middle row on the middle line.
    let middle_line = usize::from(HEIGHT / 2);
    let middle = live.rows()[middle_line].clone();

    live.press('w');
    assert_eq!(live.rows()[middle_line], middle, "w keeps the middle row");
    live.press('w');
    assert_eq!(
        live.rows()[middle_line],
        middle,
        "w back keeps the middle row"
    );
}

/// The diff pane's part of a screen row, right of the file-list separator.
fn diff_pane(row: &str) -> String {
    row.split_once('│')
        .map_or(row, |(_, pane)| pane)
        .trim_start_matches(' ')
        .to_owned()
}

/// One block of paired changes, so every layout shows a labeled row on each pane line.
fn paired_patch() -> String {
    let removed = (0..100)
        .map(|row| format!("-old_{row}\n"))
        .collect::<String>();
    let added = (0..100)
        .map(|row| format!("+new_{row}\n"))
        .collect::<String>();
    format!("--- a/pairs.rs\n+++ b/pairs.rs\n@@ -1,100 +1,100 @@\n{removed}{added}")
}

/// The first `old_N` or `new_N` label in `row`.
fn change_label(row: &str) -> Option<String> {
    let start = row.find("old_").or_else(|| row.find("new_"))?;
    let label = row[start..]
        .chars()
        .take_while(|symbol| symbol.is_alphanumeric() || *symbol == '_')
        .collect();

    Some(label)
}

/// architecture.md (Interaction model, View model): `e` hides and shows the file list,
/// floating a `? help` box while it is hidden and keeping the middle line's row in place.
fn file_list_toggle(mode: Mode) {
    let mut live = Live::load(&paired_patch(), mode);
    for _ in 0..20 {
        live.press('j');
    }
    let start = live.buffer();
    let pane_height = content_pane_height(usize::from(HEIGHT), mode.layout);
    let middle_line = FIRST_CONTENT_LINE + pane_height / 2;
    let middle = change_label(&live.rows()[middle_line]);
    assert!(middle.is_some(), "a labeled row on the middle line");
    assert!(live.shows("pairs.rs"), "the file list starts shown");

    live.press('e');
    assert!(!live.shows("pairs.rs"), "e hides the file list");
    assert_eq!(change_label(&live.rows()[middle_line]), middle);
    assert!(live.shows("? help"), "the help hint floats in a box");

    live.press('e');
    assert_eq!(live.buffer(), start, "e shows the file list again");
}

/// architecture.md (Interaction model): `?` opens the keymap popup; while it is open,
/// `?`, `Esc`, and `q` close it without exiting, `Ctrl-C` exits, and every other key does
/// nothing.
fn keymap_popup(mode: Mode) {
    let mut live = Live::load(&long_patch(), mode);
    let start = live.buffer();
    let escape = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let interrupt = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(live.send(escape), Step::Continue);
    assert_eq!(live.buffer(), start, "Esc does nothing without the popup");

    for close in ['?', 'q'] {
        live.press('?');
        assert!(live.shows("cycle layout"), "? opens the popup");
        let open = live.buffer();

        for key in ['j', 'G', 'v', 'c', 'w', '+', 'e', 'x'] {
            assert_eq!(live.press(key), Step::Continue);
        }
        assert_eq!(live.buffer(), open, "other keys do nothing while open");
        assert_eq!(live.tui.preferences().layout, mode.layout);

        assert_eq!(live.press(close), Step::Continue, "{close} does not exit");
        assert_eq!(live.buffer(), start, "{close} closes the popup");
    }

    live.press('?');
    live.send(escape);
    assert_eq!(live.buffer(), start, "Esc closes the popup");

    live.press('?');
    assert_eq!(
        live.send(interrupt),
        Step::Exit,
        "Ctrl-C exits from the popup"
    );
}
