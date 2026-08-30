# Kubernetes preparation benchmark

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
first-parent history. Benchmark commands run at ordinary scheduler priority.

Add every new candidate build-input file to the Git index before running the benchmark.
The script rejects untracked Cargo manifests, toolchain and Cargo configuration, build
scripts, and Rust source so the candidate digest covers every file used by the build.

Each range warms both binaries twice, then runs three alternating baseline-candidate
pairs through macOS `/usr/bin/time -lp`. Cycles determine whether execution cost improved;
retired instructions control for completed work; CPU time, preparation wall time, memory,
page faults, and context switches explain the result. A saving that does not exceed the
paired dispersion, or that materially regresses instructions or peak memory, is reported
as inconclusive.

The script keeps the Kubernetes checkout, generated patches, baseline worktrees, candidate
source diffs, raw timing files, and append-only result lines in
`.ace/bench/kubernetes/`, which is intentionally untracked. Each result identifies the
hdiff baseline commit and the SHA-256 digest of the candidate build-input diff.

Copy each conclusive range summary from `.ace/bench/kubernetes/results.txt` into the
benchmark record in `docs/spec/performance.md`. Each summary records the corpus range,
both hdiff states, averaged counters, dispersion, verdict, and file and record counts.
Do not add inconclusive or regressed candidates to the authoritative record. The accepted
candidate is committed and becomes the next optimization slice's baseline.
