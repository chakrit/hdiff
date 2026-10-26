mod common;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    process::Output,
};

use common::{TestEnv, assert_error, success_output};

/// docs/spec/performance.md#benchmark-drivers: the drivers `--bench` accepts.
const DRIVERS: [&str; 5] = ["prepare", "stages", "movement", "first-frame", "loading"];

const PATCH: &str = "--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";

/// hdiff's output for `--bench driver` over a patch file holding `patch`.
fn run_bench(driver: &str, patch: &str) -> Output {
    let env = TestEnv::new();
    env.write_file("input.patch", patch);

    env.run(["--bench", driver, "input.patch"])
}

/// Every line of a benchmark `report`, parsed as a record.
fn records_of(report: &str) -> Vec<Record<'_>> {
    report.lines().map(Record::parse).collect()
}

/// The size of [`PATCH`] in bytes, as reports state sizes.
fn patch_bytes() -> u128 {
    u128::try_from(PATCH.len()).expect("patch length fits u128")
}

/// docs/spec/performance.md#stages: every stage under its parent, plus each parent's
/// unattributed remainder.
fn expected_stages() -> BTreeSet<(&'static str, &'static str)> {
    let preparation =
        ["labels", "layout", "detail", "styling", "unattributed"].map(|name| ("preparation", name));

    preparation.into_iter().collect()
}

/// One report line: its kind, then its `key=value` fields.
struct Record<'a> {
    line: &'a str,
    kind: &'a str,
    fields: BTreeMap<&'a str, &'a str>,
}

impl<'a> Record<'a> {
    /// `line` split into its kind and fields; a field without `=` fails the test.
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

    /// The record's field names.
    fn keys(&self) -> BTreeSet<&'a str> {
        self.fields.keys().copied().collect()
    }

    /// The value of field `key`; a missing field fails the test.
    fn text(&self, key: &str) -> &'a str {
        self.fields
            .get(key)
            .copied()
            .unwrap_or_else(|| panic!("{key} in {:?}", self.line))
    }

    /// Field `key` as an integer; a missing or non-integer field fails the test.
    fn integer(&self, key: &str) -> u128 {
        self.text(key)
            .parse()
            .unwrap_or_else(|error| panic!("integer {key} in {:?}: {error}", self.line))
    }

    /// Whether field `key` holds an integer; a missing field fails the test.
    fn is_integer(&self, key: &str) -> bool {
        self.text(key).parse::<u128>().is_ok()
    }
}

#[test]
fn benchmark_accounts_for_input_and_parsing_before_preparation() {
    let text = success_output(&run_bench("prepare", PATCH));
    let records = records_of(&text);

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
    let text = success_output(&run_bench("stages", PATCH));
    let records = records_of(&text);

    let [diagnostic, startup, profile, stage_records @ .., work] = &records[..] else {
        panic!("diagnostic, startup, profile, stage and work records: {text}");
    };
    for record in stage_records {
        assert_eq!(record.kind, "stage", "{text}");
    }
    let stages = stage_records
        .iter()
        .map(|record| {
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
    assert_eq!(work.integer("hunks"), 1, "{text}");
    // docs/spec/performance.md#stages: the scanner reads "fn old() {}" and "fn new() {}",
    // 11 bytes each, and gives each a keyword and a function name.
    assert_eq!(work.integer("scanned_bytes"), 22, "{text}");
    assert_eq!(work.integer("segments"), 4, "{text}");
}

#[test]
fn input_without_a_diff_never_emits_success_measurements() {
    for driver in DRIVERS {
        let output = run_bench(driver, "not a patch\n");

        assert_error(&output, "input contains no diff");
    }
}

#[test]
fn empty_input_prepares_zero_files_and_records() {
    let text = success_output(&run_bench("prepare", ""));
    let records = records_of(&text);

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
    let text = success_output(&run_bench("stages", ""));
    let records = records_of(&text);

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
    let text = success_output(&run_bench("movement", PATCH));
    let records = records_of(&text);

    for record in &records {
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
    }
    let modes = records
        .iter()
        .map(|record| {
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
fn unsupported_language_has_no_scanner_work() {
    let text = success_output(&run_bench("stages", &PATCH.replace("a.rs", "a.txt")));
    let records = records_of(&text);
    let [.., work] = &records[..] else {
        panic!("work counters: {text}");
    };

    assert_eq!(work.integer("hunks"), 1, "{text}");
    assert_eq!(work.integer("scanned_bytes"), 0, "{text}");
    assert_eq!(work.integer("segments"), 0, "{text}");
}

#[test]
fn piped_patches_report_their_size() {
    let env = TestEnv::new();
    let patch = env.write_file("input.patch", PATCH);
    let input = fs::File::open(patch).expect("patch input");

    let output = env.run_with_stdin(["--bench", "prepare"], input);

    let text = success_output(&output);
    let records = records_of(&text);
    let [_, startup] = &records[..] else {
        panic!("preparation and startup records: {text}");
    };
    assert_eq!(startup.integer("bytes"), patch_bytes(), "{text}");
}

#[test]
fn operand_comparisons_report_their_records_and_the_size_of_diffs_output() {
    let env = TestEnv::new();
    env.write_file("old.rs", "fn old() {}\n");
    env.write_file("new.rs", "fn new() {}\n");

    let output = env.run(["--bench", "stages", "old.rs", "new.rs"]);

    let text = success_output(&output);
    let records = records_of(&text);
    let [diagnostic, startup, ..] = &records[..] else {
        panic!("diagnostic and startup records: {text}");
    };
    assert_eq!(diagnostic.integer("records"), 2, "{text}");
    assert!(startup.integer("bytes") > 0, "{text}");
}

/// docs/spec/performance.md#first-frame: the time from launch to the first frame, with
/// the files the first frame lists.
#[test]
fn first_frame_reports_the_time_to_the_first_frame() {
    let text = success_output(&run_bench("first-frame", PATCH));
    let records = records_of(&text);

    let [first_frame] = &records[..] else {
        panic!("one first-frame record: {text}");
    };
    assert_eq!(first_frame.kind, "first_frame", "{text}");
    assert_eq!(first_frame.integer("version"), 1, "{text}");
    assert_eq!(first_frame.integer("files"), 1, "{text}");
}

/// docs/spec/performance.md#loading: a summary, then one record per key answered while
/// files arrived and one per update shown.
#[test]
fn loading_reports_each_key_and_update_wait() {
    let text = success_output(&run_bench("loading", &PATCH.repeat(3)));
    let records = records_of(&text);

    let [summary, samples @ ..] = &records[..] else {
        panic!("a loading summary: {text}");
    };
    let count = |kind| samples.iter().filter(|record| record.kind == kind).count();
    assert_eq!(summary.kind, "loading", "{text}");
    assert_eq!(
        count("loading_key") as u128,
        summary.integer("keys"),
        "{text}"
    );
    assert_eq!(
        count("update_frame") as u128,
        summary.integer("updates"),
        "{text}"
    );
    assert!(summary.integer("updates") > 0, "{text}");
}
