//! Row wrapping and the positions it keeps.

use hdiff::preferences::DiffLayout;

use crate::{
    fixtures::long_patch,
    harness::{FIRST_CONTENT_LINE, HEIGHT, Live, Mode, diff_pane},
};

matrix!(wrapping);

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

/// Count of `x` on screen.
fn x_count(live: &Live) -> usize {
    live.rows().iter().map(|row| row.matches('x').count()).sum()
}

#[test]
fn wrapping_starts_disabled() {
    let live = Live::load(&long_patch(), Mode::line(DiffLayout::Unified));

    assert!(!live.shows("_end"), "the wide row is clipped");
}

#[test]
fn wrapping_pads_the_shorter_side_of_a_pair() {
    let patch = format!(
        "--- a/pair.rs\n+++ b/pair.rs\n@@ -1 +1 @@\n-short\n+long_{}_end\n",
        "y".repeat(121)
    );
    let mut live = Live::load(&patch, Mode::line(DiffLayout::Vertical));

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
    let mut live = Live::load(&long_patch(), Mode::line(DiffLayout::Unified));
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
    let mut live = Live::load(&long_patch(), Mode::line(DiffLayout::Unified));
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
