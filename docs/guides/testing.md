# Testing hdiff

Run every check from the repository root:

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
sh tests/benchmark-runner.sh
```

`cargo test` runs the unit tests, the CLI and benchmark-report integration tests, and the
`live` interaction tests. None of them assert timing.

## Live interaction tests

`tests/live.rs` drives the interactive loop in-process, without starting the `hdiff`
executable or a terminal. Each test parses a fixture patch, loads it into a `Tui`, and
sends one event at a time with `tui.step` to a 120-by-36 `Terminal<TestBackend>`. After
each step it asserts on the backend buffer: visible text, cell positions, or whole-buffer
equality with an earlier frame.

Run only this target, or one scenario across all modes:

```sh
cargo test --locked --test live
cargo test --locked --test live file_rotation
```

The `matrix!` macro runs each scenario function once per layout (`unified`, `vertical`,
`stacked`) and detail mode (`line`, `character`), as test modules named after the
scenario, such as `file_rotation::stacked_character`. To add a scenario, write a
`fn name(mode: Mode)` that starts with `Live::load(patch, mode)`, then add `name` to the
`matrix!` list. Write a plain `#[test]` for behavior that belongs to one mode only.

Assert what the reader sees rather than how the frame is built. Prefer text on screen,
relative column positions, and "this key returns to the earlier frame". Patches shared
across tests live in `tests/fixtures/`; build small patches inline in the test file.

## Timing

Timing is measured with `hdiff --bench <driver>`, not with tests. Run
`bench/fixtures.sh` to build the release binary and run every driver over the repository
fixtures and the generated 1,024-hunk movement workload. Raw output and a summary go to
`.ace/bench/fixtures/`. The driver contracts and targets are in
[`docs/spec/performance.md`](../spec/performance.md). For preparation comparisons, see
the [Kubernetes benchmark guide](kubernetes-benchmark.md).
