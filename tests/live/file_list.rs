//! The file list: its order, selection, markers, and showing and hiding it.

use hdiff::preferences::DiffLayout;

use crate::{
    fixtures::{
        MODIFIED_ROW_PREFIX, TREE_PATCH, directory_name, file_name, files_in_directories,
        listed_row, many_files, paired_patch,
    },
    harness::{FIRST_CONTENT_LINE, HEIGHT, Live, Mode, change_label},
};

/// Lines the file-list toggle scrolls first, so a labeled change row sits on the middle
/// line, away from the block's start.
const LINES_INTO_THE_BLOCK: usize = 20;

/// More files than the default screen has rows, so the file list cannot show them all
/// and files remain below any selection the tests make.
const MORE_FILES_THAN_ROWS: usize = 2 * HEIGHT as usize;

/// How many continuity markers the file list shows above and below its rows.
#[derive(Debug, PartialEq, Eq)]
struct EdgeMarkers {
    above: usize,
    below: usize,
}

/// How many modified-file rows the file list shows. Every file in [`many_files`] and
/// [`files_in_directories`] is modified, so selecting the file at this index, the first
/// one past them, scrolls the list.
fn listed_modified_files(live: &Live) -> usize {
    live.file_column()
        .iter()
        .filter(|row| row.starts_with(MODIFIED_ROW_PREFIX))
        .count()
}

matrix!(file_list_toggle);

/// docs/spec/architecture.md#interaction-model and docs/spec/architecture.md#view-model:
/// `e` hides and shows the file list, floating a `? help` box while it is hidden and
/// keeping the middle line's row in place.
fn file_list_toggle(mode: Mode) {
    let mut live = Live::load(&paired_patch(), mode);
    for _ in 0..LINES_INTO_THE_BLOCK {
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

/// docs/spec/architecture.md#interaction-model: `?` opens a centered popup over the diff,
/// so the file list beside the diff stays as it was drawn.
#[test]
fn opens_the_keymap_popup_over_the_diff_not_the_file_list() {
    let mut live = Live::load(&paired_patch(), Mode::line(DiffLayout::Vertical));
    let file_column = live.file_column();

    live.press('?');

    assert!(live.shows("cycle layout"), "? opens the popup");
    assert_eq!(live.file_column(), file_column);
}

#[test]
fn keeps_a_selection_beyond_the_file_list_height_visible() {
    let patch = many_files(MORE_FILES_THAN_ROWS);
    let mut live = Live::load(&patch, Mode::line(DiffLayout::Unified));
    let selected = listed_modified_files(&live);

    for _ in 0..selected {
        live.next_file();
    }
    assert!(
        live.shows(&listed_row(selected)),
        "the selected file stays listed"
    );
    assert!(
        !live.shows(&file_name(0)),
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
    assert_eq!(live.file_column()[rows], listed);
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
    // Back past the first file to `run.sh`, the one listed file the checks below leave
    // out, so none of the rows they read is highlighted.
    live.previous_file();
    live.previous_file();
    assert!(live.shows("rename to bin/run.sh"), "run.sh is selected");

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
    let file_count = MORE_FILES_THAN_ROWS;
    let mut live = Live::load(&many_files(file_count), Mode::line(DiffLayout::Unified));
    let selected = listed_modified_files(&live);
    let markers = |live: &Live| {
        let list = live.file_column().join("");
        EdgeMarkers {
            above: list.matches('▴').count(),
            below: list.matches('▾').count(),
        }
    };

    assert_eq!(live.file_column()[FIRST_CONTENT_LINE], listed_row(0));
    assert_eq!(
        markers(&live),
        EdgeMarkers { above: 0, below: 1 },
        "files below the list"
    );
    for _ in 0..selected {
        live.next_file();
    }
    assert_eq!(
        markers(&live),
        EdgeMarkers { above: 1, below: 1 },
        "files above and below"
    );
    // Back to the first file, then once more, wrapping to the last.
    for _ in 0..=selected {
        live.previous_file();
    }
    assert!(live.shows(&listed_row(file_count - 1)));
    assert_eq!(
        markers(&live),
        EdgeMarkers { above: 1, below: 0 },
        "the last file ends the list"
    );
}

#[test]
fn keeps_a_selection_below_its_headings_visible() {
    let mut live = Live::load(
        &files_in_directories(MORE_FILES_THAN_ROWS),
        Mode::line(DiffLayout::Unified),
    );
    let selected = listed_modified_files(&live);

    for _ in 0..selected {
        live.next_file();
    }
    let list = live.file_column();
    let selected_row = list
        .iter()
        .position(|row| *row == listed_row(selected))
        .expect("the selected file stays listed below its heading rows");
    assert_eq!(
        list[selected_row - 1],
        format!("{}/", directory_name(selected))
    );
    assert!(
        !live.shows(&format!("{}/", directory_name(0))),
        "the list scrolls past earlier headings"
    );
}
