mod common;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    process::{Output, Stdio},
};

use common::{TestEnv, assert_trouble};
use hdiff::{
    bench::Driver,
    measurement::{Parent, Stage},
};

const PATCH: &str = "--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";

fn run(driver: &str, patch: &str) -> Output {
    let env = TestEnv::new();
    let path = env.write_file("input.patch", patch);

    env.hdiff()
        .args(["--bench", driver])
        .arg(path)
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
    let output = run("prepare", PATCH);
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());

    let text = String::from_utf8(output.stdout).expect("UTF-8 report");
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "preparation and startup records");
    assert!(lines[0].starts_with("preparation "));
    assert!(lines[1].starts_with("startup "));

    let preparation = fields(lines[0]);
    let startup = fields(lines[1]);
    let patch_bytes = u128::try_from(PATCH.len()).expect("patch length fits u128");

    assert_eq!(preparation["files"], 1);
    assert_eq!(preparation["records"], 2);
    assert_eq!(startup["version"], 1);
    assert_eq!(startup["bytes"], patch_bytes);
    assert_eq!(startup["preparation_ns"], preparation["duration_ns"]);
    assert!(
        startup["ready_ns"]
            >= startup["input_ns"] + startup["parse_ns"] + startup["preparation_ns"]
    );
}

/// One `stage` record's parent, name, and duration.
fn stage_record(line: &str) -> ((&str, &str), u128) {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    assert_eq!(parts.len(), 4, "{line}");
    assert_eq!(parts[0], "stage");

    let name = parts[1].strip_prefix("name=").expect("stage name");
    let parent = parts[2].strip_prefix("parent=").expect("stage parent");
    let duration = parts[3]
        .strip_prefix("duration_ns=")
        .expect("duration")
        .parse()
        .expect("nanoseconds");

    ((parent, name), duration)
}

/// Every stage under its parent, plus each parent's unattributed remainder.
fn expected_stages() -> BTreeSet<(&'static str, &'static str)> {
    let stages = Stage::ALL
        .into_iter()
        .map(|stage| (stage.parent().label(), stage.label()));
    let remainders = Parent::ALL
        .into_iter()
        .map(|parent| (parent.label(), "unattributed"));

    stages.chain(remainders).collect()
}

#[test]
fn stages_partitions_parent_costs_and_counts_real_work() {
    let output = run("stages", PATCH);
    assert!(output.status.success(), "{:?}", output);
    let text = String::from_utf8(output.stdout).expect("UTF-8 report");
    let lines = text.lines().collect::<Vec<_>>();

    let startup = fields(lines[1]);
    let stages = lines[3..lines.len() - 1]
        .iter()
        .map(|line| stage_record(line))
        .collect::<BTreeMap<_, _>>();
    let parent_sum = |parent: &str| -> u128 {
        stages
            .iter()
            .filter(|((owner, _), _)| *owner == parent)
            .map(|(_, duration)| duration)
            .sum()
    };
    let work = fields(lines.last().expect("work counters"));

    assert!(lines[0].starts_with("diagnostic "));
    assert_eq!(lines[2], "profile version=1");
    assert_eq!(
        stages.keys().copied().collect::<BTreeSet<_>>(),
        expected_stages()
    );
    assert_eq!(parent_sum("preparation"), startup["preparation_ns"]);
    assert_eq!(parent_sum("syntax"), stages[&("preparation", "syntax")]);
    assert_eq!(work["hunks"], 1);
    assert_eq!(work["parse_calls"], 2);
    assert_eq!(work["projected_bytes"], 24);
    assert!(work["captures"] > 0);
}

#[test]
fn malformed_input_never_emits_success_measurements() {
    for driver in Driver::ALL {
        let output = run(driver.label(), "not a patch\n");

        assert_trouble(&output);
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn empty_input_still_emits_zero_count_measurements() {
    for mode in ["prepare", "stages"] {
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
        if mode == "stages" {
            assert_eq!(lines[2], "profile version=1");
            let work = fields(lines.last().expect("work counters"));
            assert!(work.values().all(|count| *count == 0));
        }
    }
}

#[test]
fn movement_times_each_key_in_every_layout_detail_and_wrapping_mode() {
    let output = run("movement", PATCH);
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).expect("UTF-8 report");

    let modes = text
        .lines()
        .map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let [kind, version, layout, detail, wrap, key, step] = fields[..] else {
                panic!("movement line shape: {line}");
            };
            assert_eq!((kind, version), ("movement", "version=2"), "{line}");
            assert!(
                matches!(key, "key=l" | "key=h" | "key=j" | "key=k"),
                "{line}"
            );
            let (_, step_ns) = step.split_once("step_ns=").expect("step duration");
            step_ns.parse::<u128>().expect("integer step duration");
            (layout, detail, wrap)
        })
        .collect::<BTreeSet<_>>();

    let expected = ["layout=unified", "layout=vertical", "layout=stacked"]
        .into_iter()
        .flat_map(|layout| ["detail=line", "detail=character"].map(|detail| (layout, detail)))
        .flat_map(|(layout, detail)| ["wrap=off", "wrap=on"].map(|wrap| (layout, detail, wrap)))
        .collect::<BTreeSet<_>>();
    assert_eq!(modes, expected);
}

#[test]
fn unsupported_language_has_no_syntax_parser_work() {
    let output = run("stages", &PATCH.replace("a.rs", "a.txt"));
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("UTF-8 report");
    let work = fields(text.lines().last().expect("work counters"));
    assert_eq!(work["hunks"], 1);
    assert_eq!(work["parse_calls"], 0);
    assert_eq!(work["captures"], 0);
}

#[test]
fn stdin_and_operand_comparison_report_the_acquired_patch_size() {
    let env = TestEnv::new();
    let patch = env.write_file("input.patch", PATCH);
    let stdin = env
        .hdiff()
        .args(["--bench", "prepare"])
        .stdin(Stdio::from(fs::File::open(patch).expect("patch input")))
        .output()
        .expect("stdin benchmark");
    assert!(stdin.status.success());
    let text = String::from_utf8(stdin.stdout).expect("report");
    assert_eq!(
        fields(text.lines().nth(1).expect("startup"))["bytes"],
        PATCH.len() as u128
    );

    let old = env.write_file("old.rs", "fn old() {}\n");
    let new = env.write_file("new.rs", "fn new() {}\n");
    let comparison = env
        .hdiff()
        .args(["--bench", "stages"])
        .arg(old)
        .arg(new)
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
