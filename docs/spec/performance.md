---
status: accepted
---

# Performance contract

This file is authoritative for preparation, measurement, and performance optimization.
The existing `--bench` and `--profile` measurement contract defines the preparation target.

## Preparation boundary

`hdiff` completes parsing, layout derivation, Tree-sitter syntax highlighting, and
character detail before entering the terminal loop. The loop receives fully prepared data
and has no parser, cache, readiness, loading, or first-file state.
It owns only interaction, terminal lifecycle, redraw decisions, and rendering of the
prepared active viewport. No async runtime, executor, channel, or task dependency is
part of this boundary.

Preparation is one optimized single-threaded pass. Incremental preparation is not part of
this design unless measured preparation latency makes the eager boundary untenable.

Document-wide file-list labels are derived once during preparation. Each prepared file
derives only its own rendered rows, split pairs, syntax spans, and character-detail spans.
The prepared layout is the sole owner of each exact-sized immutable rendered-row buffer.
Unified and side-by-side layout projections reference those buffers.

## Benchmark mode

Zero-byte input still emits valid zero-count reports in `--bench` and `--profile`.

`hdiff --bench` accepts the normal diff input, completes the normal preparation pass,
prints machine-readable result lines, and exits without opening the terminal. The first
result reports preparation time and the prepared file and record counts as
`preparation duration_ns=<integer> files=<integer> records=<integer>`.
The preparation measurement starts immediately before the shared preparation pass.
The second line has this versioned schema, with integer nanosecond durations:

```text
startup version=1 input_ns=N parse_ns=N preparation_ns=N ready_ns=N bytes=N
```

Input time includes reading stdin or the patch file, or producing a two-operand comparison.
Parse time covers unified-diff parsing. Ready time runs from immediately before input
acquisition until preparation completes, including orchestration overhead. Reporting,
terminal setup, first draw, and process teardown are excluded. Input bytes count the
complete acquired patch. Preparation time has the same meaning in both result lines.
Whole-process measurements remain separate. Prepared patch runs exclude upstream Git
production latency; reading a live pipe can include waiting on its producer.

### Diagnostic profiles

`hdiff --profile` accepts the same input sources and executes the same preparation
algorithm with explicit stage observation. Its first line uses `diagnostic` in place of
`preparation`; its startup line uses the same schema. It then emits `profile version=1`,
stage lines, and a workload line:

```text
stage name=NAME parent=preparation|syntax duration_ns=N
work hunks=N parse_calls=N captures=N projected_bytes=N
```

Preparation children are `labels`, `layout`, `syntax`, `detail`, and `unattributed`.
Syntax children are `language`, `projection`, `parse`, `query`, and `unattributed`.
Parse includes incremental-tree editing; query includes capture traversal and mapping.
Child durations partition their parent; parent and child durations are never summed
together. Unattributed time includes orchestration, destruction, and observer bookkeeping
outside named spans. Counts include attempted parse calls and all returned captures.
Projected bytes count the old and new virtual source buffers, including separators.

Normal rendering and benchmark comparisons have no per-hunk clock reads or counters.
Static dispatch selects observation at preparation entry. Profiles are diagnostic runs;
their timings never enter benchmark acceptance calculations. Reports are written only
after successful preparation, and malformed input produces no success record.

## Measurement model

### Movement diagnostics

The ignored `terminal::movement_measurement::measures_prepared_movement` test measures
the production navigation-bounds calculation and prepared frame renderer independently.
Run it with:

```sh
nice -n 19 cargo test --release --locked -j 1 measures_prepared_movement -- --ignored --nocapture
```

It uses a 120-by-36 Ratatui test backend and three generated Rust files,
with 16, 64, 256, or 1,024 seven-line hunks per file, six context records per hunk, and long
replacement strings that exercise horizontal clipping. This is a synthetic scaling
workload, not a recorded user diff.

Preparation runs once per workload and is shared by all six layout/detail combinations.
Each combination draws an initial frame, warms one `l/h/j/k` sequence, and records three
further sequences. Bounds, redraw, and combined input-to-frame durations use nanoseconds.
Redraw includes file-list derivation
and the test backend's buffer update. These measurements exclude terminal event reads,
PTY transport, terminal-emulator painting, and preparation; they are not end-to-end
keypress latency. Preparation is reported separately as one diagnostic observation.
No timing assertion or optimization acceptance verdict is derived from this diagnostic.

#### Initial movement baseline

The September 11, 2026 diagnostic measures the extracted production bounds calculation
and the prepared frame renderer. It ran on macOS arm64
with Rust 1.98.1, release optimization, one build job, and scheduler priority 19.
The original record names `.ace/movement-baseline-final.txt` as its raw output;
that file is not available in this checkout. The imported baseline and its source
provenance are preserved in `.ace/merge-incoming-performance.md`.

Median durations across twelve measured `l/h/j/k` inputs, in milliseconds:

| Hunks per file | Layout   | Detail    | Bounds | Redraw | Combined |
|----------------|----------|-----------|--------|--------|----------|
| 16             | Unified  | Line      | 0.049  | 0.494  | 0.542    |
| 64             | Unified  | Line      | 0.178  | 0.505  | 0.684    |
| 256            | Unified  | Line      | 1.850  | 2.436  | 4.285    |
| 1,024          | Unified  | Line      | 25.847 | 28.140 | 54.062   |
| 1,024          | Unified  | Character | 25.835 | 28.499 | 54.300   |
| 1,024          | Vertical | Line      | 25.892 | 28.109 | 54.005   |
| 1,024          | Vertical | Character | 25.892 | 28.538 | 54.422   |
| 1,024          | Stacked  | Line      | 25.868 | 28.667 | 54.551   |
| 1,024          | Stacked  | Character | 25.867 | 29.023 | 54.903   |

Bounds and redraw medians are independent and need not sum to the combined median.
The largest workload has 24,576 source records across three files; navigation stays
near the top of the second file. These results establish synthetic scaling, not a
latency guarantee for a real user diff or terminal. No optimization or preparation-time
saving is claimed.

Source inspection identifies the repeated work: `Layout::shows_unified_row` scans all
rendered rows for each context row; bounds scan the selected file for both visible
count and maximum width; the frame renderer constructs both complete remaining layout
projections before clipping to the active pane. The bounds scan's roughly fourteenfold
growth from 256 to 1,024 hunks is consistent with that quadratic context lookup.

Verification passed 95 unit tests, 18 integration tests, the explicit diagnostic,
formatting, and Clippy across all targets and features with warnings denied. The final
diagnostic compilation took 9.87 seconds; the subsequent suite compiled in 0.09 seconds
and Clippy completed in 10.37 seconds. The original record references raw verification
in `.ace/movement-tests.txt` and `.ace/movement-clippy.txt`; those files and earlier
diagnostic captures are unavailable in this checkout. The first capture included
record counting in its preparation timer and was superseded by the final capture.

### Preparation comparisons

Optimization comparisons measure the complete `hdiff` process with macOS
`/usr/bin/time -lp`. Elapsed CPU cycles are the execution-cost measurement. Retired
instructions are the stable-work control. Process user and system time, preparation wall
time, maximum resident set size, peak memory footprint, page faults, and context switches
are supporting evidence.

The baseline is the current commit of the hdiff repository. The candidate is that
repository's dirty tracked working tree. The benchmark records the baseline commit and a
SHA-256 digest of the candidate build-input diff; Kubernetes revisions identify only the
input corpus. Build inputs are Cargo manifests, toolchain and Cargo configuration, build
scripts, and Rust source. The candidate may contain staged or unstaged tracked changes.
The benchmark rejects untracked build inputs so the digest covers every file used by the
candidate build.

The runner also supports `--mode measure` and `--mode profile` for a clean or dirty current
checkout. Comparison is the default mode. Measurement repeats the current binary three
times after two warmups and does not produce an improvement verdict. Profiles run
separately and preserve stage output and an OS stack sample. A stack sample reports its
sampling window and completion status and never claims whole-process coverage when
attachment or completion cuts that window short. Sampling records distinguish captured,
empty, and unavailable stacks and preserve the sampler exit status. A failed sampler
leaves the successful stage report available.

Every run preserves a manifest and raw files beneath a unique run directory, including
failed runs. Provenance includes UTC time, hardware and OS, Rust and Cargo versions, build
flags and commands, source revision and build-input diff, runner digest, binary digests,
and corpus digest and size. Both binaries use the same explicit installed stable Rust toolchain and
locked dependencies. Source and binary identity are checked across measurement. Builds
and corpus generation happen outside the timed interval, with workloads run serially.

Each Kubernetes range warms the baseline and candidate twice, then measures three pairs
in baseline-candidate, candidate-baseline, baseline-candidate order. A pair's saving is
the baseline measurement minus the candidate measurement. The mean cycle saving must be
positive and greater than the maximum absolute deviation among the three paired savings.
Mean retired-instruction and peak-memory savings must not be negative by more than their
own maximum absolute deviations. A comparison that does not meet every condition cannot
be accepted as improved.

The paired dispersion rule is an acceptance heuristic, not a statistical confidence
interval. Cycles, instructions, or peak memory worsening beyond their paired dispersion
produce `regressed`; remaining comparisons are `inconclusive`. Missing or malformed
samples fail the run. Legacy baselines with only the preparation line remain comparable
for that metric; startup measurements are unavailable, never zero. Diagnostic records
are rejected by the benchmark reader. Raw and summary records carry their run ID.

The benchmark appends every raw sample and its order before appending the derived range
summary. Existing raw measurements are never replaced. Baseline and candidate file and
record counts must agree for every pair. Raw evidence includes inconclusive and regressed
runs; only accepted optimizations enter the optimization performance record.
Instrumentation verification and diagnostic reports retain their actual verdicts in a
separate verification record. The shared result log receives summaries only after the
entire run succeeds; a failed multi-range run retains completed ranges in its own raw
result file.

The one-second target is user-visible preparation latency. Quiet-machine wall-clock runs
confirm milestone progress and the final target; routine optimization acceptance does not
depend on precise wall-clock measurement.

## Representative corpus

Performance measurements use a local checkout of `kubernetes/kubernetes` at a pinned
commit. The benchmark streams only Go changes from three historical ranges ending at that
commit.

The reproducible corpus is Kubernetes v1.32.0. `bench/kubernetes.sh` owns its checkout
below `.ace/bench/kubernetes/`, builds the current hdiff commit in a cached linked
worktree and the dirty candidate at ordinary scheduler priority, and appends raw samples
and summaries to `.ace/bench/kubernetes/results.txt`. Benchmark commands are never
de-prioritized. Each result identifies both measured hdiff states.

Let `D` be the number of first-parent edges from the repository root to the pinned commit.
The three base commits are at first-parent indices `floor(D * 7 / 8)`,
`floor(D * 3 / 4)`, and `floor(D / 2)` from the root. Diffing each base against the pinned
commit gives the final eighth, quarter, and half of the repository history respectively.

The resolved commit pairs and their results are recorded with each benchmark run. The
benchmark measures the complete preparation pass; it does not model an interactive
terminal session.

## Optimization investigation record

Completed and rejected investigations are retained below. Raw measurements and source
provenance remain under `.ace/bench/kubernetes/`; the earlier measurement record is
preserved in `.ace/spec-cleanup-performance-before.md`.

### Completed and rejected investigations

- **Prepared-file layout.** Each prepared file derives its rows without rebuilding the
  document-wide file list.
- **Syntax-span projection.** Span mapping begins at the first record overlapping each
  source event.
- **Sanitized source reuse.** Rendering reuses parser-sanitized source text.
- **Direct syntax-query pass.** A purpose-built public Tree-sitter query pass
  incrementally reparses each same-language old/new hunk pair and maps captures directly
  to hdiff syntax classes.
- **Line-detail omission.** Line granularity carries no character detail and does not
  retain a row-aligned empty table.
- **Shared rendered buffers.** Unified and side-by-side layout projections share each
  rendered row's exact-sized immutable byte buffer.
- **Index-owned rendered rows.** The prepared layout owns rendered rows directly; both
  layout projections carry indices instead of reference-counted row copies.
- **Tree-sitter 0.27 parser core.** Syntax preparation uses the upstream ASCII decoding and
  position-advance fast paths while retaining the public query boundary.
- **Per-file syntax projection.** Concatenating each side's hunks into one projection per
  file regressed preparation time and is rejected.

## Optimization method

Optimization follows the [engineering standard](architecture.md#engineering-standard).
The one-second Kubernetes preparation target is reached through successive proper
refactors. Each slice starts by understanding the
actual data flow and cost, then chooses the most elegant structural change that makes the
fast path the natural implementation.

An optimization must improve the code as well as its runtime. Prepared and rendered
output remain correct because the design makes them correct. When no clean and measured
improvement exists, leave the code alone.

Each optimization slice reports its measured cycle saving and preparation-time saving
from the benchmark comparison. Savings are derived from the authoritative measurements;
they are not stored as a second fact. A candidate that is not accepted does not advance
the optimization record.
