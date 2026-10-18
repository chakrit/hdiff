//! Indentation, continuity markers, frames, and the minimum layout.

use hdiff::preferences::DiffLayout;

use crate::{
    fixtures::{INDENTED, long_patch},
    harness::{FIRST_CONTENT_LINE, Live, Mode, WIDTH, diff_pane},
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
    let mut live = Live::load(&long_patch(), Mode::line(DiffLayout::Unified));
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
        diff_pane(&rows[usize::from(wide.y)]).chars().last(),
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

    live.resize(80, 6);
    assert!(live.shows("screen too short"));
    live.resize(80, 7);
    assert_eq!(live.count('▾'), 2, "each pane shows a line");
    assert!(!live.shows("screen too short"));
}
