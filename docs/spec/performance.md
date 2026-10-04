---
status: accepted
---

# Performance contract

This file is authoritative for preparation, measurement, and performance optimization.

## Preparation boundary

`hdiff` completes parsing, layout derivation, Tree-sitter syntax highlighting, and
character detail before entering the terminal loop. The loop receives fully prepared data
and has no parser, readiness, loading, or first-file state.
It owns only interaction, terminal lifecycle, redraw decisions, and rendering of the
prepared active viewport. No async runtime, executor, channel, or task dependency is
part of this boundary.

Preparation is one optimized single-threaded pass. Incremental preparation is not part of
this design unless measured preparation latency makes the eager boundary untenable.

Document-wide file-list labels are derived once during preparation. Each prepared file
derives only its own rendered rows, split pairs, syntax spans, and character-detail spans.
The prepared layout is the sole owner of each exact-sized immutable rendered-row buffer.
Unified and side-by-side layout projections reference those buffers.

## Benchmark drivers

`hdiff --bench <driver>` accepts the normal diff input after the driver name, completes
the normal preparation pass, runs the named driver, prints machine-readable result lines,
and exits without opening the terminal. `--profile` does not exist. Zero-byte input still
emits valid zero-count reports. Reports are written only after successful preparation,
and malformed input produces no success record.

Drivers are variants of one `bench::Driver` enum behind a single entry point: read, parse,
prepare, run the driver, report. The drivers are `prepare`, `stages`, and `movement`.

### `prepare`

The first result reports preparation time and the prepared file and record counts as
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

### `stages`

`stages` executes the same preparation algorithm with explicit stage observation. Its
first line uses `diagnostic` in place of `preparation`; its startup line uses the same
schema. It then emits `profile version=1`, stage lines, and a workload line:

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

Normal rendering, `prepare`, and `movement` have no per-hunk clock reads or counters.
Static dispatch selects observation at preparation entry. `stages` runs are diagnostic;
their timings never enter benchmark acceptance calculations.

### `movement`

`movement` loads the prepared document into a `Tui` drawing to a 120-by-36
`Terminal<CrosstermBackend<io::Sink>>` and times each `tui.step` over a fixed `l/h/j/k`
key sequence in every layout and detail mode. It emits one line per key:

```text
movement version=1 layout=NAME detail=NAME key=KEY step_ns=N
```

Step time covers input mapping, interaction transition, frame rendering, and escape
encoding into the sink. It excludes terminal event reads, PTY transport, and
terminal-emulator painting.

## Targets

Targets are recorded here and reviewed from script output. Tests never assert timing.

| Measurement                       | Target                                   |
|-----------------------------------|------------------------------------------|
| Kubernetes preparation            | 1 second                                 |
| `movement` step, worst case       | 1 ms                                     |

The reference machine is `gz44`, an M1 Mac.

## Measurement scripts

Scripts build the release binary and drive `--bench` from outside.

- `bench/kubernetes.sh` measures and compares preparation on the Kubernetes corpus and
  writes to `.ace/bench/kubernetes/`.
- `bench/fixtures.sh` runs every driver over the repository fixtures and the generated
  1,024-hunk movement workload and writes raw output and a summary to
  `.ace/bench/fixtures/`.

## Measurement model

### Movement workload

The movement workload is three generated Rust files with 16, 64, 256, or 1,024
seven-line hunks per file, six context records per hunk, and long replacement strings
that exercise horizontal clipping. This is a synthetic scaling workload, not a recorded
user diff.

#### Initial movement baseline

The September 11, 2026 baseline was taken by the former ignored
`measures_prepared_movement` test, which timed the bounds calculation and a 120-by-36
Ratatui test-backend redraw separately. It ran on macOS arm64
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

#### Constant-time context visibility

`Layout` derives each rendered line's visibility once: always shown, shown within a
record distance of the nearest change in its hunk, or never shown. It also derives each
line's tab-expanded width once, so movement bounds compare stored numbers. The frame
renderer builds rows for the active layout only, from the viewport, at most one screen
tall.

`bench/fixtures.sh` on `gz44` at scheduler priority 19, worst `movement` step on the
1,024-hunk workload across all layouts, detail modes, and keys:

| Run                              | Revision                 | Worst step |
|----------------------------------|--------------------------|------------|
| `20260927T131110Z-e1f4d80-98479` | `e1f4d80`                | 216.923 ms |
| `20260927T132036Z-e1f4d80-19824` | `e1f4d80` + visibility   | 1.598 ms   |
| `20260927T132603Z-343cb84-27077` | `343cb84` + widths       | 0.441 ms   |

With visibility alone, bounds dominated the remaining step by re-expanding every
visible row's tabs to find the maximum width. Workload preparation stayed at
142–150 ms across the three runs, within run-to-run variation; the Kubernetes
preparation benchmark was not run for these slices.

#### Cached layout splits

Ratatui's `layout-cache` feature memoizes `Layout::split` instead of running its solver
every frame. The whole `--bench movement` process on the 1,024-hunk workload (96 steps,
including preparation), measured with `/usr/bin/time -lp` on `gz44` in three
baseline-candidate pairs:

| Binary           | Retired instructions (three runs)             | Mean          |
|------------------|-----------------------------------------------|---------------|
| `95ccc17`        | 3,137,491,182 · 3,141,996,319 · 3,140,215,102 | 3,139,900,868 |
| + `layout-cache` | 3,029,821,341 · 3,029,504,257 · 3,025,169,607 | 3,028,165,068 |

The mean saving is 111.7 million retired instructions per run (paired savings 107.7,
112.5, and 115.0 million), about 1.16 million per step. Elapsed cycles fell from about
663 million to 625 million. `bench/fixtures.sh` at scheduler priority 19, worst
`movement` step on the 1,024-hunk workload:

| Run                              | Revision                   | Worst step |
|----------------------------------|----------------------------|------------|
| `20260927T142023Z-95ccc17-69390` | `95ccc17`                  | 0.440 ms   |
| `20260927T142059Z-95ccc17-72599` | `95ccc17` + `layout-cache` | 0.296 ms   |

The feature adds `critical-section` to the lock file; `lru` was already present. The
Kubernetes preparation benchmark was not run: preparation renders no frame.

#### Direct row writing

Each diff row is written straight into the frame buffer, with one grapheme segmentation
of the whole expanded row, instead of a `Paragraph` per row. The row borrows prepared
text; nothing is copied into a `Line`. Same measurement as
[Cached layout splits](#cached-layout-splits):

| Binary          | Retired instructions (three runs)             | Mean          |
|-----------------|-----------------------------------------------|---------------|
| `8c4d462`       | 3,029,867,528 · 3,029,862,946 · 3,024,736,951 | 3,028,155,808 |
| + row writer    | 2,965,450,185 · 2,975,084,988 · 2,966,034,094 | 2,968,856,422 |

The mean saving is 59.3 million retired instructions per run (paired savings 64.4,
54.8, and 58.7 million), about 0.62 million per step. Elapsed cycles fell from about
622 million to 606 million. `bench/fixtures.sh` at scheduler priority 19, worst
`movement` step on the 1,024-hunk workload, from 0.296 ms:

| Run                              | Revision                 | Worst step |
|----------------------------------|--------------------------|------------|
| `20260927T142701Z-8c4d462-88695` | `8c4d462` + row writer   | 0.234 ms   |

#### Stored scroll extents

The interactive loop keeps the visible rows and widest row of the selected file, layout,
and context count, and rebuilds them only when one of those changes. Movement bounds read
the stored extent instead of rescanning the file. Same measurement as
[Cached layout splits](#cached-layout-splits):

| Binary          | Retired instructions (three runs)             | Mean          |
|-----------------|-----------------------------------------------|---------------|
| `4b8eeab`       | 2,968,316,164 · 2,967,673,289 · 2,971,913,966 | 2,969,301,140 |
| + extents       | 2,929,017,955 · 2,928,051,315 · 2,929,909,546 | 2,928,992,939 |

The mean saving is 40.3 million retired instructions per run (paired savings 39.3, 39.6,
and 42.0 million), about 0.42 million per step. `bench/fixtures.sh` at scheduler
priority 19, worst `movement` step on the 1,024-hunk workload, from 0.234 ms:

| Run                              | Revision                 | Worst step |
|----------------------------------|--------------------------|------------|
| `20260927T143503Z-4b8eeab-97473` | `4b8eeab` + extents      | 0.214 ms   |

Workload preparation read 158.7 ms in this run against 146–148 ms in the two before
it; the `prepare` driver does not build extents.

#### Static frame chrome

The file list renders borrowed labels, and only the window that fits around the
selection; footer hints are static strings; separators fill buffer cells. Same
measurement as [Cached layout splits](#cached-layout-splits):

| Binary          | Retired instructions (three runs)             | Mean          |
|-----------------|-----------------------------------------------|---------------|
| `7136b58`       | 2,929,023,090 · 2,929,609,157 · 2,929,803,546 | 2,929,478,598 |
| + chrome        | 2,915,201,921 · 2,915,378,890 · 2,911,096,224 | 2,913,892,345 |

The mean saving is 15.6 million retired instructions per run (paired savings 13.8,
14.2, and 18.7 million), about 0.16 million per step. The workload has three files, so
this does not measure the per-label saving on large corpora. `bench/fixtures.sh` at
scheduler priority 19, worst `movement` step on the 1,024-hunk workload, from 0.214 ms:

| Run                              | Revision                 | Worst step |
|----------------------------------|--------------------------|------------|
| `20260927T143809Z-7136b58-3764`  | `7136b58` + chrome       | 0.187 ms   |

### Preparation comparisons

Optimization comparisons measure the complete `hdiff` process with macOS
`/usr/bin/time -lp`. Retired instructions are the primary measurement. Wall-clock time is
not fully reliable: benchmarks run on a shared work machine. Elapsed CPU cycles, process
user and system time, preparation wall time, maximum resident set size, peak memory
footprint, page faults, and context switches are supporting evidence.

The baseline is the current commit of the hdiff repository. The candidate is that
repository's dirty tracked working tree. The benchmark records the baseline commit and a
SHA-256 digest of the candidate build-input diff; Kubernetes revisions identify only the
input corpus. Build inputs are Cargo manifests, toolchain and Cargo configuration, build
scripts, and Rust source. The candidate may contain staged or unstaged tracked changes.
The benchmark rejects untracked build inputs so the digest covers every file used by the
candidate build.

The runner also supports `--mode measure` and `--mode profile` (`--bench stages`) for a
clean or dirty current checkout. Comparison is the default mode. Measurement repeats the
current binary three times after two warmups and does not produce an improvement verdict. Profiles run
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
the baseline measurement minus the candidate measurement. The mean retired-instruction
saving must be positive and greater than the maximum absolute deviation among the three
paired savings. Mean cycle and peak-memory savings must not be negative by more than their
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

Each optimization slice reports its measured retired-instruction saving first, then its
cycle and preparation-time savings, from the benchmark comparison. Savings are derived from the authoritative measurements;
they are not stored as a second fact. A candidate that is not accepted does not advance
the optimization record.
