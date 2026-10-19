mod common;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    process::{Output, Stdio},
};

use common::{TestEnv, assert_error};

/// docs/spec/performance.md#benchmark-drivers: the drivers `--bench` accepts.
const DRIVERS: [&str; 3] = ["prepare", "stages", "movement"];

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

/// The records of a successful report, which writes nothing to standard error.
fn report(output: Output) -> String {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");

    String::from_utf8(output.stdout).expect("UTF-8 report")
}

fn records(report: &str) -> Vec<Record<'_>> {
    report.lines().map(Record::parse).collect()
}

fn patch_bytes() -> u128 {
    u128::try_from(PATCH.len()).expect("patch length fits u128")
}

/// docs/spec/performance.md#stages: every stage under its parent, plus each parent's
/// unattributed remainder.
fn expected_stages() -> BTreeSet<(&'static str, &'static str)> {
    let preparation =
        ["labels", "layout", "syntax", "detail", "unattributed"].map(|name| ("preparation", name));
    let syntax =
        ["language", "projection", "parse", "query", "unattributed"].map(|name| ("syntax", name));

    preparation.into_iter().chain(syntax).collect()
}

/// One report line: its kind, then its `key=value` fields.
struct Record<'a> {
    line: &'a str,
    kind: &'a str,
    fields: BTreeMap<&'a str, &'a str>,
}

impl<'a> Record<'a> {
    fn parse(line: &'a str) -> Self {
        let mut words = line.split_whitespace();
        let kind = words
            .next()
            .unwrap_or_else(|| panic!("record kind: {line:?}"));
        let fields = words
            .map(|field| {
                field
                    .split_once('=')
                    .unwrap_or_else(|| panic!("key=value field {field:?} in {line:?}"))
            })
            .collect();

        Self { line, kind, fields }
    }

    fn keys(&self) -> BTreeSet<&'a str> {
        self.fields.keys().copied().collect()
    }

    fn text(&self, key: &str) -> &'a str {
        self.fields
            .get(key)
            .copied()
            .unwrap_or_else(|| panic!("{key} in {:?}", self.line))
    }

    fn integer(&self, key: &str) -> u128 {
        self.text(key)
            .parse()
            .unwrap_or_else(|_| panic!("integer {key} in {:?}", self.line))
    }
}

#[test]
fn benchmark_accounts_for_input_and_parsing_before_preparation() {
    let text = report(run("prepare", PATCH));
    let records = records(&text);

    let [preparation, startup] = &records[..] else {
        panic!("preparation and startup records: {text}");
    };
    assert_eq!(preparation.kind, "preparation", "{text}");
    assert_eq!(startup.kind, "startup", "{text}");
    assert_eq!(preparation.integer("files"), 1);
    assert_eq!(preparation.integer("records"), 2);
    assert_eq!(startup.integer("version"), 1);
    assert_eq!(startup.integer("bytes"), patch_bytes());
    assert_eq!(
        startup.integer("preparation_ns"),
        preparation.integer("duration_ns")
    );
    assert!(
        startup.integer("ready_ns")
            >= startup.integer("input_ns")
                + startup.integer("parse_ns")
                + startup.integer("preparation_ns"),
        "{text}"
    );
}

#[test]
fn stages_partitions_parent_costs_and_counts_real_work() {
    let text = report(run("stages", PATCH));
    let records = records(&text);

    let [diagnostic, startup, profile, stage_records @ .., work] = &records[..] else {
        panic!("diagnostic, startup, profile, stage and work records: {text}");
    };
    let stages = stage_records
        .iter()
        .map(|record| {
            assert_eq!(record.kind, "stage", "{text}");
            let key = (record.text("parent"), record.text("name"));
            (key, record.integer("duration_ns"))
        })
        .collect::<BTreeMap<_, _>>();
    let parent_sum = |parent: &str| -> u128 {
        stages
            .iter()
            .filter(|((owner, _), _)| *owner == parent)
            .map(|(_, duration)| duration)
            .sum()
    };

    assert_eq!(diagnostic.kind, "diagnostic", "{text}");
    assert_eq!(profile.line, "profile version=1", "{text}");
    assert_eq!(
        stages.keys().copied().collect::<BTreeSet<_>>(),
        expected_stages()
    );
    assert_eq!(parent_sum("preparation"), startup.integer("preparation_ns"));
    assert_eq!(parent_sum("syntax"), stages[&("preparation", "syntax")]);
    assert_eq!(work.integer("hunks"), 1);
    assert_eq!(work.integer("parse_calls"), 2);
    assert_eq!(work.integer("projected_bytes"), 24);
    assert!(work.integer("captures") > 0, "{text}");
}

#[test]
fn malformed_input_never_emits_success_measurements() {
    for driver in DRIVERS {
        let output = run(driver, "not a patch\n");

        assert_error(&output, "input contains no diff");
    }
}

#[test]
fn empty_input_still_emits_zero_count_measurements() {
    for driver in ["prepare", "stages"] {
        let text = report(run(driver, ""));
        let records = records(&text);
        let preparation = &records[0];
        let startup = &records[1];

        assert_eq!(preparation.integer("files"), 0);
        assert_eq!(preparation.integer("records"), 0);
        assert_eq!(startup.integer("bytes"), 0);
        assert_eq!(startup.integer("version"), 1);
        if driver == "stages" {
            assert_eq!(records[2].line, "profile version=1", "{text}");
            let work = records.last().expect("work counters");
            assert!(
                work.keys().iter().all(|key| work.integer(key) == 0),
                "{text}"
            );
        }
    }
}

#[test]
fn movement_times_each_key_in_every_layout_detail_and_wrapping_mode() {
    let text = report(run("movement", PATCH));

    let modes = records(&text)
        .iter()
        .map(|record| {
            assert_eq!(record.kind, "movement", "{}", record.line);
            assert_eq!(
                record.keys(),
                BTreeSet::from(["version", "layout", "detail", "wrap", "key", "step_ns"]),
                "{}",
                record.line
            );
            assert_eq!(record.integer("version"), 2, "{}", record.line);
            assert!(
                matches!(record.text("key"), "l" | "h" | "j" | "k"),
                "{}",
                record.line
            );
            record.integer("step_ns");
            (
                record.text("layout"),
                record.text("detail"),
                record.text("wrap"),
            )
        })
        .collect::<BTreeSet<_>>();

    let expected = ["unified", "vertical", "stacked"]
        .into_iter()
        .flat_map(|layout| ["line", "character"].map(|detail| (layout, detail)))
        .flat_map(|(layout, detail)| ["off", "on"].map(|wrap| (layout, detail, wrap)))
        .collect::<BTreeSet<_>>();
    assert_eq!(modes, expected);
}

#[test]
fn unsupported_language_has_no_syntax_parser_work() {
    let text = report(run("stages", &PATCH.replace("a.rs", "a.txt")));
    let records = records(&text);
    let work = records.last().expect("work counters");

    assert_eq!(work.integer("hunks"), 1);
    assert_eq!(work.integer("parse_calls"), 0);
    assert_eq!(work.integer("captures"), 0);
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
    let text = report(stdin);
    assert_eq!(records(&text)[1].integer("bytes"), patch_bytes());

    let old = env.write_file("old.rs", "fn old() {}\n");
    let new = env.write_file("new.rs", "fn new() {}\n");
    let comparison = env
        .hdiff()
        .args(["--bench", "stages"])
        .arg(old)
        .arg(new)
        .output()
        .expect("operand profile");
    let text = report(comparison);
    let records = records(&text);
    assert_eq!(records[0].integer("records"), 2);
    assert!(records[1].integer("bytes") > 0, "{text}");
}
