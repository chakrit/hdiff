---
status: accepted
---

# Performance plan

## Preparation boundary

`hdiff` completes parsing, layout derivation, Tree-sitter syntax highlighting, and character
detail before entering the terminal loop. The loop receives fully prepared data and has no
parser, cache, readiness, loading, or first-file state.

Preparation is one optimized Rust pass. Incremental preparation is not part of this design unless
measured preparation latency makes the eager boundary untenable.

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

| Range | Base commit                              | Target commit                            | Duration (ns) | Files | Records |
|-------|------------------------------------------|------------------------------------------|---------------|-------|---------|
| 7/8   | `ee94dce5b179923e362356a62738fa1de06c62b6` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 292509661458  | 13344 | 3995123 |
| 6/8   | `5c6d853b4434f72ac10a1d9eafe15a791cd5db31` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 280323353042  | 18264 | 5224599 |
| 4/8   | `e111ccbe09aaa7f1854da1625eb8da1cf939210e` | `70d3cc986aa8221cd1dfb1121852688902d3bf53` | 257964687583  | 20017 | 6219966 |
