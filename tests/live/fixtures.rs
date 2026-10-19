//! Patches the scenarios share.

/// Rust, JavaScript, Python, Go, and C files in several directories, one page each.
pub const MIXED_LANGUAGES: &str = include_str!("../fixtures/mixed-language-layout.patch");

/// One deletion and three additions, flush and indented by spaces and by a tab.
pub const INDENTED: &str = "--- a/indent.rs\n+++ b/indent.rs\n@@ -1 +1,3 @@\n-removed\n\
                        +added_flush\n+    added_spaces\n+\tadded_tab\n";

/// `count` one-change files, `file_00.rs` onward, at the top level.
pub fn many_files(count: usize) -> String {
    (0..count)
        .map(|file| one_change_file(&file_name(file)))
        .collect()
}

/// `count` one-change files, each `file_NN.rs` in its own `dir_NN/` directory.
pub fn files_in_directories(count: usize) -> String {
    (0..count)
        .map(|file| format!("{}/{}", directory_name(file), file_name(file)))
        .map(|path| one_change_file(&path))
        .collect()
}

/// The patch of a file at `path` whose one line changes from `old` to `new`.
pub fn one_change_file(path: &str) -> String {
    format!("--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n")
}

/// The name of file `index` in [`many_files`] and [`files_in_directories`].
pub fn file_name(index: usize) -> String {
    format!("file_{index:02}.rs")
}

/// The file list's marker and space before a modified file's name.
pub const MODIFIED_ROW_PREFIX: &str = "~ ";

/// The file list's row for file `index` in [`many_files`] or [`files_in_directories`],
/// both of which modify every file.
pub fn listed_row(index: usize) -> String {
    format!("{MODIFIED_ROW_PREFIX}{}", file_name(index))
}

/// The directory of file `index` in [`files_in_directories`].
pub fn directory_name(index: usize) -> String {
    format!("dir_{index:02}")
}

/// The file list's heading row for the directory of file `index` in
/// [`files_in_directories`].
pub fn directory_row(index: usize) -> String {
    format!("{}/", directory_name(index))
}

/// How many `x` graphemes the wide row of [`long_patch`] holds.
pub const WIDE_ROW_GRAPHEMES: usize = 200;

/// How many added rows [`long_patch`] holds.
pub const LONG_PATCH_ROWS: usize = 200;

/// The index of [`long_patch`]'s one row wider than every pane.
pub const WIDE_ROW_INDEX: usize = 1;

/// [`LONG_PATCH_ROWS`] added rows named `row_{index}`, except the wide row at
/// [`WIDE_ROW_INDEX`], which is wider than every pane.
pub fn long_patch() -> String {
    let rows = (0..LONG_PATCH_ROWS)
        .map(|row| match row {
            WIDE_ROW_INDEX => format!("+wide_{}_end\n", "x".repeat(WIDE_ROW_GRAPHEMES)),
            _ => format!("+row_{row}\n"),
        })
        .collect::<String>();
    format!("--- a/long.rs\n+++ b/long.rs\n@@ -0,0 +1,{LONG_PATCH_ROWS} @@\n{rows}")
}

/// How many context records follow [`context_patch`]'s one change.
const CONTEXT_PATCH_RECORDS: usize = 6;

/// One change followed by [`CONTEXT_PATCH_RECORDS`] context records.
pub fn context_patch() -> String {
    let context = (1..=CONTEXT_PATCH_RECORDS)
        .map(|row| format!(" context_{row}\n"))
        .collect::<String>();
    let side_lines = CONTEXT_PATCH_RECORDS + 1;
    format!(
        "--- a/ctx.rs\n+++ b/ctx.rs\n@@ -1,{side_lines} +1,{side_lines} @@\n\
         -changed_old\n+changed_new\n{context}"
    )
}

/// How many hunks [`hidden_context_patch`] holds.
pub const HIDDEN_CONTEXT_HUNKS: usize = 40;

/// How many leading context records each hunk of [`hidden_context_patch`] holds.
const HIDDEN_CONTEXT_RECORDS: usize = 8;

/// The lines from one hunk's start to the next in [`hidden_context_patch`], wider than a
/// hunk so the hunks never overlap.
const HIDDEN_CONTEXT_HUNK_STRIDE: usize = 20;

/// [`HIDDEN_CONTEXT_HUNKS`] hunks, each with [`HIDDEN_CONTEXT_RECORDS`] leading context
/// records before one change, so most of them are hidden at the default context count.
pub fn hidden_context_patch() -> String {
    let side_lines = HIDDEN_CONTEXT_RECORDS + 1;
    let hunks = (0..HIDDEN_CONTEXT_HUNKS)
        .map(|hunk| {
            let start = hunk * HIDDEN_CONTEXT_HUNK_STRIDE + 1;
            let context = (1..=HIDDEN_CONTEXT_RECORDS)
                .map(|row| format!(" context_{hunk}_{row}\n"))
                .collect::<String>();
            format!(
                "@@ -{start},{side_lines} +{start},{side_lines} @@\n\
                 {context}-old_{hunk}\n+new_{hunk}\n"
            )
        })
        .collect::<String>();
    format!("--- a/hidden.rs\n+++ b/hidden.rs\n{hunks}")
}

/// Git changes in nested directories and at the top level, for the file tree.
pub const TREE_PATCH: &str = "\
diff --git a/src/terminal/view.rs b/src/terminal/view.rs
index 1111111..2222222 100644
--- a/src/terminal/view.rs
+++ b/src/terminal/view.rs
@@ -1 +1 @@
-view_old
+view_new
diff --git a/README.md b/README.md
deleted file mode 100644
--- a/README.md
+++ /dev/null
@@ -1 +0,0 @@
-readme_gone
diff --git a/src/new.rs b/src/new.rs
new file mode 100644
--- /dev/null
+++ b/src/new.rs
@@ -0,0 +1 @@
+added_file
diff --git a/old.sh b/bin/run.sh
similarity index 100%
rename from old.sh
rename to bin/run.sh
diff --git a/src/lib.rs b/src/lib.rs
old mode 100644
new mode 100755
";

/// How many deletions and how many additions [`paired_patch`]'s block holds.
const PAIRED_ROWS: usize = 100;

/// One block of [`PAIRED_ROWS`] paired changes, so every layout shows a labeled row on
/// each pane line.
pub fn paired_patch() -> String {
    let removed = (0..PAIRED_ROWS)
        .map(|row| format!("-old_{row}\n"))
        .collect::<String>();
    let added = (0..PAIRED_ROWS)
        .map(|row| format!("+new_{row}\n"))
        .collect::<String>();
    format!(
        "--- a/pairs.rs\n+++ b/pairs.rs\n@@ -1,{PAIRED_ROWS} +1,{PAIRED_ROWS} @@\n\
         {removed}{added}"
    )
}

/// Two deletions beside one addition: the second deletion pairs with an alignment peer.
pub const EXTRA_DELETION: &str =
    "--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n";

/// One word changed by one letter.
pub const ONE_LETTER_CHANGE: &str = "--- a/source.txt\n+++ b/source.txt\n@@ -1 +1 @@\n-one\n+ore\n";
