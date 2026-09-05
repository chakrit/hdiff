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
checkout. Comparison is the default mode. Measurement repeats the current binary after two
warmups; profiles run separately and preserve stage output and an OS stack sample. A stack
sample reports its sampling window and completion status and never claims whole-process
coverage when attachment or completion cuts that window short.

Every run preserves a manifest and raw files beneath a unique run directory, including
failed runs. Provenance includes UTC time, hardware and OS, Rust and Cargo versions, build
flags and commands, source revision and build-input diff, runner digest, binary digests,
and corpus digest and size. Both binaries use the same explicit installed toolchain and
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
separate verification record.

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

### Measurement foundation verification

The instrumentation comparison baseline is `579198f550fbbe613236b75f9301c2483c29da61`.
The preliminary candidate build-input SHA-256 is
`c32c2241899cb05d67048607b99c0c67de38248d6c88d0dbaeb2dc4b8ece949f`.
The machine reports `Macmini9,1`, eight CPUs, and 16 GiB memory; the compiler is
Rust 1.98.1 for `aarch64-apple-darwin`. These are instrumentation-cost measurements,
separate from the accepted optimization record.

The initial 7/8 comparison run is
`20260905T050955Z-579198f550fbbe613236b75f9301c2483c29da61-76472`.
Preparation averaged 23,432,574,402 ns at baseline and 23,426,776,749 ns with
instrumentation, a saving of 5,797,653 ns. The verdict is `regressed`: mean instructions
increased by 443,682,581 and peak memory by 31,124,138 bytes, exceeding their paired
dispersions. Cycles were inconclusive. This run is not an accepted optimization.

The final diagnostic run is
`20260905T054713Z-579198f550fbbe613236b75f9301c2483c29da61-37738`.
It uses the final candidate identified below, with the same captured binary SHA-256:
`f59ce87667e619cae3dbd7348a160fc5356d0ceea6cb0e277c06efae43b57699`.
It covers the 4/8 corpus: 233,280,860 input bytes, 20,017 files, and 6,219,966 records.
The full stage report is:

```text
diagnostic duration_ns=30857921917 files=20017 records=6219966
startup version=1 input_ns=48452500 parse_ns=3076894666 preparation_ns=30857921917 ready_ns=33983269083 bytes=233280860
profile version=1
stage name=labels parent=preparation duration_ns=5589209
stage name=layout parent=preparation duration_ns=383737989
stage name=syntax parent=preparation duration_ns=29574247519
stage name=detail parent=preparation duration_ns=891928703
stage name=unattributed parent=preparation duration_ns=2418497
stage name=language parent=syntax duration_ns=2873966
stage name=projection parent=syntax duration_ns=180039995
stage name=parse parent=syntax duration_ns=18294385760
stage name=query parent=syntax duration_ns=10058745198
stage name=unattributed parent=syntax duration_ns=1038202600
work hunks=47030 parse_calls=78485 captures=21221116 projected_bytes=228017900
```

The separate stack-sampled process completed successfully; sampling also returned zero
and preserved a nonempty stack file. Its requested window was ten seconds at one
millisecond intervals, covering only the beginning of the process. The capture contains
8,275 main-thread observations. Its most frequent leaf frames include `ts_parser_parse`,
hdiff's `sanitize`, and `ts_query_cursor__advance`; their sample proportions do not
represent whole-process stage proportions. Raw profiles, commands, binary snapshots,
manifests, and sampling coverage live under `.ace/bench/kubernetes/runs/<run-id>/`.

The final comparison run is
`20260905T053102Z-579198f550fbbe613236b75f9301c2483c29da61-18083`.
Its candidate build-input SHA-256 is
`72ce6b1902f57f6d666b19834c70194e5a48357a83c5c1ef23f71b22ef05e7e0`.
All three ranges completed with matching file and record counts, stable identities,
and successful process exits. Each verdict is `regressed` under the unchanged
acceptance rule, with instruction growth exceeding paired dispersion in every range.
These records quantify the instrumentation's overhead:

```text
summary run_id=20260905T053102Z-579198f550fbbe613236b75f9301c2483c29da61-18083 range=7/8 corpus_base=ee94dce5b179923e362356a62738fa1de06c62b6 corpus_target=70d3cc986aa8221cd1dfb1121852688902d3bf53 baseline=579198f550fbbe613236b75f9301c2483c29da61 candidate=579198f550fbbe613236b75f9301c2483c29da61-dirty-72ce6b1902f57f6d666b19834c70194e5a48357a83c5c1ef23f71b22ef05e7e0 verdict=regressed baseline_cycles=80514650610 candidate_cycles=80558319567 cycle_saving=-43668956 cycle_dispersion=361056416 baseline_instructions=308633031055 candidate_instructions=309163155048 instruction_saving=-530123993 instruction_dispersion=199622138 baseline_peak_memory=2480969962 candidate_peak_memory=2497905557 memory_saving=-16935594 memory_dispersion=27923798 baseline_maximum_rss=2598005418 candidate_maximum_rss=2609326762 baseline_duration_ns=23574814305 candidate_duration_ns=23573314986 preparation_saving_ns=1499319 ready_saving_ns=unavailable baseline_schema=legacy candidate_schema=startup-v1 files=13344 records=3995123
summary run_id=20260905T053102Z-579198f550fbbe613236b75f9301c2483c29da61-18083 range=6/8 corpus_base=5c6d853b4434f72ac10a1d9eafe15a791cd5db31 corpus_target=70d3cc986aa8221cd1dfb1121852688902d3bf53 baseline=579198f550fbbe613236b75f9301c2483c29da61 candidate=579198f550fbbe613236b75f9301c2483c29da61-dirty-72ce6b1902f57f6d666b19834c70194e5a48357a83c5c1ef23f71b22ef05e7e0 verdict=regressed baseline_cycles=92918936850 candidate_cycles=93124260197 cycle_saving=-205323347 cycle_dispersion=191436817 baseline_instructions=355949484604 candidate_instructions=356746233667 instruction_saving=-796749063 instruction_dispersion=46529196 baseline_peak_memory=3154888896 candidate_peak_memory=3157160853 memory_saving=-2271957 memory_dispersion=469717 baseline_maximum_rss=3272152405 candidate_maximum_rss=3273457664 baseline_duration_ns=26869184667 candidate_duration_ns=26939908986 preparation_saving_ns=-70724319 ready_saving_ns=unavailable baseline_schema=legacy candidate_schema=startup-v1 files=18264 records=5224599
summary run_id=20260905T053102Z-579198f550fbbe613236b75f9301c2483c29da61-18083 range=4/8 corpus_base=e111ccbe09aaa7f1854da1625eb8da1cf939210e corpus_target=70d3cc986aa8221cd1dfb1121852688902d3bf53 baseline=579198f550fbbe613236b75f9301c2483c29da61 candidate=579198f550fbbe613236b75f9301c2483c29da61-dirty-72ce6b1902f57f6d666b19834c70194e5a48357a83c5c1ef23f71b22ef05e7e0 verdict=regressed baseline_cycles=107525893519 candidate_cycles=107365611386 cycle_saving=160282133 cycle_dispersion=681745847 baseline_instructions=413148697806 candidate_instructions=413778072470 instruction_saving=-629374664 instruction_dispersion=601780331 baseline_peak_memory=3760398954 candidate_peak_memory=3751305834 memory_saving=9093120 memory_dispersion=16465920 baseline_maximum_rss=3719938048 candidate_maximum_rss=3769772714 baseline_duration_ns=30924767208 candidate_duration_ns=31036053014 preparation_saving_ns=-111285806 ready_saving_ns=unavailable baseline_schema=legacy candidate_schema=startup-v1 files=20017 records=6219966
```

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
