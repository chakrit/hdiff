---
status: accepted
---

# Performance plan

## Preparation boundary

`hdiff` completes parsing, layout derivation, Tree-sitter syntax highlighting, and
character detail before entering the terminal loop. The loop receives fully prepared data
and has no parser, cache, readiness, loading, or first-file state.

Preparation is one optimized single-threaded pass. Incremental preparation is not part of
this design unless measured preparation latency makes the eager boundary untenable.

Document-wide file-list labels are derived once during preparation. Each prepared file
derives only its own rendered rows, split pairs, syntax spans, and character-detail spans.
The prepared layout is the sole owner of each exact-sized immutable rendered-row buffer.
Unified and side-by-side layout projections reference those buffers.

## Benchmark mode

`hdiff --bench` accepts the normal diff input, completes the normal preparation pass,
prints one machine-readable result line, and exits without opening the terminal. The
result reports total preparation time and the prepared file and record counts as
`preparation duration_ns=<integer> files=<integer> records=<integer>`.
Benchmark mode starts timing immediately before the shared preparation pass and exits
immediately after reporting its result.

## Measurement model

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

Each Kubernetes range warms the baseline and candidate twice, then measures three pairs
in baseline-candidate, candidate-baseline, baseline-candidate order. A pair's saving is
the baseline measurement minus the candidate measurement. The mean cycle saving must be
positive and greater than the maximum absolute deviation among the three paired savings.
Mean retired-instruction and peak-memory savings must not be negative by more than their
own maximum absolute deviations. A comparison that does not meet every condition is
inconclusive, never an improvement.

The benchmark appends every raw sample and its order before appending the derived range
summary. Existing raw measurements are never replaced. Baseline and candidate file and
record counts must agree for every pair. Raw evidence includes inconclusive and regressed
runs; only accepted candidates enter the authoritative performance record.

The one-second target is user-visible preparation latency. Quiet-machine wall-clock runs
confirm milestone progress and the final target; routine optimization acceptance does not
depend on precise wall-clock measurement.

## Representative corpus

Performance measurements use a local checkout of `kubernetes/kubernetes` at a pinned
commit. The benchmark streams only Go changes from three historical ranges ending at that
commit.

The reproducible corpus is Kubernetes v1.32.0,
`70d3cc986aa8221cd1dfb1121852688902d3bf53`. `bench/kubernetes.sh` owns its checkout below
`.ace/bench/kubernetes/`, builds the release binary once at ordinary scheduler priority,
builds the current hdiff commit in a cached linked worktree, builds the dirty candidate,
and appends raw samples and summaries to `.ace/bench/kubernetes/results.txt`. Benchmark
commands are never de-prioritized. Each result identifies both measured hdiff states.

Let `D` be the number of first-parent edges from the repository root to the pinned commit.
The three base commits are at first-parent indices `floor(D * 7 / 8)`,
`floor(D * 3 / 4)`, and `floor(D / 2)` from the root. Diffing each base against the pinned
commit gives the final eighth, quarter, and half of the repository history respectively.

The resolved commit pairs and their results are recorded with each benchmark run. The
benchmark measures the complete preparation pass; it does not model an interactive
terminal session.

## Benchmark record

### Optimization investigation ledger

The benchmark table is the measurement record. This ledger records the boundaries that
were investigated, so a later optimization slice begins from the remaining work rather
than revisiting a completed or ruled-out attempt.

### Completed

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

The one-second Kubernetes preparation target is reached through successive proper
refactors, never by fast-forwarding to a number. Each slice starts by understanding the
actual data flow and cost, then chooses the most elegant structural change that makes the
fast path the natural implementation.

An optimization must improve the code as well as its runtime: clear ownership, idiomatic
Rust, direct data flow, and no accidental allocation or repeated derivation. Prepared and
rendered output remain correct because the design makes them correct, not because an
opaque shortcut happens to preserve a test case.

Do not trade readability, sound boundaries, or maintainability for a speculative
micro-optimization. When no clean and measured improvement exists, leave the code alone;
the target is the accumulated result of good engineering, not a reason to force a change.

Each optimization slice reports its measured cycle saving against the preceding accepted
hdiff commit and its preparation-time saving against the preceding quiet-machine
wall-clock milestone. A candidate that is not accepted does not advance either record.
Savings are derived from the authoritative measurements; they are not stored as a second
fact.

The table below is the historical wall-clock record created before paired hardware-counter
comparisons became authoritative. It is not backfilled with hardware counters.

### Paired comparison record

```text
summary range=7/8 corpus_base=ee94dce5b179923e362356a62738fa1de06c62b6 corpus_target=70d3cc986aa8221cd1dfb1121852688902d3bf53 baseline=e5360f9d8944e9ddcb2fe15345e3040c72585183 candidate=e5360f9d8944e9ddcb2fe15345e3040c72585183-dirty-372c99738304c98f3e4362fe8162e93f66980690ee0aaa02e595a0ac78526695 verdict=improved baseline_cycles=86007809295 candidate_cycles=85355386615 cycle_saving=652422680 cycle_dispersion=47116820 baseline_instructions=333463492937 candidate_instructions=331084816078 instruction_saving=2378676859 instruction_dispersion=100326085 baseline_peak_memory=2878621440 candidate_peak_memory=2524824490 memory_saving=353796949 memory_dispersion=5717995 baseline_maximum_rss=2978556586 candidate_maximum_rss=2624323584 baseline_duration_ns=24591934056 candidate_duration_ns=24438332638 files=13344 records=3995123
summary range=6/8 corpus_base=5c6d853b4434f72ac10a1d9eafe15a791cd5db31 corpus_target=70d3cc986aa8221cd1dfb1121852688902d3bf53 baseline=e5360f9d8944e9ddcb2fe15345e3040c72585183 candidate=e5360f9d8944e9ddcb2fe15345e3040c72585183-dirty-372c99738304c98f3e4362fe8162e93f66980690ee0aaa02e595a0ac78526695 verdict=improved baseline_cycles=97511216634 candidate_cycles=96564440637 cycle_saving=946775997 cycle_dispersion=44801580 baseline_instructions=376725859612 candidate_instructions=373428100008 instruction_saving=3297759604 instruction_dispersion=142314289 baseline_peak_memory=3653662762 candidate_peak_memory=3200665920 memory_saving=452996842 memory_dispersion=16034538 baseline_maximum_rss=3750641664 candidate_maximum_rss=3288656554 baseline_duration_ns=27478403347 candidate_duration_ns=27429889819 files=18264 records=5224599
summary range=4/8 corpus_base=e111ccbe09aaa7f1854da1625eb8da1cf939210e corpus_target=70d3cc986aa8221cd1dfb1121852688902d3bf53 baseline=e5360f9d8944e9ddcb2fe15345e3040c72585183 candidate=e5360f9d8944e9ddcb2fe15345e3040c72585183-dirty-372c99738304c98f3e4362fe8162e93f66980690ee0aaa02e595a0ac78526695 verdict=improved baseline_cycles=110939124085 candidate_cycles=109933454958 cycle_saving=1005669127 cycle_dispersion=97638903 baseline_instructions=431989873062 candidate_instructions=428408688074 instruction_saving=3581184988 instruction_dispersion=127394246 baseline_peak_memory=4275786240 candidate_peak_memory=3736336384 memory_saving=539449856 memory_dispersion=6072960 baseline_maximum_rss=4343234560 candidate_maximum_rss=3814315349 baseline_duration_ns=31277371069 candidate_duration_ns=31095025124 files=20017 records=6219966
summary range=7/8 corpus_base=ee94dce5b179923e362356a62738fa1de06c62b6 corpus_target=70d3cc986aa8221cd1dfb1121852688902d3bf53 baseline=e3ba805832ecf480047809e36d5e55d9a2dbf681 candidate=e3ba805832ecf480047809e36d5e55d9a2dbf681-dirty-34d864def0867bcb103c9e2d646a31f8a3b43aaadd2c2446ccc729174b602439 verdict=improved baseline_cycles=86383253147 candidate_cycles=79549332067 cycle_saving=6833921080 cycle_dispersion=1232830421 baseline_instructions=327192465807 candidate_instructions=302504908847 instruction_saving=24687556960 instruction_dispersion=62453862 baseline_peak_memory=2361802154 candidate_peak_memory=2312300416 memory_saving=49501738 memory_dispersion=33926038 baseline_maximum_rss=2776197802 candidate_maximum_rss=2731928234 baseline_duration_ns=24815259361 candidate_duration_ns=22658687028 files=13344 records=3995123
summary range=6/8 corpus_base=5c6d853b4434f72ac10a1d9eafe15a791cd5db31 corpus_target=70d3cc986aa8221cd1dfb1121852688902d3bf53 baseline=e3ba805832ecf480047809e36d5e55d9a2dbf681 candidate=e3ba805832ecf480047809e36d5e55d9a2dbf681-dirty-34d864def0867bcb103c9e2d646a31f8a3b43aaadd2c2446ccc729174b602439 verdict=improved baseline_cycles=98042449081 candidate_cycles=92689372337 cycle_saving=5353076744 cycle_dispersion=485548962 baseline_instructions=368704677758 candidate_instructions=348832111263 instruction_saving=19872566495 instruction_dispersion=67728365 baseline_peak_memory=3019956181 candidate_peak_memory=2995462208 memory_saving=24493973 memory_dispersion=82083925 baseline_maximum_rss=3439001600 candidate_maximum_rss=3458891776 baseline_duration_ns=27812211444 candidate_duration_ns=26130750819 files=18264 records=5224599
summary range=4/8 corpus_base=e111ccbe09aaa7f1854da1625eb8da1cf939210e corpus_target=70d3cc986aa8221cd1dfb1121852688902d3bf53 baseline=e3ba805832ecf480047809e36d5e55d9a2dbf681 candidate=e3ba805832ecf480047809e36d5e55d9a2dbf681-dirty-34d864def0867bcb103c9e2d646a31f8a3b43aaadd2c2446ccc729174b602439 verdict=improved baseline_cycles=111640545207 candidate_cycles=106748608803 cycle_saving=4891936403 cycle_dispersion=333720819 baseline_instructions=422386671229 candidate_instructions=404252270247 instruction_saving=18134400982 instruction_dispersion=49849679 baseline_peak_memory=3455566101 candidate_peak_memory=3522855296 memory_saving=-67289194 memory_dispersion=68157654 baseline_maximum_rss=3978668714 candidate_maximum_rss=4027624106 baseline_duration_ns=31730092916 candidate_duration_ns=30150916041 files=20017 records=6219966
```

### Historical wall-clock record

| Slice                       | Range | Source                                     | Base commit                                | Target commit                              | Duration (ns) | Files | Records |
|-----------------------------|-------|--------------------------------------------|--------------------------------------------|--------------------------------------------|---------------|-------|---------|
| Pre-optimization            | 7/8   | `4cd17797c05585c1714b25e462d2063c09fdea79` | `ee94dce5b179923e362356a62738fa1de06c62b6` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 292509661458  | 13344 | 3995123 |
| Pre-optimization            | 6/8   | `4cd17797c05585c1714b25e462d2063c09fdea79` | `5c6d853b4434f72ac10a1d9eafe15a791cd5db31` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 280323353042  | 18264 | 5224599 |
| Pre-optimization            | 4/8   | `4cd17797c05585c1714b25e462d2063c09fdea79` | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 257964687583  | 20017 | 6219966 |
| Prepared-file layout        | 4/8   | `5cd170c8af2d02759970034ae1547763bc2faee1` | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 139103250625  | 20017 | 6219966 |
| Syntax projection lookup    | 7/8   | `010ffc6f4a67c45b97a854b5b8cf84868c743a08` | `ee94dce5b179923e362356a62738fa1de06c62b6` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 26225344250   | 13344 | 3995123 |
| Syntax projection lookup    | 6/8   | `010ffc6f4a67c45b97a854b5b8cf84868c743a08` | `5c6d853b4434f72ac10a1d9eafe15a791cd5db31` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 29148720958   | 18264 | 5224599 |
| Syntax projection lookup    | 4/8   | `010ffc6f4a67c45b97a854b5b8cf84868c743a08` | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 33339918416   | 20017 | 6219966 |
| Reuse sanitized source text | 4/8   | `efbf1573dbe41cf036325027dfce1176793afe3e` | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 32797317959   | 20017 | 6219966 |
| Direct syntax-query pass    | 4/8   | `4dc0a90bc79897d9121ad2d92805b6298abb1a59` | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 32347072792   | 20017 | 6219966 |
| Line-detail omission        | 4/8   | `bdb33cd4b2abe7d4c7413ae370f6ca97220c5ae0` | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 32288018750   | 20017 | 6219966 |
| Per-file syntax projection  | 4/8   | `bdb33cd-dirty`                            | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 35333977917   | 20017 | 6219966 |
| Harness verification        | 7/8   | `9fce2ec-dirty`                            | `ee94dce5b179923e362356a62738fa1de06c62b6` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 25065650333   | 13344 | 3995123 |
| Harness verification        | 6/8   | `9fce2ec-dirty`                            | `5c6d853b4434f72ac10a1d9eafe15a791cd5db31` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 28876855527   | 18264 | 5224599 |
| Harness verification        | 4/8   | `9fce2ec-dirty`                            | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 31994288028   | 20017 | 6219966 |
| Shared rendered buffers     | 7/8   | `6429c40-dirty`                            | `ee94dce5b179923e362356a62738fa1de06c62b6` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 24566397611   | 13344 | 3995123 |
| Shared rendered buffers     | 6/8   | `6429c40-dirty`                            | `5c6d853b4434f72ac10a1d9eafe15a791cd5db31` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 27410366694   | 18264 | 5224599 |
| Shared rendered buffers     | 4/8   | `6429c40-dirty`                            | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 31196953222   | 20017 | 6219966 |
