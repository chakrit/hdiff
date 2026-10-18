use std::process::{Command, Output};

const TEST_PATH: &str = "/usr/bin:/bin";

fn hdiff(argument: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .arg(argument)
        .env_clear()
        .env("PATH", TEST_PATH)
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

fn comparison_tree() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("fixture directory");
    for (path, text) in [
        ("before/top.txt", "old top\n"),
        ("after/top.txt", "new top\n"),
        ("before/nested/inner.txt", "old inner\n"),
        ("after/nested/inner.txt", "new inner\n"),
    ] {
        let path = directory.path().join(path);
        std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture tree");
        std::fs::write(path, text).expect("fixture file");
    }
    directory
}

fn compare(directory: &std::path::Path, arguments: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .args(arguments)
        .current_dir(directory)
        .env_clear()
        .env("PATH", TEST_PATH)
        .output()
        .expect("run comparison");

    assert!(output.status.success(), "{arguments:?}: {output:?}");
    String::from_utf8(output.stdout).expect("comparison output is UTF-8")
}

#[test]
fn help_describes_the_cli_without_reading_diff_input() {
    let help = hdiff("--help");

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
    let version = hdiff("--version");
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
    let directory = tempfile::tempdir().expect("fixture directory");
    std::fs::write(
        directory.path().join("--install"),
        "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
    )
    .expect("patch named like an installation option");
    let output = Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .args(["--bench", "stages", "--install"])
        .current_dir(directory.path())
        .env_clear()
        .env("PATH", TEST_PATH)
        .output()
        .expect("bench with incompatible installation option");

    assert!(
        !output.status.success(),
        "installation cannot be combined with measurement"
    );
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("--install cannot be combined with diff input operands")
    );
}

#[test]
fn empty_sources_exit_silently() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let patch = directory.path().join("empty.patch");
    let before = directory.path().join("before.txt");
    let after = directory.path().join("after.txt");
    std::fs::write(&patch, "").expect("empty patch");
    std::fs::write(&before, "unchanged\n").expect("before source");
    std::fs::write(&after, "unchanged\n").expect("after source");

    for operands in [vec![], vec![&patch], vec![&before, &after]] {
        let output = Command::new(env!("CARGO_BIN_EXE_hdiff"))
            .args(&operands)
            .env_clear()
            .env("PATH", TEST_PATH)
            .output()
            .expect("run empty viewer input");

        assert!(output.status.success(), "{operands:?}: {output:?}");
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn metadata_only_input_remains_visible() {
    let output = hdiff("tests/fixtures/git-rename-only.patch");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        include_bytes!("fixtures/git-rename-only.patch")
    );
}

#[test]
fn truncated_patch_is_an_error() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let patch = directory.path().join("truncated.patch");
    std::fs::write(&patch, "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n").expect("truncated patch");
    let output = hdiff(patch.to_str().expect("fixture path is UTF-8"));

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}
#[test]
fn directory_comparison_descends_only_with_recursion() {
    let directory = comparison_tree();

    let top_level = compare(directory.path(), &["before", "after"]);
    assert!(top_level.contains("+new top\n"));
    assert!(!top_level.contains("+new inner\n"));

    let recursive = compare(directory.path(), &["-r", "before", "after"]);
    assert!(recursive.contains("+new top\n"));
    assert!(recursive.contains("+new inner\n"));
}

#[test]
fn dash_operand_compares_standard_input() {
    let directory = comparison_tree();
    let output = Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .args(["-", "after/top.txt"])
        .current_dir(directory.path())
        .stdin(std::fs::File::open(directory.path().join("before/top.txt")).expect("stdin"))
        .env_clear()
        .env("PATH", TEST_PATH)
        .output()
        .expect("run stdin comparison");

    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).expect("comparison output is UTF-8");
    assert!(text.contains("-old top\n+new top\n"), "{text}");
}

#[test]
fn dash_leading_paths_remain_comparison_operands() {
    let directory = comparison_tree();
    std::fs::write(directory.path().join("-q"), "old top\n").expect("dash-named file");

    let text = compare(directory.path(), &["-q", "after/top.txt"]);
    assert!(text.contains("-old top\n+new top\n"), "{text}");
}

#[test]
fn piped_stdin_with_file_operands_is_a_usage_error() {
    let directory = comparison_tree();
    let mut command = Command::new(env!("CARGO_BIN_EXE_hdiff"));
    command
        .args(["before/top.txt", "after/top.txt"])
        .current_dir(directory.path())
        .stdin(std::process::Stdio::piped())
        .env_clear()
        .env("PATH", TEST_PATH);
    let output = command.output().expect("run comparison with piped stdin");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("piped standard input cannot be combined with file operands")
    );
}

#[test]
fn directory_comparison_renders_added_and_removed_files_as_diffs() {
    let directory = comparison_tree();
    std::fs::write(directory.path().join("before/removed.txt"), "gone\n").expect("removed file");
    std::fs::write(directory.path().join("after/added.txt"), "fresh\n").expect("added file");

    let text = compare(directory.path(), &["before", "after"]);
    assert!(text.contains("-gone\n"), "{text}");
    assert!(text.contains("+fresh\n"), "{text}");
}

#[test]
fn comparison_holding_only_surrounding_text_is_shown() {
    let directory = tempfile::tempdir().expect("fixture directory");
    for (path, bytes) in [("before.bin", b"\0old".as_slice()), ("after.bin", b"\0new")] {
        std::fs::write(directory.path().join(path), bytes).expect("binary fixture");
    }

    let text = compare(directory.path(), &["before.bin", "after.bin"]);
    assert!(
        text.contains("before.bin") && text.contains("differ"),
        "{text}"
    );
}

#[test]
fn patch_file_holding_only_text_is_an_error() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let patch = directory.path().join("notes.patch");
    std::fs::write(&patch, "Binary files a and b differ\n").expect("text-only patch");
    let output = hdiff(patch.to_str().expect("fixture path is UTF-8"));

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "hdiff: line 1: input contains no diff\n"
    );
}

#[test]
fn usage_errors_name_the_program_on_one_stderr_line() {
    let output = Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .args(["a", "b", "c"])
        .env_clear()
        .env("PATH", TEST_PATH)
        .output()
        .expect("run hdiff with too many operands");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "hdiff: expected zero, one, or two operands\n"
    );
}

#[test]
fn failed_comparisons_report_each_diff_error_line() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let output = Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .args(["missing-before", "missing-after"])
        .current_dir(directory.path())
        .env_clear()
        .env("PATH", TEST_PATH)
        .output()
        .expect("run comparison of missing files");

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
    let directory = tempfile::tempdir().expect("fixture directory");
    let output = Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .arg("missing.patch")
        .current_dir(directory.path())
        .env_clear()
        .env("PATH", TEST_PATH)
        .output()
        .expect("run hdiff on a missing patch");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.starts_with("hdiff: missing.patch: "), "{stderr}");
    assert_eq!(stderr.matches('\n').count(), 1, "{stderr}");
}
