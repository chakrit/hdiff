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

The script pins Kubernetes v1.32.0 as its input corpus. It builds the current hdiff commit
in a cached linked worktree, builds the dirty hdiff working tree as the candidate, and
measures Go-only changes from the final eighth, quarter, and half of the pinned Kubernetes
first-parent history. Both builds use installed stable Rust with locked dependencies.
Benchmark commands run serially at ordinary scheduler priority.

Measure the current checkout without requiring a code change:

```sh
bench/kubernetes.sh --mode measure --range 7/8
```

This mode warms the current binary twice and records three runs. It describes the current
cost and does not produce an improvement verdict.

Capture diagnostic stage timings and a separate stack sample:

```sh
bench/kubernetes.sh --mode profile --range 7/8
```

Stage timings account for input acquisition, unified-diff parsing, layout, syntax, and
character detail. Syntax is further divided into language initialization/lookup, source
projection, parsing/editing, and capture query/mapping. Child durations partition their
parent; do not add the syntax parent to its children. Unattributed time is included.
Stack sampling attaches to a separate process; inspect its status and coverage before
using it to explain a stage. A short attachment window is not whole-process attribution.
The sampling record distinguishes captured, empty, and unavailable stacks and preserves
the sampler exit status. A failed sampler leaves the successful stage report available.

For a small patch, inspect the application's reports directly:

```sh
target/release/hdiff --bench tests/fixtures/mixed-language-layout.patch
target/release/hdiff --profile tests/fixtures/mixed-language-layout.patch
```

The benchmark reports preparation and input-to-ready time separately. Input-to-ready
includes input acquisition and diff parsing but excludes terminal setup, first draw,
reporting, and destruction. Reading a generated patch excludes Git's production time;
reading from a live pipe can include waiting on Git. OS whole-process time includes
startup and destruction and must not be relabeled preparation time.

Add every new candidate build-input file to the Git index before running the benchmark.
The script rejects untracked Cargo manifests, toolchain and Cargo configuration, build
scripts, and Rust source so the candidate digest covers every file used by the build.

Each range warms both binaries twice, then runs three alternating baseline-candidate
pairs through macOS `/usr/bin/time -lp`. Cycles determine whether execution cost improved;
retired instructions control for completed work; CPU time, preparation wall time, memory,
page faults, and context switches explain the result. An improvement requires a cycle
saving above paired dispersion without instructions or peak memory regressing beyond
their dispersions. A regression beyond dispersion in any of those metrics is reported as
regressed; remaining results are inconclusive. This is an acceptance heuristic, not a
statistical confidence interval.

Historical binaries that emit only preparation timing remain usable as baselines.
Their startup fields are explicitly unavailable; they are never treated as zero or
compared against the candidate's startup fields. Diagnostic output cannot enter the
comparison calculation.

The script keeps the Kubernetes checkout, generated patches, baseline worktrees, candidate
source diffs, raw timing files, and append-only result lines in
`.ace/bench/kubernetes/`, which is intentionally untracked. Each result identifies the
hdiff baseline commit and the SHA-256 digest of the candidate build-input diff. Run IDs
connect summaries to raw files. Each run retains a provenance manifest, completion status,
source and binary identities, corpus hash and size, machine and toolchain information,
and exact measurement commands. Failed runs retain their evidence too.
The shared result log receives summaries only after the entire run succeeds; a failed
multi-range run retains completed ranges in its own raw result file.

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

Copy improved optimization summaries from `.ace/bench/kubernetes/results.txt` into the
optimization record in `docs/spec/performance.md`. Each summary records the corpus range,
both hdiff states, averaged counters, dispersion, verdict, and file and record counts.
Instrumentation overhead and diagnostic profiles belong in the separate verification
record with their actual verdicts. An accepted optimization is committed and becomes the
next optimization slice's baseline.
