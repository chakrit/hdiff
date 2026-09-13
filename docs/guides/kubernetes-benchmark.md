# Kubernetes startup benchmarking and profiling

Use an unprofiled comparison to judge a change and a separate profile to locate its cost.
The measurement boundaries and output schemas are in
[the performance specification](../spec/performance.md).

Run the reproducible Kubernetes preparation benchmark from the repository root:

```sh
bench/kubernetes.sh
```

Run only the smallest range while checking the measurement setup with:

```sh
bench/kubernetes.sh --range 7/8
```

The comparison uses the baseline, candidate, corpus, build policy, and scheduler priority
defined in the [performance contract](../spec/performance.md#measurement-model).

Measure the current checkout without requiring a code change:

```sh
bench/kubernetes.sh --mode measure --range 7/8
```

This mode describes the current cost without producing an improvement verdict.

Capture diagnostic stage timings and a separate stack sample:

```sh
bench/kubernetes.sh --mode profile --range 7/8
```

Read the [diagnostic profile contract](../spec/performance.md#diagnostic-profiles)
to interpret stage timings. Inspect the stack sample status and coverage before using
it to explain a stage; a short attachment window is not whole-process attribution.

For a small patch, inspect the application's reports directly:

```sh
target/release/hdiff --bench tests/fixtures/mixed-language-layout.patch
target/release/hdiff --profile tests/fixtures/mixed-language-layout.patch
```

Use the [benchmark output contract](../spec/performance.md#benchmark-mode) to
distinguish preparation, input-to-ready, and whole-process measurements.

Add every new candidate build-input file to the Git index before running the benchmark.
The script rejects untracked Cargo manifests, toolchain and Cargo configuration, build
scripts, and Rust source so the candidate digest covers every file used by the build.

Read the [measurement model](../spec/performance.md#measurement-model) for paired
acceptance, supporting metrics, and legacy-output handling.

Find generated patches, worktrees, source diffs, raw timings, result lines, and provenance
in `.ace/bench/kubernetes/`. Use run IDs to connect summaries to raw files, including
evidence retained from failed runs.

Keep the machine free of competing heavy work while measuring. Start with `7/8` to check
the setup, then run all ranges. Inspect raw paired measurements and memory/instruction
controls before accepting a timing difference. Profiles identify candidates for an
optimization pass; only an unprofiled comparison can establish its benefit.

Check the runner without building hdiff or loading Kubernetes:

```sh
sh tests/benchmark-runner.sh
```

The isolated fixtures exercise measurement arithmetic, output validation, provenance,
input mutation, failed work, and sampling outcomes.

Keep optimization summaries in `.ace/bench/kubernetes/results.txt` with their raw
measurements and provenance. Report measured cycle and preparation-time savings from
those records. Instrumentation overhead and diagnostic profiles retain their actual
verdicts separately from accepted optimizations. Update the completed or rejected
investigation boundaries in `docs/spec/performance.md` when a slice settles one.
