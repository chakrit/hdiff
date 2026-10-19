//! The file list: its order, selection, markers, and showing and hiding it.

use hdiff::preferences::DiffLayout;

use crate::{
    fixtures::{TREE_PATCH, files_in_directories, many_files, paired_patch},
    harness::{FIRST_CONTENT_LINE, Live, Mode, change_label},
};

/// How many continuity markers the file list shows above and below its rows.
#[derive(Debug, PartialEq, Eq)]
struct EdgeMarkers {
    above: usize,
    below: usize,
}

matrix!(file_list_toggle);

/// docs/spec/architecture.md#interaction-model and docs/spec/architecture.md#view-model:
/// `e` hides and shows the file list, floating a `? help` box while it is hidden and
/// keeping the middle line's row in place.
fn file_list_toggle(mode: Mode) {
    let mut live = Live::load(&paired_patch(), mode);
    for _ in 0..20 {
        live.press('j');
    }
    let start = live.buffer();
    let middle_line = live.pane_middle_line();
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

#[test]
fn keeps_a_selection_beyond_the_file_list_height_visible() {
    let mut live = Live::load(&many_files(40), Mode::line(DiffLayout::Unified));

    for _ in 0..36 {
        live.next_file();
    }
    assert!(live.shows("~ file_36.rs"), "the selected file stays listed");
    assert!(
        !live.shows("file_00.rs"),
        "the list scrolls past the first file"
    );
}

#[test]
fn lists_top_level_files_first_then_directories_in_input_order() {
    let live = Live::load(TREE_PATCH, Mode::line(DiffLayout::Unified));
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
    assert_eq!(live.left_of_first_separator()[rows], listed);
}

#[test]
fn tab_moves_between_files_and_skips_headings() {
    let mut live = Live::load(TREE_PATCH, Mode::line(DiffLayout::Unified));
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
        live.next_file();
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

    live.previous_file();
    assert!(
        live.shows("new mode 100755"),
        "Shift-Tab wraps to the last file"
    );
}

/// docs/spec/architecture.md#file-tree: markers take their state's color.
#[test]
fn colors_markers_by_state() {
    let mut live = Live::load(TREE_PATCH, Mode::line(DiffLayout::Unified));
    live.previous_file();
    live.previous_file();

    let readme = live.file_row_colors("- README.md");
    let lib = live.file_row_colors("~ lib.rs");
    let new = live.file_row_colors("+ new.rs");
    let view = live.file_row_colors("~ view.rs");

    assert_eq!(view.marker, lib.marker, "one color per state");
    assert_ne!(readme.marker, lib.marker, "deleted and modified differ");
    assert_ne!(readme.marker, new.marker, "deleted and added differ");
    assert_ne!(lib.marker, new.marker, "modified and added differ");

    assert_eq!(
        [lib.name, new.name, view.name],
        [readme.name; 3],
        "every name shares one color"
    );
    assert_ne!(readme.name, readme.marker, "names are not colored by state");
}

/// docs/spec/architecture.md#continuity-markers: the file list marks rows past each edge.
#[test]
fn marks_files_beyond_the_file_list() {
    let mut live = Live::load(&many_files(40), Mode::line(DiffLayout::Unified));
    let markers = |live: &Live| {
        let list = live.left_of_first_separator().join("");
        EdgeMarkers {
            above: list.matches('▴').count(),
            below: list.matches('▾').count(),
        }
    };

    assert_eq!(
        live.left_of_first_separator()[FIRST_CONTENT_LINE],
        "~ file_00.rs"
    );
    assert_eq!(
        markers(&live),
        EdgeMarkers { above: 0, below: 1 },
        "files below the list"
    );
    for _ in 0..36 {
        live.next_file();
    }
    assert_eq!(
        markers(&live),
        EdgeMarkers { above: 1, below: 1 },
        "files above and below"
    );
    // Back past the first file, wrapping to the last.
    for _ in 0..37 {
        live.previous_file();
    }
    assert!(live.shows("~ file_39.rs"));
    assert_eq!(
        markers(&live),
        EdgeMarkers { above: 1, below: 0 },
        "the last file ends the list"
    );
}

#[test]
fn keeps_a_selection_below_its_headings_visible() {
    let mut live = Live::load(&files_in_directories(30), Mode::line(DiffLayout::Unified));

    for _ in 0..20 {
        live.next_file();
    }
    let list = live.left_of_first_separator();
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
