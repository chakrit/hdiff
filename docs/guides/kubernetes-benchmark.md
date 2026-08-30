# Kubernetes preparation benchmark

Run the reproducible Kubernetes preparation benchmark from the repository root:

```sh
bench/kubernetes.sh
```

The harness pins Kubernetes v1.32.0, builds `hdiff` once at low priority, and measures
Go-only changes from the final eighth, quarter, and half of the pinned first-parent
history. Each range uses two discarded warm-up invocations, followed by three measured
invocations whose preparation durations are averaged. It keeps the checkout, generated
patches, first-parent history, and appended result lines in
`.ace/bench/kubernetes/`, which is intentionally untracked.

Copy the three newly appended lines from `.ace/bench/kubernetes/results.txt` into the
benchmark record in `docs/spec/performance.md`. Each line records its range, resolved
base, pinned target, measured hdiff source revision, and averaged `hdiff --bench` result.
