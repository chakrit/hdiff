mod common;

use std::{
    fs,
    process::{Output, Stdio},
};

use common::TestEnv;

#[test]
fn install_backs_up_and_configures_the_global_diff_pager() {
    let env = TestEnv::new();
    let original_config = "[user]\n\tname = Test User\n";
    let config_path = env.write_file("gitconfig", original_config);

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
        fs::read_to_string(env.path("gitconfig.bak")).expect("read backup"),
        original_config
    );
    let pager = configured_value(&env, "gitconfig", "pager.diff");
    assert!(pager.contains(env!("CARGO_BIN_EXE_hdiff")));
}

#[test]
fn git_show_preserves_commit_text_and_repeated_file_paths() {
    let env = setup_commit_history();
    let arguments = ["show", "--no-color", "HEAD~1", "HEAD"];
    let expected = git_output(&env, &arguments);
    let viewed = pipe_git_to_hdiff(&env, &arguments);

    assert!(viewed.status.success(), "{viewed:?}");
    assert!(viewed.stderr.is_empty());
    assert_eq!(viewed.stdout, expected);
    let text = String::from_utf8(viewed.stdout).expect("UTF-8 Git history");
    assert!(text.contains("First change"));
    assert!(text.contains("Second change"));
    assert_eq!(text.matches("diff --git a/file.txt b/file.txt").count(), 2);
}

#[test]
fn git_show_sanitizes_colored_commit_text_and_diff() {
    let env = setup_commit_history();
    let expected = git_output(
        &env,
        &[
            "show",
            "--no-color",
            "--format=Commit heading%n%n    %s",
            "HEAD",
        ],
    );
    let viewed = pipe_git_to_hdiff(
        &env,
        &[
            "show",
            "--color=always",
            "--format=%C(red)Commit heading%C(reset)%n%n    \u{1b}]0;unsafe-title\u{7}%s",
            "HEAD",
        ],
    );

    assert!(viewed.status.success(), "{viewed:?}");
    assert!(viewed.stderr.is_empty());
    assert_eq!(viewed.stdout, expected);
}

#[test]
fn git_show_without_diff_content_is_rejected() {
    let env = setup_commit_history();

    for arguments in [
        vec!["show", "HEAD:file.txt"],
        vec!["show", "--no-patch", "HEAD"],
    ] {
        let viewed = pipe_git_to_hdiff(&env, &arguments);

        assert_eq!(viewed.status.code(), Some(2), "{arguments:?}: {viewed:?}");
        assert!(viewed.stdout.is_empty());
        assert!(!viewed.stderr.is_empty());
    }
}

fn setup_commit_history() -> TestEnv {
    let env = TestEnv::new();
    git_output(&env, &["init", "--quiet"]);

    for (content, message) in [("first\n", "First change"), ("second\n", "Second change")] {
        env.write_file("file.txt", content);
        git_output(&env, &["add", "file.txt"]);
        git_output(&env, &["commit", "--quiet", "-m", message]);
    }

    env
}

fn git_output(env: &TestEnv, arguments: &[&str]) -> Vec<u8> {
    let output = env.git(arguments).output().expect("run fixture Git");

    assert!(output.status.success(), "{arguments:?}: {output:?}");
    output.stdout
}

fn pipe_git_to_hdiff(env: &TestEnv, arguments: &[&str]) -> Output {
    let mut producer = env
        .git(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start Git producer");
    let input = producer.stdout.take().expect("Git output pipe");
    let output = env
        .hdiff()
        .stdin(Stdio::from(input))
        .output()
        .expect("read Git output with hdiff");
    let produced = producer.wait_with_output().expect("wait for Git producer");

    assert!(produced.status.success(), "{arguments:?}: {produced:?}");
    output
}

/// Reads `key` from the global config file `config` under the sandbox root.
fn configured_value(env: &TestEnv, config: &str, key: &str) -> String {
    let output = env
        .git(&["config", "--global", "--get", key])
        .env("GIT_CONFIG_GLOBAL", env.path(config))
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
