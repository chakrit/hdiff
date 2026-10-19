//! Row wrapping and the positions it keeps.

use hdiff::preferences::DiffLayout;
use ratatui::layout::Size;

use crate::{
    fixtures::{WIDE_ROW_GRAPHEMES, long_patch},
    harness::{FIRST_CONTENT_LINE, Live, Mode, WIDTH, right_of_first_separator},
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
    let shown_x = live.count('x');
    live.press('G');
    assert_eq!(
        shown_x - live.count('x'),
        WIDE_ROW_GRAPHEMES,
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

#[test]
fn wrapping_starts_disabled() {
    let live = Live::load(&long_patch(), Mode::line(DiffLayout::Unified));

    assert!(!live.shows("_end"), "the wide row is clipped");
}

#[test]
fn wrapping_pads_the_shorter_side_of_a_pair() {
    let patch = format!(
        "--- a/pair.rs\n+++ b/pair.rs\n@@ -1 +1 @@\n-short\n+long_{}_end\n",
        "y".repeat(usize::from(WIDTH) + 1)
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

/// docs/spec/architecture.md#view-model: resizing keeps the top record on the same line
/// within it.
#[test]
fn resizing_keeps_the_top_line_of_a_wrapped_record() {
    let mut live = Live::load(&long_patch(), Mode::line(DiffLayout::Unified));
    live.press('w');
    live.press('g');
    let top_line = |live: &Live| right_of_first_separator(&live.rows()[FIRST_CONTENT_LINE]);
    // The wide row's first line opens with its marker; its continuation opens with `x`.
    // The wide row is the page's second record, so its continuation starts on the first
    // screen, fewer presses away than the screen has rows.
    let screen_rows = live.rows().len();
    let mut presses = 0;
    while !top_line(&live).starts_with('x') {
        assert!(
            presses < screen_rows,
            "scrolls to the wide row's second line within a screen: {:?}",
            top_line(&live)
        );
        live.press('j');
        presses += 1;
    }

    live.resize(Size::new(100, 20));

    let top = top_line(&live);
    assert!(top.starts_with('x'), "still the second line: {top:?}");
    assert!(!top.contains("wide_") && !top.starts_with('+'));
}

/// docs/spec/architecture.md#view-model: toggling wrapping keeps the middle row on the
/// middle line.
#[test]
fn toggling_wrapping_keeps_the_middle_row() {
    let mut live = Live::load(&long_patch(), Mode::line(DiffLayout::Unified));
    for _ in 0..20 {
        live.press('j');
    }
    let middle_line = live.pane_middle_line();
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
