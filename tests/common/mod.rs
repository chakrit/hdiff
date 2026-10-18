//! Sandbox shared by the integration tests: a temporary directory and an hdiff command
//! that inherits nothing from the host environment.
//!
//! Each test crate compiles its own copy of this module and uses only part of it, so
//! items that some crates leave unused carry `#[allow(dead_code)]`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use hdiff::exit_status::DiffStatus;
use tempfile::TempDir;

/// The only `PATH` any test process sees.
pub const TEST_PATH: &str = "/usr/bin:/bin";
const TEST_TERM: &str = "dumb";

pub struct TestEnv {
    directory: TempDir,
}

impl TestEnv {
    pub fn new() -> Self {
        Self {
            directory: tempfile::tempdir().expect("temporary directory"),
        }
    }

    /// The canonical sandbox root; macOS reaches temporary directories through a symlink.
    pub fn root(&self) -> PathBuf {
        self.directory
            .path()
            .canonicalize()
            .expect("canonical temporary directory")
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root().join(name)
    }

    /// Writes `contents` to `name` under the root, creating parent directories.
    pub fn write_file(&self, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.path(name);
        fs::create_dir_all(path.parent().expect("sandbox file has a parent"))
            .expect("create sandbox directories");
        fs::write(&path, contents).expect("write sandbox file");

        path
    }

    /// hdiff running in the root with only `HOME`, `PATH`, and `TERM` set.
    pub fn hdiff(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hdiff"));
        command.current_dir(self.root());
        self.isolate(&mut command);

        command
    }

    /// Git running in the root, isolated from system and user configuration.
    #[allow(dead_code)]
    pub fn git(&self, arguments: &[&str]) -> Command {
        self.git_in(&self.root(), arguments)
    }

    /// Git running in `directory`, isolated like [`TestEnv::git`].
    #[allow(dead_code)]
    pub fn git_in(&self, directory: &Path, arguments: &[&str]) -> Command {
        let mut command = Command::new("git");
        command.args(arguments).current_dir(directory);
        self.isolate(&mut command);
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture Author")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture Author")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");

        command
    }

    fn isolate(&self, command: &mut Command) {
        command
            .env_clear()
            .env("HOME", self.root())
            .env("PATH", TEST_PATH)
            .env("TERM", TEST_TERM);
    }
}

/// Asserts that hdiff exited with the trouble status its errors use.
#[allow(dead_code)]
pub fn assert_trouble(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(i32::from(DiffStatus::Trouble.code())),
        "{output:?}"
    );
}

/// A file under `tests/fixtures/`, as an absolute path usable from any sandbox.
#[allow(dead_code)]
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}
