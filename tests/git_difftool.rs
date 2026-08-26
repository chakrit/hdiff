use std::{
    fs,
    process::{Command, Stdio},
};

use tempfile::tempdir;

fn hdiff() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hdiff"))
}

#[test]
fn install_backs_up_and_configures_the_global_diff_pager() {
    let directory = tempdir().expect("temporary directory");
    let config_path = directory.path().join("gitconfig");
    let original_config = "[user]\n\tname = Test User\n";
    fs::write(&config_path, original_config).expect("write global config");

    let installation = hdiff()
        .arg("--install")
        .env("GIT_CONFIG_GLOBAL", &config_path)
        .output()
        .expect("run hdiff installer");

    assert!(
        installation.status.success(),
        "installer failed: {}",
        String::from_utf8_lossy(&installation.stderr)
    );
    assert_eq!(
        fs::read_to_string(config_path.with_extension("bak")).expect("read backup"),
        original_config
    );
    let command = git_config(&config_path, "pager.diff");
    assert!(command.contains(env!("CARGO_BIN_EXE_hdiff")));
}

#[test]
fn compares_two_files_for_git_diff_pager() {
    let directory = tempdir().expect("temporary directory");
    let before = directory.path().join("before.rs");
    let after = directory.path().join("after.rs");
    fs::write(&before, "fn main() {}\n").expect("write before file");
    fs::write(&after, "fn main() { println!(\"new\"); }\n").expect("write after file");

    let comparison = hdiff()
        .arg(&before)
        .arg(&after)
        .output()
        .expect("run hdiff comparison");

    assert!(
        comparison.status.success(),
        "comparison failed: {}",
        String::from_utf8_lossy(&comparison.stderr)
    );

    let rendered = String::from_utf8(comparison.stdout).expect("utf-8 diff output");
    assert!(rendered.contains("-fn main() {}"));
    assert!(rendered.contains("+fn main() { println!(\"new\"); }"));
}

#[test]
fn bench_prepares_diff_without_opening_the_terminal() {
    let input = fs::File::open("tests/fixtures/git-multiline.patch").expect("open fixture");
    let benchmark = hdiff()
        .arg("--bench")
        .stdin(Stdio::from(input))
        .output()
        .expect("run hdiff benchmark");

    assert!(
        benchmark.status.success(),
        "benchmark failed: {}",
        String::from_utf8_lossy(&benchmark.stderr)
    );
    let output = String::from_utf8(benchmark.stdout).expect("benchmark output is UTF-8");

    assert!(output.starts_with("preparation duration_ns="));
    assert!(output.contains(" files=2 records=7\n"));
}

fn git_config(config_path: &std::path::Path, key: &str) -> String {
    let output = Command::new("git")
        .args(["config", "--global", "--get", key])
        .env("GIT_CONFIG_GLOBAL", config_path)
        .output()
        .expect("read global Git config");

    assert!(
        output.status.success(),
        "Git could not read {key}: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout)
        .expect("UTF-8 Git config")
        .trim_end()
        .to_owned()
}
