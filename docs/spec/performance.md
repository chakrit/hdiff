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
preparation time and the prepared file and record counts.

## Representative corpus

Performance measurements use a local checkout of `kubernetes/kubernetes` at a pinned commit. The
benchmark streams only Go changes from three historical ranges ending at that commit.

Let `D` be the number of first-parent edges from the repository root to the pinned commit. The
three base commits are at first-parent indices `floor(D * 7 / 8)`, `floor(D * 3 / 4)`, and
`floor(D / 2)` from the root. Diffing each base against the pinned commit gives the final eighth,
quarter, and half of the repository history respectively.

The resolved commit pairs and their results are recorded with each benchmark run. The benchmark
measures the complete preparation pass; it does not model an interactive terminal session.
