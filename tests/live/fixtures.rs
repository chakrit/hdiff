//! Patches the scenarios share.

pub const MIXED_LANGUAGES: &str = include_str!("../fixtures/mixed-language-layout.patch");

pub const INDENTED: &str = "--- a/indent.rs\n+++ b/indent.rs\n@@ -1 +1,3 @@\n-removed\n\
                        +added_flush\n+    added_spaces\n+\tadded_tab\n";

/// 200 added rows with one row wider than every pane.
pub fn long_patch() -> String {
    let rows = (0..200)
        .map(|row| match row {
            1 => format!("+wide_{}_end\n", "x".repeat(200)),
            _ => format!("+row_{row}\n"),
        })
        .collect::<String>();
    format!("--- a/long.rs\n+++ b/long.rs\n@@ -0,0 +1,200 @@\n{rows}")
}

/// One change followed by six context records.
pub fn context_patch() -> String {
    let context = (1..=6)
        .map(|row| format!(" context_{row}\n"))
        .collect::<String>();
    format!("--- a/ctx.rs\n+++ b/ctx.rs\n@@ -1,7 +1,7 @@\n-changed_old\n+changed_new\n{context}")
}

/// Forty hunks, each with eight leading context records, five of them hidden at the
/// default context count.
pub fn hidden_context_patch() -> String {
    let hunks = (0..40)
        .map(|hunk| {
            let start = hunk * 20 + 1;
            let context = (1..=8)
                .map(|row| format!(" context_{hunk}_{row}\n"))
                .collect::<String>();
            format!("@@ -{start},9 +{start},9 @@\n{context}-old_{hunk}\n+new_{hunk}\n")
        })
        .collect::<String>();
    format!("--- a/hidden.rs\n+++ b/hidden.rs\n{hunks}")
}

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

/// One block of paired changes, so every layout shows a labeled row on each pane line.
pub fn paired_patch() -> String {
    let removed = (0..100)
        .map(|row| format!("-old_{row}\n"))
        .collect::<String>();
    let added = (0..100)
        .map(|row| format!("+new_{row}\n"))
        .collect::<String>();
    format!("--- a/pairs.rs\n+++ b/pairs.rs\n@@ -1,100 +1,100 @@\n{removed}{added}")
}
