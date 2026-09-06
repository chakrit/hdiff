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
            "  [FILE ...]  Read one patch file, compare two files, ",
            "or read standard input\n",
            "              when omitted\n",
            "\n",
            "Options:\n",
            "  --bench    Prepare the diff and report benchmark measurements\n",
            "  --profile  Prepare the diff and report diagnostic stage measurements\n",
            "  --install  Register hdiff as the user-level Git diff pager\n",
            "  --help     Print help\n",
            "  --version  Print version\n",
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
fn profile_rejects_install_even_when_that_patch_filename_exists() {
    let directory = tempfile::tempdir().expect("fixture directory");
    std::fs::write(
        directory.path().join("--install"),
        "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
    )
    .expect("patch named like an installation option");
    let output = Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .args(["--profile", "--install"])
        .current_dir(directory.path())
        .env_clear()
        .env("PATH", TEST_PATH)
        .output()
        .expect("profile with incompatible installation option");

    assert!(
        !output.status.success(),
        "installation cannot be combined with profiling"
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
