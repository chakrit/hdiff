---
status: accepted
---

# Performance plan

## Preparation boundary

`hdiff` completes parsing, layout derivation, Tree-sitter syntax highlighting, and character
detail before entering the terminal loop. The loop receives fully prepared data and has no
parser, cache, readiness, loading, or first-file state.

Preparation is one optimized single-threaded pass. Incremental preparation is not part of this
design unless measured preparation latency makes the eager boundary untenable.

Document-wide file-list labels are derived once during preparation. Each prepared file derives
only its own rendered rows, split pairs, syntax spans, and character-detail spans.

## Benchmark mode

`hdiff --bench` accepts the normal diff input, completes the normal preparation pass, prints one
machine-readable result line, and exits without opening the terminal. The result reports total
preparation time and the prepared file and record counts as
`preparation duration_ns=<integer> files=<integer> records=<integer>`.
Benchmark mode starts timing immediately before the shared preparation pass and exits immediately
after reporting its result.

## Representative corpus

Performance measurements use a local checkout of `kubernetes/kubernetes` at a pinned commit. The
benchmark streams only Go changes from three historical ranges ending at that commit.

The reproducible corpus is Kubernetes v1.32.0,
`70d3cc986aa8221cd1dfb1121852688902d3bf53`. `bench/kubernetes.sh` owns its checkout below
`.ace/bench/kubernetes/`, builds the release binary once at low priority, and appends each run's
three resolved measurements to `.ace/bench/kubernetes/results.txt`.

Let `D` be the number of first-parent edges from the repository root to the pinned commit. The
three base commits are at first-parent indices `floor(D * 7 / 8)`, `floor(D * 3 / 4)`, and
`floor(D / 2)` from the root. Diffing each base against the pinned commit gives the final eighth,
quarter, and half of the repository history respectively.

The resolved commit pairs and their results are recorded with each benchmark run. The benchmark
measures the complete preparation pass; it does not model an interactive terminal session.

## Benchmark record

### Optimization investigation ledger

The benchmark table is the measurement record. This ledger records the boundaries that
were investigated, so a later optimization slice begins from the remaining work rather
than revisiting a completed or ruled-out attempt.

### Completed

- **Prepared-file layout.** The 4/8 measurement fell from 257964687583 ns to
  139103250625 ns. Each prepared file derives its rows without rebuilding the
  document-wide file list.
- **Syntax-span projection.** The 4/8 measurement fell from 139103250625 ns to
  33339918416 ns. Span mapping begins at the first record overlapping each source event.
- **Sanitized source reuse.** The 4/8 measurement fell from 33339918416 ns to
  32797317959 ns. Rendering reuses parser-sanitized source text.
- **Direct syntax-query pass.** The 4/8 measurement fell from 32797317959 ns to
  32347072792 ns. A purpose-built public Tree-sitter query pass incrementally reparses each
  same-language old/new hunk pair and maps captures directly to hdiff syntax classes.
- **Line-detail omission.** The 4/8 measurement fell from 32347072792 ns to
  32288018750 ns. Line granularity carries no character detail and does not retain a
  row-aligned empty table.

## Optimization method

The one-second Kubernetes preparation target is reached through successive proper
refactors, never by fast-forwarding to a number. Each slice starts by understanding the
actual data flow and cost, then chooses the most elegant structural change that makes the
fast path the natural implementation.

An optimization must improve the code as well as its runtime: clear ownership, idiomatic
Rust, direct data flow, and no accidental allocation or repeated derivation. Prepared and
rendered output remain correct because the design makes them correct, not because an opaque
shortcut happens to preserve a test case.

Do not trade readability, sound boundaries, or maintainability for a speculative
micro-optimization. When no clean and measured improvement exists, leave the code alone;
the target is the accumulated result of good engineering, not a reason to force a change.

Each optimization slice reports its measured preparation-time savings against the preceding
recorded benchmark.

| Slice                       | Range | Base commit                              | Target commit                            | Duration (ns) | Files | Records |
|-----------------------------|-------|------------------------------------------|------------------------------------------|---------------|-------|---------|
| Prepared-file layout        | 7/8   | `ee94dce5b179923e362356a62738fa1de06c62b6` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 292509661458  | 13344 | 3995123 |
| Prepared-file layout        | 6/8   | `5c6d853b4434f72ac10a1d9eafe15a791cd5db31` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 280323353042  | 18264 | 5224599 |
| Prepared-file layout        | 4/8   | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 257964687583  | 20017 | 6219966 |
| Syntax projection lookup    | 7/8   | `ee94dce5b179923e362356a62738fa1de06c62b6` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 26225344250   | 13344 | 3995123 |
| Syntax projection lookup    | 6/8   | `5c6d853b4434f72ac10a1d9eafe15a791cd5db31` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 29148720958   | 18264 | 5224599 |
| Syntax projection lookup    | 4/8   | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 33339918416   | 20017 | 6219966 |
| Reuse sanitized source text | 4/8   | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 32797317959   | 20017 | 6219966 |
| Direct syntax-query pass    | 4/8   | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 32347072792   | 20017 | 6219966 |
| Line-detail omission        | 4/8   | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 32288018750   | 20017 | 6219966 |
