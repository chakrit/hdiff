//! Indentation, continuity markers, frames, and the minimum layout.

use hdiff::preferences::DiffLayout;
use ratatui::layout::Size;

use crate::{
    fixtures::{INDENTED, long_patch},
    harness::{FIRST_CONTENT_LINE, Live, Mode, WIDTH, columns, right_of_first_separator},
};

matrix!(
    leading_spaces,
    leading_tabs,
    continuity_markers,
    no_markers_when_content_fits
);

fn leading_spaces(mode: Mode) {
    let live = Live::load(INDENTED, mode);
    let flush = live.find("added_flush").x;
    let indent = INDENTED
        .lines()
        .find_map(|line| line.strip_prefix('+')?.strip_suffix("added_spaces"))
        .expect("the fixture indents added_spaces");

    assert_eq!(live.find("added_spaces").x, flush + columns(indent));
}

fn leading_tabs(mode: Mode) {
    let live = Live::load(INDENTED, mode);
    let flush = live.find("added_flush").x;

    assert!(
        live.find("added_tab").x > flush,
        "a leading tab indents its row"
    );
}

/// docs/spec/architecture.md#continuity-markers: each marker shows only while content
/// continues past its edge.
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
        live.find("_end").x + columns("_end"),
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
    let mut live = Live::load(&long_patch(), Mode::line(DiffLayout::Unified));
    let rows = live.rows();
    let [top_frame, ..] = &rows[..] else {
        panic!("the screen has a top frame row");
    };
    let first_content = usize::from(live.find("--- a/long.rs").y);
    let wide = live.find("wide_");

    assert_eq!(
        first_content, FIRST_CONTENT_LINE,
        "the top frame row precedes the content"
    );
    assert!(
        top_frame.contains('│')
            && right_of_first_separator(top_frame)
                .chars()
                .all(|symbol| symbol == ' '),
        "the top frame row holds no content: {top_frame:?}"
    );
    assert_eq!(
        right_of_first_separator(&rows[usize::from(wide.y)])
            .chars()
            .last(),
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
    let mut live = Live::load(INDENTED, Mode::line(DiffLayout::Stacked));

    live.resize(Size::new(80, 6));
    assert!(live.shows("screen too short"));
    live.resize(Size::new(80, 7));
    assert_eq!(live.count('▾'), 2, "each pane shows a line");
    assert!(!live.shows("screen too short"));
}

/// docs/spec/architecture.md#continuity-markers: `▴` and `▾` sit at the center of their
/// frame rows and `◂` and `▸` at the middle of their frame columns. With the file list
/// hidden, the one unified pane fills the screen, so on a 121 by 37 screen its content
/// spans columns 1 to 119, centered on column 60, and rows 1 to 35, centered on row 18.
#[test]
fn continuity_markers_sit_at_the_center_of_their_frame_edges() {
    let mut live = Live::sized(
        &long_patch(),
        Mode::line(DiffLayout::Unified),
        Size::new(121, 37),
    );
    live.press('e');
    live.press('j');
    live.press('l');

    let at = |marker| {
        let position = live.find(marker);
        (position.x, position.y)
    };
    assert_eq!(at("▴"), (60, 0), "above the content's center column");
    assert_eq!(at("▾"), (60, 36), "below the content's center column");
    assert_eq!(at("◂"), (0, 18), "left of the content's middle row");
    assert_eq!(at("▸"), (120, 18), "right of the content's middle row");
}
