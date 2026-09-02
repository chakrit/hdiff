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
