//! Movement, file rotation, layout and preference changes, resizing, and exiting.

use hdiff::{
    navigation::Step,
    parser::parse_patch,
    preferences::{ContextRecords, DiffGranularity, DiffLayout, FileList},
    prepared::PreparedDocument,
    terminal::Tui,
    theme::ColorDepth,
};
use ratatui::layout::Size;

use crate::{
    fixtures::{MIXED_LANGUAGES, context_patch, hidden_context_patch, long_patch, paired_patch},
    harness::{FIRST_CONTENT_LINE, Live, Mode, SCREEN, WIDTH, change_label},
};

/// The screen the resize scenarios shrink the terminal to.
const RESIZED: Size = Size::new(80, 20);

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
    key_before_resize_event,
    stale_resize_event,
    bottom_on_too_short_screen
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
        live.next_file();
        assert!(
            live.shows(expected),
            "Tab selects the file showing {expected}"
        );
    }

    live.previous_file();
    assert!(
        live.shows("const char *greeting(void)"),
        "Shift-Tab wraps to the last file"
    );
    live.shift_tab();
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
        let layout = live.preferences().layout;
        live.press('v');
        assert_eq!(live.preferences().layout, layout.next());
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
    assert_ne!(live.preferences().granularity, mode.granularity);
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

    assert_eq!(live.resize(RESIZED), Step::Continue);
    assert_eq!(live.buffer().area.as_size(), RESIZED);
    assert!(live.shows("pub fn greeting("));

    live.resize(SCREEN);
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
    assert_eq!(live.interrupt(), Step::Exit);
}

fn bottom_past_hidden_context(mode: Mode) {
    let mut live = Live::load(&hidden_context_patch(), mode);

    live.press('G');
    assert!(
        live.shows("new_39"),
        "G shows the last row when hidden context precedes it"
    );
}

/// docs/spec/architecture.md#interaction-model: `?` opens the keymap popup; while it is
/// open, `?`, `Esc`, and `q` close it without exiting, `Ctrl-C` exits, and every other
/// key does nothing.
fn keymap_popup(mode: Mode) {
    let mut live = Live::load(&long_patch(), mode);
    let start = live.buffer();
    assert_eq!(live.escape(), Step::Continue);
    assert_eq!(live.buffer(), start, "Esc does nothing without the popup");

    for close in ['?', 'q'] {
        live.press('?');
        assert!(live.shows("cycle layout"), "? opens the popup");
        let open = live.buffer();

        for key in ['j', 'G', 'v', 'c', 'w', '+', 'e', 'x'] {
            assert_eq!(live.press(key), Step::Continue);
        }
        assert_eq!(live.buffer(), open, "other keys do nothing while open");
        assert_eq!(live.preferences().layout, mode.layout);

        assert_eq!(live.press(close), Step::Continue, "{close} does not exit");
        assert_eq!(live.buffer(), start, "{close} closes the popup");
    }

    live.press('?');
    live.escape();
    assert_eq!(live.buffer(), start, "Esc closes the popup");

    live.press('?');
    assert_eq!(live.interrupt(), Step::Exit, "Ctrl-C exits from the popup");
}

/// docs/spec/architecture.md#keyboard-bindings: `Ctrl-D` and `Ctrl-U` move by half of a
/// diff pane's lines, so no line passes the pane unseen.
fn half_page_movement(mode: Mode) {
    let mut live = Live::load(&paired_patch(), mode);
    for _ in 0..10 {
        live.press('j');
    }
    let start = live.buffer();
    let top_label = |live: &Live| label_number(&live.rows()[FIRST_CONTENT_LINE]);
    let pane_lines = live.old_label_lines();
    let top = top_label(&live);

    live.control('d');
    assert_eq!(
        top_label(&live),
        top + pane_lines / 2,
        "Ctrl-D moves half a pane"
    );
    live.control('u');
    assert_eq!(live.buffer(), start, "Ctrl-U moves back");
}

/// docs/spec/architecture.md#viewport-coordinates: each frame adopts the size it is drawn
/// at, so a key that arrives between a size change and its resize event draws a full
/// frame.
fn key_before_resize_event(mode: Mode) {
    let mut resized = Live::load(MIXED_LANGUAGES, mode);
    resized.resize(RESIZED);
    resized.press('j');
    let expected = resized.buffer();

    let mut live = Live::load(MIXED_LANGUAGES, mode);
    live.resize_backend_only(RESIZED);
    live.press('j');
    assert_eq!(
        live.buffer(),
        expected,
        "the key draws the frame at the new size"
    );

    live.resize(RESIZED);
    assert_eq!(
        live.buffer(),
        expected,
        "the resize event that follows changes nothing"
    );
}

/// docs/spec/architecture.md#viewport-coordinates: the terminal is the one source of the
/// screen size, so a resize event naming a size the terminal no longer has moves nothing.
fn stale_resize_event(mode: Mode) {
    let mut resized = Live::load(&long_patch(), mode);
    resized.resize(RESIZED);
    resized.press('G');
    let expected = resized.buffer();

    let mut live = Live::load(&long_patch(), mode);
    live.resize(RESIZED);
    live.press('G');
    live.resize_event_only(SCREEN);
    assert_eq!(
        live.buffer(),
        expected,
        "a stale resize event keeps the view at the bottom"
    );
}

/// docs/spec/architecture.md#viewport-coordinates: the vertical offset indexes the screen
/// lines, so `G` on a screen too short for panes still names the last line when the
/// screen regrows.
fn bottom_on_too_short_screen(mode: Mode) {
    let mut full = Live::load(&long_patch(), mode);
    full.press('G');
    let expected = full.buffer();

    let mut live = Live::load(&long_patch(), mode);
    live.resize(Size::new(WIDTH, 2));
    live.press('G');
    live.resize(SCREEN);
    assert_eq!(
        live.buffer(),
        expected,
        "regrowing after G keeps the view at the bottom"
    );
}

/// The number of the `old_N` label on a screen row.
fn label_number(row: &str) -> usize {
    let label = change_label(row).expect("a labeled row");
    label
        .trim_start_matches("old_")
        .parse()
        .expect("an old_N label")
}

/// docs/spec/architecture.md#view-model: context changes keep the middle row on the
/// middle line.
#[test]
fn context_changes_keep_the_middle_row() {
    let mut live = Live::load(&hidden_context_patch(), Mode::line(DiffLayout::Unified));
    for _ in 0..14 {
        live.press('j');
    }
    let middle_line = live.pane_middle_line();
    let middle = live.rows()[middle_line].clone();

    live.press('+');
    assert_eq!(live.rows()[middle_line], middle, "+ keeps the middle row");
    live.press('-');
    assert_eq!(live.rows()[middle_line], middle, "- keeps the middle row");
}

/// A session starts in unified line mode. docs/spec/architecture.md#interaction-model:
/// the file list starts shown; docs/spec/architecture.md#view-model: three context
/// records show on either side of a change.
#[test]
fn preferences_start_in_unified_line_mode() {
    let tui = Tui::start(
        PreparedDocument::prepare(&parse_patch(MIXED_LANGUAGES.as_bytes()).expect("valid patch")),
        SCREEN,
        ColorDepth::TrueColor,
    );
    let start = tui.preferences();

    assert_eq!(start.layout, DiffLayout::Unified);
    assert_eq!(start.granularity, DiffGranularity::Line);
    assert_eq!(start.file_list, FileList::Shown);
    assert_eq!(start.context, ContextRecords::new(3));
}

/// docs/spec/architecture.md#lossless-and-safe-rendering and
/// docs/spec/architecture.md#viewport-coordinates: the document is never truncated, and
/// no offset type caps how far a row reaches, so `$` reaches the end of a row wider than
/// any terminal offset type.
#[test]
fn far_right_reaches_the_end_of_a_very_wide_row() {
    let patch = format!(
        "--- a/wide.min.js\n+++ b/wide.min.js\n@@ -0,0 +1 @@\n+start_{}_end\n",
        "x".repeat(usize::from(u16::MAX) + 1)
    );
    let mut live = Live::load(&patch, Mode::line(DiffLayout::Unified));

    live.press('$');
    assert!(live.shows("_end"), "$ shows the end of the row");
}
