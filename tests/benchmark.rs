use std::{
    collections::BTreeMap,
    fs,
    process::{Command, Stdio},
};

const PATCH: &str = "--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";

fn run(mode: &str, patch: &str) -> std::process::Output {
    let directory = tempfile::tempdir().expect("fixture directory");
    let path = directory.path().join("input.patch");
    fs::write(&path, patch).expect("write patch");
    Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .arg(mode)
        .arg(path)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run measurement")
}

fn fields(line: &str) -> BTreeMap<&str, u128> {
    line.split_whitespace()
        .skip(1)
        .map(|field| {
            let (key, value) = field.split_once('=').expect("key=value");
            (key, value.parse().expect("integer measurement"))
        })
        .collect()
}

#[test]
fn benchmark_accounts_for_input_and_parsing_before_preparation() {
    let output = run("--bench", PATCH);
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).expect("UTF-8 report");
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "preparation and startup records");
    assert!(lines[0].starts_with("preparation "));
    assert!(lines[1].starts_with("startup "));
    let preparation = fields(lines[0]);
    let startup = fields(lines[1]);
    assert_eq!(preparation["files"], 1);
    assert_eq!(preparation["records"], 2);
    assert_eq!(startup["version"], 1);
    assert_eq!(startup["bytes"], PATCH.len() as u128);
    assert_eq!(startup["preparation_ns"], preparation["duration_ns"]);
    assert!(
        startup["ready_ns"]
            >= startup["input_ns"] + startup["parse_ns"] + startup["preparation_ns"]
    );
}

#[test]
fn profile_partitions_parent_costs_and_counts_real_work() {
    let output = run("--profile", PATCH);
    assert!(output.status.success(), "{:?}", output);
    let text = String::from_utf8(output.stdout).expect("UTF-8 report");
    let lines = text.lines().collect::<Vec<_>>();
    assert!(lines[0].starts_with("diagnostic "));
    assert_eq!(lines[2], "profile version=1");
    let startup = fields(lines[1]);
    let mut sums = BTreeMap::new();
    let mut stages = BTreeMap::new();
    for line in &lines[3..lines.len() - 1] {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "stage");
        let name = parts[1].strip_prefix("name=").expect("stage name");
        let parent = parts[2].strip_prefix("parent=").expect("stage parent");
        let duration: u128 = parts[3]
            .strip_prefix("duration_ns=")
            .expect("duration")
            .parse()
            .expect("nanoseconds");
        assert!(stages.insert((parent, name), duration).is_none());
        *sums.entry(parent).or_insert(0) += duration;
    }
    assert_eq!(stages.len(), 10);
    assert_eq!(sums["preparation"], startup["preparation_ns"]);
    assert_eq!(sums["syntax"], stages[&("preparation", "syntax")]);
    let work = fields(lines.last().expect("work counters"));
    assert_eq!(work["hunks"], 1);
    assert_eq!(work["parse_calls"], 2);
    assert_eq!(work["projected_bytes"], 24);
    assert!(work["captures"] > 0);
}

#[test]
fn malformed_input_never_emits_success_measurements() {
    for mode in ["--bench", "--profile"] {
        let output = run(mode, "not a patch\n");
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn empty_input_still_emits_zero_count_measurements() {
    for mode in ["--bench", "--profile"] {
        let output = run(mode, "");
        assert!(output.status.success(), "{mode}: {output:?}");
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).expect("UTF-8 report");
        let lines = text.lines().collect::<Vec<_>>();
        let preparation = fields(lines[0]);
        let startup = fields(lines[1]);

        assert_eq!(preparation["files"], 0);
        assert_eq!(preparation["records"], 0);
        assert_eq!(startup["bytes"], 0);
        assert_eq!(startup["version"], 1);
        if mode == "--profile" {
            assert_eq!(lines[2], "profile version=1");
            let work = fields(lines.last().expect("work counters"));
            assert!(work.values().all(|count| *count == 0));
        }
    }
}

#[test]
fn unsupported_language_has_no_syntax_parser_work() {
    let output = run("--profile", &PATCH.replace("a.rs", "a.txt"));
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("UTF-8 report");
    let work = fields(text.lines().last().expect("work counters"));
    assert_eq!(work["hunks"], 1);
    assert_eq!(work["parse_calls"], 0);
    assert_eq!(work["captures"], 0);
}

#[test]
fn stdin_and_operand_comparison_report_the_acquired_patch_size() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let patch = directory.path().join("input.patch");
    fs::write(&patch, PATCH).expect("patch");
    let stdin = Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .arg("--bench")
        .stdin(Stdio::from(fs::File::open(patch).expect("patch input")))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("stdin benchmark");
    assert!(stdin.status.success());
    let text = String::from_utf8(stdin.stdout).expect("report");
    assert_eq!(
        fields(text.lines().nth(1).expect("startup"))["bytes"],
        PATCH.len() as u128
    );

    let old = directory.path().join("old.rs");
    let new = directory.path().join("new.rs");
    fs::write(&old, "fn old() {}\n").expect("old source");
    fs::write(&new, "fn new() {}\n").expect("new source");
    let comparison = Command::new(env!("CARGO_BIN_EXE_hdiff"))
        .args(["--profile"])
        .arg(old)
        .arg(new)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("operand profile");
    assert!(comparison.status.success(), "{:?}", comparison);
    let text = String::from_utf8(comparison.stdout).expect("report");
    assert_eq!(
        fields(text.lines().next().expect("diagnostic"))["records"],
        2
    );
    assert!(fields(text.lines().nth(1).expect("startup"))["bytes"] > 0);
}
