//! Files arriving while the session runs: the `(parsing...)` line, file rotation while
//! loading, and a selection that stays on its file. Rules:
//! docs/spec/architecture.md#file-tree.

use crate::harness::Loading;

/// A modified file at `path` whose one record reads `line`.
fn modified(path: &str, line: &str) -> String {
    format!("--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old {line}\n+new {line}\n")
}

/// The patch of modified files at `paths`, each record naming its file.
fn patch(paths: &[&str]) -> String {
    paths.iter().map(|path| modified(path, path)).collect()
}

#[test]
fn ends_the_file_list_with_a_parsing_line_until_the_input_ends() {
    let mut loading = Loading::start(&patch(&["one.rs", "two.rs"]));

    assert_eq!(
        loading.live.file_column()[1..3],
        ["~ one.rs", "(parsing...)"],
        "{:?}",
        loading.live.file_column()
    );

    loading.end();

    assert!(
        !loading.live.shows("(parsing..."),
        "{:?}",
        loading.live.rows()
    );
    assert!(loading.live.shows("~ two.rs"));
}

#[test]
fn tab_stops_at_the_last_file_while_files_are_still_arriving() {
    let mut loading = Loading::start(&patch(&["one.rs", "two.rs", "three.rs"]));

    loading.live.next_file();
    assert!(
        loading.live.shows("new one.rs"),
        "Tab stays on the only file"
    );

    loading.arrive(1);
    loading.live.next_file();
    assert!(
        loading.live.shows("new two.rs"),
        "Tab reaches the file that arrived"
    );
    loading.live.next_file();
    assert!(
        loading.live.shows("new two.rs"),
        "Tab stops at the last file"
    );

    loading.end();
    loading.live.next_file();
    assert!(loading.live.shows("new three.rs"));
    loading.live.next_file();
    assert!(
        loading.live.shows("new one.rs"),
        "Tab wraps once the input has ended"
    );
}

#[test]
fn a_file_arriving_above_the_selection_keeps_it_on_its_file_and_its_line() {
    let mut loading = Loading::start(&patch(&["src/lib.rs", "top.md"]));
    let selected_line = loading.live.find("~ lib.rs").y;

    loading.arrive(1);

    assert!(
        loading.live.shows("new src/lib.rs"),
        "the selection stays on its file"
    );
    assert_eq!(loading.live.find("~ lib.rs").y, selected_line);
    assert!(
        loading.live.file_column()[0].contains('▴'),
        "the arrived top-level file lists above the window: {:?}",
        loading.live.file_column()
    );

    loading.live.previous_file();
    assert!(
        loading.live.shows("~ top.md"),
        "{:?}",
        loading.live.file_column()
    );
    assert!(loading.live.shows("new top.md"));
}
