//! Where a frame puts its parts: the file list, separators, diff panes, the help box, and
//! surrounding text.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use hdiff::preferences::{DiffGranularity, DiffLayout};
use ratatui::layout::Size;

use crate::harness::{Live, Mode, THEME, unified_line};

const TWO_FILES: &str = "--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n\
                         --- a/second\n+++ b/second\n@@ -1 +1 @@\n-before\n+after\n";

fn vertical_line() -> Mode {
    Mode {
        layout: DiffLayout::Vertical,
        detail: DiffGranularity::Line,
    }
}

fn next_file(live: &mut Live) {
    live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
}

/// The text of screen row `y` with trailing blanks trimmed.
fn row_text(live: &Live, y: usize) -> String {
    live.rows()[y].trim_end().to_owned()
}

/// architecture.md (Interaction model): a compact file list, one padded separator, and the
/// diff pane, with the help hint below the list.
#[test]
fn renders_compact_file_and_diff_panes_with_one_separator() {
    let mut live = Live::sized(TWO_FILES, unified_line(), Size::new(80, 10));
    next_file(&mut live);

    let rendered = live.buffer();
    let expected_symbols = [
        ((2, 1), "f", "first file"),
        ((24, 0), " ", "left separator padding"),
        ((25, 0), "│", "separator"),
        ((26, 0), " ", "right separator padding"),
        ((28, 1), " ", "metadata marker column"),
        ((29, 1), " ", "metadata marker spacing"),
        ((30, 1), "-", "metadata content"),
        ((0, 2), "~", "selected file's state marker"),
        ((28, 4), "-", "deletion marker"),
        ((28, 5), "+", "addition marker"),
        ((1, 9), "?", "help hint"),
    ];

    for ((x, y), expected, name) in expected_symbols {
        assert_eq!(rendered[(x, y)].symbol(), expected, "{name}");
    }
    assert_eq!(rendered[(2, 1)].fg, THEME.chrome.file_name, "file name");
    assert_eq!(rendered[(0, 9)].fg, THEME.chrome.hint, "help hint");
}

#[test]
fn collapses_the_file_list_before_the_diff_pane() {
    let live = Live::sized(
        "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n",
        unified_line(),
        Size::new(23, 8),
    );

    let rendered = live.buffer();
    assert_eq!(rendered[(1, 4)].symbol(), "-");
    assert_eq!(rendered[(2, 4)].symbol(), " ");
}

/// architecture.md (Interaction model): vertical panes sit on either side of a dimmer inner
/// separator, and a one-sided change pairs with a markerless alignment peer.
#[test]
fn renders_vertical_pairs_with_a_dimmer_inner_separator() {
    let live = Live::sized(
        "--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        vertical_line(),
        Size::new(80, 9),
    );

    let rendered = live.buffer();
    assert_eq!(rendered[(28, 4)].symbol(), "-", "before marker");
    assert_eq!(rendered[(55, 4)].symbol(), "+", "after marker");
    assert_eq!(rendered[(53, 4)].symbol(), "│", "inner separator");
    assert_eq!(rendered[(30, 6)].symbol(), "c", "before context payload");
    assert_eq!(rendered[(57, 6)].symbol(), "c", "after context payload");
    assert_eq!(rendered[(53, 6)].symbol(), "│", "context separator");
    assert_eq!(
        rendered[(55, 5)].symbol(),
        " ",
        "alignment peer has no marker"
    );
    assert_eq!(
        rendered[(53, 4)].fg,
        THEME.chrome.dim_separator,
        "inner separator is dimmer than the outer separator"
    );
}

/// architecture.md (Interaction model): one horizontal offset scrolls both vertical panes.
#[test]
fn scrolls_both_vertical_panes_by_one_horizontal_offset() {
    let tail = "_".repeat(40);
    let patch = format!("--- a/f\n+++ b/f\n@@ -1 +1 @@\n-abcdef{tail}\n+ABCDEF{tail}\n");
    let mut live = Live::sized(&patch, vertical_line(), Size::new(80, 8));

    live.press('l');
    live.press('l');

    let rendered = live.buffer();
    assert_eq!(rendered[(28, 4)].symbol(), "a", "before pane offset");
    assert_eq!(rendered[(55, 4)].symbol(), "A", "after pane offset");
}

/// architecture.md (Interaction model): a `? help` box floats over the bottom-left corner
/// of the diff while the file list is hidden, by `e` or by a narrow screen.
#[test]
fn floats_a_help_box_while_the_file_list_is_hidden() {
    let mut hidden = Live::sized(TWO_FILES, unified_line(), Size::new(80, 10));
    hidden.press('e');
    let narrow = Live::sized(TWO_FILES, unified_line(), Size::new(23, 10));

    for live in [&hidden, &narrow] {
        assert!(row_text(live, 7).starts_with("┌────────┐"));
        assert!(row_text(live, 8).starts_with("│ ? help │"));
        assert!(row_text(live, 9).starts_with("└────────┘"));
        assert_eq!(
            live.rows().concat().matches("? help").count(),
            1,
            "one help hint"
        );
    }
    assert_eq!(
        hidden.buffer()[(1, 4)].symbol(),
        "-",
        "deletion marker at the left edge"
    );
    assert!(!hidden.shows("second"), "no file list");
}

/// architecture.md (Core model): text around each file shows with that file, once per
/// pane, and the final text follows the last file.
#[test]
fn renders_surrounding_text_once_per_pane_for_each_file_occurrence() {
    let patch = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
    let input = format!("first message\n{patch}second message\n{patch}trailer\n");

    for (layout, copies) in [
        (DiffLayout::Unified, 1),
        (DiffLayout::Vertical, 2),
        (DiffLayout::Stacked, 2),
    ] {
        let mode = Mode {
            layout,
            detail: DiffGranularity::Line,
        };
        let mut live = Live::load(&input, mode);
        let text = |live: &Live| live.rows().concat();

        assert_eq!(
            text(&live).matches("first message").count(),
            copies,
            "{layout:?}"
        );
        assert!(!text(&live).contains("second message"), "{layout:?}");
        assert!(!text(&live).contains("trailer"), "{layout:?}");

        next_file(&mut live);
        assert_eq!(
            text(&live).matches("second message").count(),
            copies,
            "{layout:?}"
        );
        assert!(!text(&live).contains("first message"), "{layout:?}");
        assert_eq!(text(&live).matches("trailer").count(), copies, "{layout:?}");
    }
}

/// architecture.md (Input contract): comparison output that holds only surrounding text is
/// shown, and stays shown through movement and file rotation.
#[test]
fn shows_comparison_output_that_holds_only_text() {
    let mut live = Live::comparison("Binary files a.bin and b.bin differ\n", unified_line());

    live.press('j');
    next_file(&mut live);
    live.send(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));

    assert!(live.shows("Binary files a.bin and b.bin differ"));
}
