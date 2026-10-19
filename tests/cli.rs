mod common;

use std::{fs, path::Path, process::Stdio};

use common::{TestEnv, assert_error, error_output, fixture, run_fixture, success_output};

fn short_commit_hash() -> String {
    let env = TestEnv::new();
    let mut rev_parse = env.git_in(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &["rev-parse", "--short", "HEAD"],
    );

    run_fixture(&mut rev_parse).trim_end().to_owned()
}

fn comparison_tree() -> TestEnv {
    let env = TestEnv::new();
    for (path, text) in [
        ("before/top.txt", "old top\n"),
        ("after/top.txt", "new top\n"),
        ("before/nested/inner.txt", "old inner\n"),
        ("after/nested/inner.txt", "new inner\n"),
    ] {
        env.write_file(path, text);
    }
    env
}

#[test]
fn help_describes_the_cli_without_reading_diff_input() {
    let help = TestEnv::new().run(["--help"]);
    let drivers = ["prepare", "stages", "movement"];

    let text = success_output(&help);
    assert!(text.starts_with("Usage: hdiff"), "{text}");
    for named in [
        "[FILE ...]",
        " - ",
        "-r",
        "--bench",
        "--install",
        "--help",
        "--version",
    ]
    .into_iter()
    .chain(drivers)
    {
        assert!(text.contains(named), "help names {named}:\n{text}");
    }
}

#[test]
fn version_identifies_the_package_and_source_commit() {
    let version = TestEnv::new().run(["--version"]);
    let expected = format!(
        "hdiff {} ({})\n",
        env!("CARGO_PKG_VERSION"),
        short_commit_hash()
    );

    assert_eq!(success_output(&version), expected);
}

#[test]
fn bench_rejects_install_even_when_that_patch_filename_exists() {
    let env = TestEnv::new();
    env.write_file(
        "--install",
        "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let output = env.run(["--bench", "stages", "--install"]);

    assert_error(
        &output,
        "--install cannot be combined with diff input operands",
    );
}

#[test]
fn empty_sources_exit_silently() {
    let env = TestEnv::new();
    let patch = env.write_file("empty.patch", "");
    let before = env.write_file("before.txt", "unchanged\n");
    let after = env.write_file("after.txt", "unchanged\n");

    for operands in [vec![], vec![&patch], vec![&before, &after]] {
        let output = env.run(&operands);

        assert_eq!(success_output(&output), "", "{operands:?}");
    }
}

#[test]
fn metadata_only_input_remains_visible() {
    let patch = fixture("git-rename-only.patch");
    let output = TestEnv::new().run([patch]);

    assert_eq!(
        success_output(&output),
        include_str!("fixtures/git-rename-only.patch")
    );
}

#[test]
fn truncated_patch_is_an_error() {
    let env = TestEnv::new();
    let patch = env.write_file(
        "truncated.patch",
        "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n",
    );
    let output = env.run([patch]);

    assert_error(&output, "line 5: truncated hunk");
}

#[test]
fn directory_comparison_descends_only_with_recursion() {
    let env = comparison_tree();

    let top_level = success_output(&env.run(["before", "after"]));
    assert!(top_level.contains("+new top\n"), "{top_level}");
    assert!(!top_level.contains("+new inner\n"), "{top_level}");

    let recursive = success_output(&env.run(["-r", "before", "after"]));
    assert!(recursive.contains("+new top\n"), "{recursive}");
    assert!(recursive.contains("+new inner\n"), "{recursive}");
}

#[test]
fn dash_operand_compares_standard_input() {
    let env = comparison_tree();
    let before = fs::File::open(env.path("before/top.txt")).expect("stdin");
    let output = env
        .hdiff()
        .args(["-", "after/top.txt"])
        .stdin(before)
        .output()
        .expect("run stdin comparison");

    let text = success_output(&output);
    assert!(text.contains("-old top\n+new top\n"), "{text}");
}

#[test]
fn dash_leading_paths_remain_comparison_operands() {
    let env = comparison_tree();
    env.write_file("-q", "old top\n");

    let text = success_output(&env.run(["-q", "after/top.txt"]));
    assert!(text.contains("-old top\n+new top\n"), "{text}");
}

#[test]
fn piped_stdin_with_file_operands_is_a_usage_error() {
    let env = comparison_tree();
    let output = env
        .hdiff()
        .args(["before/top.txt", "after/top.txt"])
        .stdin(Stdio::piped())
        .output()
        .expect("run comparison with piped stdin");

    assert_error(
        &output,
        "piped standard input cannot be combined with file operands; name it with -",
    );
}

#[test]
fn directory_comparison_renders_added_and_removed_files_as_diffs() {
    let env = comparison_tree();
    env.write_file("before/removed.txt", "gone\n");
    env.write_file("after/added.txt", "fresh\n");

    let text = success_output(&env.run(["before", "after"]));
    assert!(text.contains("-gone\n"), "{text}");
    assert!(text.contains("+fresh\n"), "{text}");
}

#[test]
fn comparison_holding_only_surrounding_text_is_shown() {
    let env = TestEnv::new();
    env.write_file("before.bin", b"\0old");
    env.write_file("after.bin", b"\0new");

    let text = success_output(&env.run(["before.bin", "after.bin"]));
    assert!(
        text.contains("before.bin") && text.contains("differ"),
        "{text}"
    );
}

#[test]
fn patch_file_holding_only_text_is_an_error() {
    let env = TestEnv::new();
    let patch = env.write_file("notes.patch", "Binary files a and b differ\n");
    let output = env.run([patch]);

    assert_error(&output, "input contains no diff");
}

#[test]
fn usage_errors_name_the_program_on_one_stderr_line() {
    let output = TestEnv::new().run(["a", "b", "c"]);

    assert_error(&output, "expected zero, one, or two operands");
}

#[test]
fn failed_comparisons_report_each_diff_error_line() {
    let output = TestEnv::new().run(["missing-before", "missing-after"]);

    let stderr = error_output(&output);
    let lines = stderr.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "{stderr}");
    assert!(
        lines[0].starts_with("hdiff: diff: missing-before"),
        "{stderr}"
    );
    assert!(
        lines[1].starts_with("hdiff: diff: missing-after"),
        "{stderr}"
    );
}

#[test]
fn unreadable_patch_files_are_named_in_the_error() {
    let output = TestEnv::new().run(["missing.patch"]);

    let stderr = error_output(&output);
    assert!(stderr.starts_with("hdiff: missing.patch: "), "{stderr}");
    assert_eq!(stderr.matches('\n').count(), 1, "{stderr}");
}
