mod common;

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Output, Stdio},
};

use common::{TestEnv, assert_error, assert_fixture_succeeded, run_fixture, success_output};

const ORIGINAL_CONFIG: &str = "[user]\n\tname = Test User\n";

/// An executable path holding a quote, a space, and a parameter expansion, so the
/// registered pager runs only when it is quoted as one shell word.
const SPECIAL_EXECUTABLE: &str = "it's a $dir/hdiff";

/// A copy of the hdiff binary at `name` under the sandbox root.
fn copied_hdiff(env: &TestEnv, name: &str) -> PathBuf {
    let executable = env.write_file(name, "");
    fs::copy(env!("CARGO_BIN_EXE_hdiff"), &executable).expect("copy the hdiff binary");

    executable
}

/// Runs `executable --install` against a fresh global config holding
/// [`ORIGINAL_CONFIG`], returning the config's path and the installer's output.
fn install(env: &TestEnv, executable: &Path) -> (PathBuf, Output) {
    let config_path = env.write_file("gitconfig", ORIGINAL_CONFIG);

    let installation = env
        .program(executable)
        .arg("--install")
        .env("GIT_CONFIG_GLOBAL", &config_path)
        .output()
        .expect("run hdiff installer");

    (config_path, installation)
}

fn hdiff_version(env: &TestEnv) -> String {
    success_output(&env.run(["--version"]))
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set permissions");
}

fn setup_commit_history() -> TestEnv {
    let env = TestEnv::new();
    run_fixture(&mut env.git(&["init", "--quiet"]));

    for (content, message) in [("first\n", "First change"), ("second\n", "Second change")] {
        env.write_file("file.txt", content);
        run_fixture(&mut env.git(&["add", "file.txt"]));
        run_fixture(&mut env.git(&["commit", "--quiet", "-m", message]));
    }

    env
}

fn pipe_git_to_hdiff(env: &TestEnv, arguments: &[&str]) -> Output {
    let mut git = env.git(arguments);
    let mut producer = git
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start Git producer");
    let input = producer.stdout.take().expect("Git output pipe");
    let no_arguments: [&str; 0] = [];
    let output = env.run_with_stdin(no_arguments, input);
    let produced = producer.wait_with_output().expect("wait for Git producer");

    assert_fixture_succeeded(&git, &produced);
    output
}

/// Reads `pager.diff` from the global config file at `config_path`.
fn configured_pager(env: &TestEnv, config_path: &Path) -> String {
    let mut read = env.git(&["config", "--global", "--get", "pager.diff"]);
    read.env("GIT_CONFIG_GLOBAL", config_path);

    run_fixture(&mut read).trim_end().to_owned()
}

/// docs/spec/architecture.md#input-contract: `--install` backs up the global config and
/// registers the running executable as `pager.diff`, which Git runs through `sh`.
#[test]
fn install_backs_up_and_registers_a_pager_that_runs_hdiff() {
    let env = TestEnv::new();
    let executable = copied_hdiff(&env, SPECIAL_EXECUTABLE);

    let (config_path, installation) = install(&env, &executable);

    success_output(&installation);
    assert_eq!(
        fs::read_to_string(env.path("gitconfig.bak")).expect("read backup"),
        ORIGINAL_CONFIG
    );
    let pager = configured_pager(&env, &config_path);
    let pager_version = env
        .shell(&format!("{pager} --version"))
        .output()
        .expect("run the configured pager");
    assert_eq!(success_output(&pager_version), hdiff_version(&env));
}

/// docs/spec/architecture.md#initial-cli-surface: `--install` prints the `cp` and
/// `git config` commands it applied, so running them reproduces its writes.
#[test]
fn install_prints_the_commands_it_applied() {
    let env = TestEnv::new();
    let executable = copied_hdiff(&env, SPECIAL_EXECUTABLE);
    let (config_path, installation) = install(&env, &executable);

    let printed = success_output(&installation);
    let [copy_command, config_command] = printed.lines().collect::<Vec<_>>()[..] else {
        panic!("expected a cp and a git config command, got {printed:?}");
    };

    fs::remove_file(env.path("gitconfig.bak")).expect("remove the installer's backup");
    let replayed_path = env.write_file("replayed", "");

    run_fixture(&mut env.shell(copy_command));
    let mut replay_config = env.shell(config_command);
    replay_config.env("GIT_CONFIG_GLOBAL", &replayed_path);
    run_fixture(&mut replay_config);

    assert_eq!(
        fs::read_to_string(env.path("gitconfig.bak")).expect("read replayed backup"),
        fs::read_to_string(&config_path).expect("read installed config")
    );
    assert_eq!(
        configured_pager(&env, &replayed_path),
        configured_pager(&env, &config_path)
    );
}

/// docs/spec/architecture.md#error-output: unreadable files are named in the message.
#[test]
fn install_names_a_config_path_it_cannot_inspect() {
    let env = TestEnv::new();
    let locked = env.path("locked");
    fs::create_dir(&locked).expect("create locked directory");
    let config_path = locked.join("gitconfig");

    set_mode(&locked, 0o000);
    let installation = env
        .hdiff()
        .arg("--install")
        .env("GIT_CONFIG_GLOBAL", &config_path)
        .output();
    set_mode(&locked, 0o755);
    let installation = installation.expect("run hdiff installer");

    assert_error(
        &installation,
        &format!(
            "--install: {}: Permission denied (os error 13)",
            config_path.display()
        ),
    );
}

#[test]
fn git_show_preserves_commit_text_and_repeated_file_paths() {
    let env = setup_commit_history();
    let arguments = ["show", "--no-color", "HEAD~1", "HEAD"];
    let expected = run_fixture(&mut env.git(&arguments));
    let viewed = pipe_git_to_hdiff(&env, &arguments);

    let text = success_output(&viewed);
    assert_eq!(text, expected);
    assert!(text.contains("First change"), "{text}");
    assert!(text.contains("Second change"), "{text}");
    assert_eq!(
        text.matches("diff --git a/file.txt b/file.txt").count(),
        2,
        "{text}"
    );
}

#[test]
fn git_show_sanitizes_colored_commit_text_and_diff() {
    let env = setup_commit_history();
    let expected = run_fixture(&mut env.git(&[
        "show",
        "--no-color",
        "--format=Commit heading%n%n    %s",
        "HEAD",
    ]));
    let viewed = pipe_git_to_hdiff(
        &env,
        &[
            "show",
            "--color=always",
            "--format=%C(red)Commit heading%C(reset)%n%n    \u{1b}]0;unsafe-title\u{7}%s",
            "HEAD",
        ],
    );

    assert_eq!(success_output(&viewed), expected);
}

#[test]
fn git_show_without_diff_content_is_rejected() {
    let env = setup_commit_history();

    for arguments in [
        vec!["show", "HEAD:file.txt"],
        vec!["show", "--no-patch", "HEAD"],
    ] {
        let viewed = pipe_git_to_hdiff(&env, &arguments);

        assert_error(&viewed, "input contains no diff");
    }
}
