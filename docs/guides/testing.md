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

Integration tests that start the `hdiff` executable or Git build them through
`TestEnv` in `tests/common/mod.rs`: each test gets its own temporary directory, and the
process runs there with only `HOME`, `PATH`, and `TERM` set. Name repository fixtures with
`common::fixture`, which works from any sandbox.

## Live interaction tests

The `live` target in `tests/live/` drives the interactive loop in-process, without
starting the `hdiff` executable or a terminal. Each test parses a fixture patch, loads it
into a `Tui`, and sends one event at a time with `tui.step` to a 120-by-36
`Terminal<TestBackend>`. After each step it asserts on the backend buffer: visible text,
cell positions, or whole-buffer equality with an earlier frame.

Run only this target, or one scenario across all modes:

```sh
cargo test --locked --test live
cargo test --locked --test live file_rotation
```

`harness.rs` holds the `Live` harness and `Mode`, `fixtures.rs` the patches several
scenarios share, and one module per concern holds its scenarios: `navigation.rs`,
`file_list.rs`, `rendering.rs`, and `wrapping.rs`.

The `matrix!` macro in `main.rs` runs each scenario function once per layout (`unified`,
`vertical`, `stacked`) and detail mode (`line`, `character`), as test modules named after
the scenario, such as `navigation::file_rotation::stacked_character`. To add a scenario,
write a `fn name(mode: Mode)` that starts with `Live::load(patch, mode)` in the module for
its concern, then add `name` to that module's `matrix!` list. Write a plain `#[test]` for
behavior that belongs to one mode only.

Assert what the reader sees rather than how the frame is built. Prefer text on screen,
relative column positions, and "this key returns to the earlier frame". Fixture files live
in `tests/fixtures/`; build small patches inline in the test module.

## Timing

Timing is measured with `hdiff --bench <driver>`, not with tests. Run
`bench/fixtures.sh` to build the release binary and run every driver over the repository
fixtures and the generated 1,024-hunk movement workload. Raw output and a summary go to
`.ace/bench/fixtures/`. The driver contracts and targets are in
[`docs/spec/performance.md`](../spec/performance.md). For preparation comparisons, see
the [Kubernetes benchmark guide](kubernetes-benchmark.md).
