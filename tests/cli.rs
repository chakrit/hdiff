mod common;

use std::{
    fs,
    process::{Command, Output, Stdio},
};

use common::{TEST_PATH, TestEnv, fixture};

fn hdiff(arguments: &[&str]) -> Output {
    TestEnv::new()
        .hdiff()
        .args(arguments)
        .output()
        .expect("run hdiff")
}

fn short_commit_hash() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env_clear()
        .env("PATH", TEST_PATH)
        .output()
        .expect("read source commit hash");

    assert!(
        output.status.success(),
        "Git could not read the source commit"
    );

    String::from_utf8(output.stdout)
        .expect("commit hash is UTF-8")
        .trim_end()
        .to_owned()
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

fn compare(env: &TestEnv, arguments: &[&str]) -> String {
    let output = env
        .hdiff()
        .args(arguments)
        .output()
        .expect("run comparison");

    assert!(output.status.success(), "{arguments:?}: {output:?}");
    String::from_utf8(output.stdout).expect("comparison output is UTF-8")
}

#[test]
fn help_describes_the_cli_without_reading_diff_input() {
    let help = hdiff(&["--help"]);

    assert!(help.status.success());
    assert!(help.stderr.is_empty());
    assert_eq!(
        String::from_utf8(help.stdout).expect("help is UTF-8"),
        concat!(
            "Usage: hdiff [OPTIONS] [FILE ...]\n",
            "\n",
            "Arguments:\n",
            "  [FILE ...]  Read one patch file, compare two files or directories, ",
            "or read\n",
            "              standard input when omitted; - names standard input\n",
            "\n",
            "Options:\n",
            "  -r                Compare directories recursively\n",
            "  --bench <DRIVER>  Prepare the diff and report measurements from DRIVER:\n",
            "                    prepare, stages, or movement\n",
            "  --install         Register hdiff as the user-level Git diff pager\n",
            "  --help            Print help\n",
            "  --version         Print version\n",
        )
    );
}

#[test]
fn version_identifies_the_package_and_source_commit() {
    let version = hdiff(&["--version"]);
    let expected = format!(
        "hdiff {} ({})\n",
        env!("CARGO_PKG_VERSION"),
        short_commit_hash()
    );

    assert!(version.status.success());
    assert!(version.stderr.is_empty());
    assert_eq!(
        String::from_utf8(version.stdout).expect("version is UTF-8"),
        expected
    );
}

#[test]
fn bench_rejects_install_even_when_that_patch_filename_exists() {
    let env = TestEnv::new();
    env.write_file(
        "--install",
        "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let output = env
        .hdiff()
        .args(["--bench", "stages", "--install"])
        .output()
        .expect("bench with incompatible installation option");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "hdiff: --install cannot be combined with diff input operands\n"
    );
}

#[test]
fn empty_sources_exit_silently() {
    let env = TestEnv::new();
    let patch = env.write_file("empty.patch", "");
    let before = env.write_file("before.txt", "unchanged\n");
    let after = env.write_file("after.txt", "unchanged\n");

    for operands in [vec![], vec![&patch], vec![&before, &after]] {
        let output = env
            .hdiff()
            .args(&operands)
            .output()
            .expect("run empty viewer input");

        assert!(output.status.success(), "{operands:?}: {output:?}");
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn metadata_only_input_remains_visible() {
    let patch = fixture("git-rename-only.patch");
    let output = hdiff(&[patch.to_str().expect("fixture path is UTF-8")]);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        include_bytes!("fixtures/git-rename-only.patch")
    );
}

#[test]
fn truncated_patch_is_an_error() {
    let env = TestEnv::new();
    let patch = env.write_file(
        "truncated.patch",
        "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n",
    );
    let output = env
        .hdiff()
        .arg(patch)
        .output()
        .expect("run hdiff on a truncated patch");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "hdiff: line 5: truncated hunk\n"
    );
}

#[test]
fn directory_comparison_descends_only_with_recursion() {
    let env = comparison_tree();

    let top_level = compare(&env, &["before", "after"]);
    assert!(top_level.contains("+new top\n"));
    assert!(!top_level.contains("+new inner\n"));

    let recursive = compare(&env, &["-r", "before", "after"]);
    assert!(recursive.contains("+new top\n"));
    assert!(recursive.contains("+new inner\n"));
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

    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).expect("comparison output is UTF-8");
    assert!(text.contains("-old top\n+new top\n"), "{text}");
}

#[test]
fn dash_leading_paths_remain_comparison_operands() {
    let env = comparison_tree();
    env.write_file("-q", "old top\n");

    let text = compare(&env, &["-q", "after/top.txt"]);
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

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "hdiff: piped standard input cannot be combined with file operands; name it with -\n"
    );
}

#[test]
fn directory_comparison_renders_added_and_removed_files_as_diffs() {
    let env = comparison_tree();
    env.write_file("before/removed.txt", "gone\n");
    env.write_file("after/added.txt", "fresh\n");

    let text = compare(&env, &["before", "after"]);
    assert!(text.contains("-gone\n"), "{text}");
    assert!(text.contains("+fresh\n"), "{text}");
}

#[test]
fn comparison_holding_only_surrounding_text_is_shown() {
    let env = TestEnv::new();
    env.write_file("before.bin", b"\0old");
    env.write_file("after.bin", b"\0new");

    let text = compare(&env, &["before.bin", "after.bin"]);
    assert!(
        text.contains("before.bin") && text.contains("differ"),
        "{text}"
    );
}

#[test]
fn patch_file_holding_only_text_is_an_error() {
    let env = TestEnv::new();
    let patch = env.write_file("notes.patch", "Binary files a and b differ\n");
    let output = env
        .hdiff()
        .arg(patch)
        .output()
        .expect("run hdiff on a text-only patch");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "hdiff: input contains no diff\n"
    );
}

#[test]
fn usage_errors_name_the_program_on_one_stderr_line() {
    let output = hdiff(&["a", "b", "c"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "hdiff: expected zero, one, or two operands\n"
    );
}

#[test]
fn failed_comparisons_report_each_diff_error_line() {
    let output = hdiff(&["missing-before", "missing-after"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
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
    let output = hdiff(&["missing.patch"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.starts_with("hdiff: missing.patch: "), "{stderr}");
    assert_eq!(stderr.matches('\n').count(), 1, "{stderr}");
}
