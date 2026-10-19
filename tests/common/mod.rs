//! Sandbox shared by the integration tests: a temporary directory and an hdiff command
//! that inherits nothing from the host environment.
//!
//! Each test crate compiles its own copy of this module and uses only part of it, so
//! items that some crates leave unused carry `#[allow(dead_code)]`.

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    str,
};

use hdiff::exit_status::DiffStatus;
use tempfile::TempDir;

/// The only `PATH` any test process sees.
pub const TEST_PATH: &str = "/usr/bin:/bin";
/// The only `TERM` any test process sees.
const TEST_TERM: &str = "dumb";

/// One test's sandbox: a temporary directory that is every process's home, removed when
/// the test ends.
pub struct TestEnv {
    directory: TempDir,
}

impl TestEnv {
    /// A fresh, empty sandbox.
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

    /// `name` under the sandbox root.
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
        self.program(Path::new(env!("CARGO_BIN_EXE_hdiff")))
    }

    /// The output of hdiff run in the root with `arguments`, isolated like
    /// [`TestEnv::hdiff`], with no standard input.
    #[allow(dead_code)]
    pub fn run(&self, arguments: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Output {
        self.run_with_stdin(arguments, Stdio::null())
    }

    /// The output of hdiff run in the root with `arguments`, isolated like
    /// [`TestEnv::hdiff`], reading `stdin` as its standard input.
    #[allow(dead_code)]
    pub fn run_with_stdin(
        &self,
        arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
        stdin: impl Into<Stdio>,
    ) -> Output {
        let mut command = self.hdiff();
        command.args(arguments).stdin(stdin);

        command
            .output()
            .unwrap_or_else(|error| panic!("{error}: {command:?}"))
    }

    /// `program` running in the root, isolated like [`TestEnv::hdiff`].
    pub fn program(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
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

    /// `sh` running `script` in the root, isolated like [`TestEnv::hdiff`].
    #[allow(dead_code)]
    pub fn shell(&self, script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]).current_dir(self.root());
        self.isolate(&mut command);

        command
    }

    /// Clears `command`'s environment down to the sandbox's home, path, and terminal.
    fn isolate(&self, command: &mut Command) {
        command
            .env_clear()
            .env("HOME", self.root())
            .env("PATH", TEST_PATH)
            .env("TERM", TEST_TERM);
    }
}

/// The standard output of an hdiff run that succeeded: it exited with status 0, wrote
/// nothing to standard error, and wrote UTF-8.
#[allow(dead_code)]
pub fn success_output(output: &Output) -> String {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");

    str::from_utf8(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {output:?}"))
        .to_owned()
}

/// Runs a fixture command, such as Git, that must succeed, and returns its UTF-8
/// standard output; a failure shows the command and its whole output.
#[allow(dead_code)]
pub fn run_fixture(command: &mut Command) -> String {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("{error}: {command:?}"));
    assert_fixture_succeeded(command, &output);

    String::from_utf8(output.stdout.clone())
        .unwrap_or_else(|error| panic!("{error}: {command:?}: {output:?}"))
}

/// Asserts that a finished fixture command succeeded; a failure shows the command and its
/// whole output.
#[allow(dead_code)]
pub fn assert_fixture_succeeded(command: &Command, output: &Output) {
    assert!(output.status.success(), "{command:?}: {output:?}");
}

/// The standard error of an hdiff run that failed.
/// docs/spec/architecture.md#error-output: a failed run exits with the trouble status,
/// writes nothing to standard output, and writes UTF-8 to standard error.
#[allow(dead_code)]
pub fn failure_output(output: &Output) -> String {
    assert_eq!(
        output.status.code(),
        Some(i32::from(DiffStatus::Trouble.code())),
        "{output:?}"
    );
    assert!(output.stdout.is_empty(), "{output:?}");

    str::from_utf8(&output.stderr)
        .unwrap_or_else(|error| panic!("{error}: {output:?}"))
        .to_owned()
}

/// Asserts that hdiff failed with the one-line error `message`, after its `hdiff: `
/// prefix.
#[allow(dead_code)]
pub fn assert_error(output: &Output, message: &str) {
    assert_eq!(failure_output(output), format!("hdiff: {message}\n"));
}

/// A file under `tests/fixtures/`, as an absolute path usable from any sandbox.
#[allow(dead_code)]
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}
