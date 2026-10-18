//! Movement, file rotation, layout and preference changes, resizing, and exiting.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use hdiff::{
    preferences::{DiffGranularity, DiffLayout, ViewPreferences},
    terminal::Step,
};

use crate::{
    fixtures::{MIXED_LANGUAGES, context_patch, hidden_context_patch, long_patch, paired_patch},
    harness::{FIRST_CONTENT_LINE, HEIGHT, Live, Mode, WIDTH, change_label, unified_line},
};

matrix!(
    vertical_movement,
    horizontal_movement,
    file_rotation,
    layout_cycle,
    detail_toggle,
    context_adjustment,
    resize,
    quit,
    bottom_past_hidden_context,
    keymap_popup,
    half_page_movement,
    key_before_resize_event
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

/// architecture.md (Viewport coordinates): each frame adopts the size it is drawn at, so a
/// key that arrives between a size change and its resize event draws a full frame.
fn key_before_resize_event(mode: Mode) {
    let mut resized = Live::load(MIXED_LANGUAGES, mode);
    resized.resize(80, 20);
    resized.press('j');
    let expected = resized.buffer();

    let mut live = Live::load(MIXED_LANGUAGES, mode);
    live.resize_backend_only(80, 20);
    live.press('j');
    assert_eq!(
        live.buffer(),
        expected,
        "the key draws the frame at the new size"
    );

    live.resize(80, 20);
    assert_eq!(
        live.buffer(),
        expected,
        "the resize event that follows changes nothing"
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

fn bottom_past_hidden_context(mode: Mode) {
    let mut live = Live::load(&hidden_context_patch(), mode);

    live.press('G');
    assert!(
        live.shows("new_39"),
        "G shows the last row when hidden context precedes it"
    );
}

#[test]
fn context_changes_keep_the_middle_row() {
    let mut live = Live::load(&hidden_context_patch(), unified_line());
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
fn preferences_start_in_unified_line_mode() {
    let live = Live::load(MIXED_LANGUAGES, unified_line());

    assert_eq!(live.tui.preferences(), ViewPreferences::INITIAL);
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

/// architecture.md (Keyboard bindings): `Ctrl-D` and `Ctrl-U` move by half of a diff
/// pane's lines, so no line passes the pane unseen.
fn half_page_movement(mode: Mode) {
    let mut live = Live::load(&paired_patch(), mode);
    for _ in 0..10 {
        live.press('j');
    }
    let start = live.buffer();
    let top_label = |live: &Live| label_number(&live.rows()[FIRST_CONTENT_LINE]);
    let pane_lines = live.old_label_lines();
    let top = top_label(&live);

    live.send(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_eq!(
        top_label(&live),
        top + pane_lines / 2,
        "Ctrl-D moves half a pane"
    );
    live.send(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(live.buffer(), start, "Ctrl-U moves back");
}

/// The number of the `old_N` label on a screen row.
fn label_number(row: &str) -> usize {
    let label = change_label(row).expect("a labeled row");
    label
        .trim_start_matches("old_")
        .parse()
        .expect("an old_N label")
}

/// architecture.md (Lossless and safe rendering): the document is never truncated, so `$`
/// reaches the end of a row wider than any terminal offset type.
#[test]
fn far_right_reaches_the_end_of_a_very_wide_row() {
    let patch = format!(
        "--- a/wide.min.js\n+++ b/wide.min.js\n@@ -0,0 +1 @@\n+start_{}_end\n",
        "x".repeat(70_000)
    );
    let mut live = Live::load(&patch, unified_line());

    live.press('$');
    assert!(live.shows("_end"), "$ shows the end of the row");
}
