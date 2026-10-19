mod common;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    process::{Output, Stdio},
};

use common::{TestEnv, assert_error, success_output};

/// docs/spec/performance.md#benchmark-drivers: the drivers `--bench` accepts.
const DRIVERS: [&str; 3] = ["prepare", "stages", "movement"];

const PATCH: &str = "--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";

fn run(driver: &str, patch: &str) -> Output {
    let env = TestEnv::new();
    env.write_file("input.patch", patch);

    env.run(["--bench", driver, "input.patch"])
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

    fn is_integer(&self, key: &str) -> bool {
        self.text(key).parse::<u128>().is_ok()
    }
}

#[test]
fn benchmark_accounts_for_input_and_parsing_before_preparation() {
    let text = success_output(&run("prepare", PATCH));
    let records = records(&text);

    let [preparation, startup] = &records[..] else {
        panic!("preparation and startup records: {text}");
    };
    assert_eq!(preparation.kind, "preparation", "{text}");
    assert_eq!(startup.kind, "startup", "{text}");
    assert_eq!(preparation.integer("files"), 1, "{text}");
    assert_eq!(preparation.integer("records"), 2, "{text}");
    assert_eq!(startup.integer("version"), 1, "{text}");
    assert_eq!(startup.integer("bytes"), patch_bytes(), "{text}");
    assert_eq!(
        startup.integer("preparation_ns"),
        preparation.integer("duration_ns"),
        "{text}"
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
    let text = success_output(&run("stages", PATCH));
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
        expected_stages(),
        "{text}"
    );
    assert_eq!(
        parent_sum("preparation"),
        startup.integer("preparation_ns"),
        "{text}"
    );
    assert_eq!(
        parent_sum("syntax"),
        stages[&("preparation", "syntax")],
        "{text}"
    );
    assert_eq!(work.integer("hunks"), 1, "{text}");
    assert_eq!(work.integer("parse_calls"), 2, "{text}");
    assert_eq!(work.integer("projected_bytes"), 24, "{text}");
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
fn empty_input_prepares_zero_files_and_records() {
    let text = success_output(&run("prepare", ""));
    let records = records(&text);

    let [preparation, startup] = &records[..] else {
        panic!("preparation and startup records: {text}");
    };
    assert_eq!(preparation.integer("files"), 0, "{text}");
    assert_eq!(preparation.integer("records"), 0, "{text}");
    assert_eq!(startup.integer("version"), 1, "{text}");
    assert_eq!(startup.integer("bytes"), 0, "{text}");
}

#[test]
fn empty_input_profiles_zero_files_records_and_work() {
    let text = success_output(&run("stages", ""));
    let records = records(&text);

    let [diagnostic, startup, profile, .., work] = &records[..] else {
        panic!("diagnostic, startup, profile, stage and work records: {text}");
    };
    assert_eq!(diagnostic.integer("files"), 0, "{text}");
    assert_eq!(diagnostic.integer("records"), 0, "{text}");
    assert_eq!(startup.integer("version"), 1, "{text}");
    assert_eq!(startup.integer("bytes"), 0, "{text}");
    assert_eq!(profile.line, "profile version=1", "{text}");
    assert!(
        work.keys().iter().all(|key| work.integer(key) == 0),
        "{text}"
    );
}

#[test]
fn movement_times_each_key_in_every_layout_detail_and_wrapping_mode() {
    let text = success_output(&run("movement", PATCH));

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
            assert!(record.is_integer("step_ns"), "{}", record.line);
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
    assert_eq!(modes, expected, "{text}");
}

#[test]
fn unsupported_language_has_no_syntax_parser_work() {
    let text = success_output(&run("stages", &PATCH.replace("a.rs", "a.txt")));
    let records = records(&text);
    let [.., work] = &records[..] else {
        panic!("work counters: {text}");
    };

    assert_eq!(work.integer("hunks"), 1, "{text}");
    assert_eq!(work.integer("parse_calls"), 0, "{text}");
    assert_eq!(work.integer("captures"), 0, "{text}");
}

#[test]
fn stdin_and_operand_comparison_report_the_acquired_patch_size() {
    let env = TestEnv::new();
    let patch = env.write_file("input.patch", PATCH);
    let piped = env
        .hdiff()
        .args(["--bench", "prepare"])
        .stdin(Stdio::from(fs::File::open(patch).expect("patch input")))
        .output()
        .expect("stdin benchmark");
    let piped_text = success_output(&piped);
    let piped_records = records(&piped_text);
    let [_, piped_startup] = &piped_records[..] else {
        panic!("preparation and startup records: {piped_text}");
    };
    assert_eq!(
        piped_startup.integer("bytes"),
        patch_bytes(),
        "{piped_text}"
    );

    env.write_file("old.rs", "fn old() {}\n");
    env.write_file("new.rs", "fn new() {}\n");
    let compared = env.run(["--bench", "stages", "old.rs", "new.rs"]);
    let compared_text = success_output(&compared);
    let compared_records = records(&compared_text);
    let [diagnostic, compared_startup, ..] = &compared_records[..] else {
        panic!("diagnostic and startup records: {compared_text}");
    };
    assert_eq!(diagnostic.integer("records"), 2, "{compared_text}");
    assert!(compared_startup.integer("bytes") > 0, "{compared_text}");
}
