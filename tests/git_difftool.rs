use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use tempfile::{TempDir, tempdir};

const TEST_PATH: &str = "/usr/bin:/bin";
const TEST_TERM: &str = "dumb";

struct TestEnv {
    directory: TempDir,
}

impl TestEnv {
    fn new() -> Self {
        Self {
            directory: tempdir().expect("temporary directory"),
        }
    }

    fn root(&self) -> PathBuf {
        self.directory
            .path()
            .canonicalize()
            .expect("canonical temporary directory")
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root().join(name)
    }

    fn hdiff(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hdiff"));
        command
            .env_clear()
            .env("HOME", self.root())
            .env("PATH", TEST_PATH)
            .env("TERM", TEST_TERM);

        command
    }
}

#[test]
fn install_backs_up_and_configures_the_global_diff_pager() {
    let env = TestEnv::new();
    let config_path = env.path("gitconfig");
    let original_config = "[user]\n\tname = Test User\n";
    fs::write(&config_path, original_config).expect("write global config");

    let installation = env
        .hdiff()
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
    let env = TestEnv::new();
    let before = env.path("before.rs");
    let after = env.path("after.rs");
    fs::write(&before, "fn main() {}\n").expect("write before file");
    fs::write(&after, "fn main() { println!(\"new\"); }\n").expect("write after file");

    let comparison = env
        .hdiff()
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
    let env = TestEnv::new();
    let input = fs::File::open("tests/fixtures/git-multiline.patch").expect("open fixture");
    let benchmark = env
        .hdiff()
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
    let line = output
        .strip_suffix('\n')
        .expect("benchmark output ends with one newline");
    assert!(!line.contains('\n'), "benchmark output contains one line");

    let fields = line.split(' ').collect::<Vec<_>>();
    assert_eq!(fields.len(), 4, "benchmark output has four fields");
    assert_eq!(fields[0], "preparation");
    let duration = fields[1]
        .strip_prefix("duration_ns=")
        .expect("duration field")
        .parse::<u128>()
        .expect("numeric duration");

    assert!(duration > 0, "preparation duration is positive");
    assert_eq!(fields[2], "files=2");
    assert_eq!(fields[3], "records=7");
}

fn git_config(config_path: &Path, key: &str) -> String {
    let output = Command::new("git")
        .args(["config", "--global", "--get", key])
        .env_clear()
        .env(
            "HOME",
            config_path.parent().expect("Git config has a parent"),
        )
        .env("PATH", TEST_PATH)
        .env("TERM", TEST_TERM)
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
