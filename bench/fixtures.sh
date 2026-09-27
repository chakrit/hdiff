#!/bin/sh
# Runs every --bench driver over the repository fixtures and the generated movement
# workload. Contract: docs/spec/performance.md#measurement-scripts.
set -eu

readonly movement_hunks=1024
readonly drivers='prepare stages movement'

fail() {
    printf '%s\n' "$1" >&2
    exit 1
}

# Three Rust files of seven-line hunks: six context records around a long replacement.
movement_workload() {
    long_value=
    repeat=0
    while [ "$repeat" -lt 18 ]; do
        long_value=${long_value}abcdefghij
        repeat=$((repeat + 1))
    done

    for file in 0 1 2; do
        printf -- '--- a/file-%s.rs\n+++ b/file-%s.rs\n' "$file" "$file"
        hunk=0
        while [ "$hunk" -lt "$movement_hunks" ]; do
            line=$((hunk * 7 + 1))
            printf '@@ -%s,7 +%s,7 @@\n' "$line" "$line"
            printf ' fn value_%s() -> &'"'"'static str {\n' "$hunk"
            printf ' // A context row before the changed string.\n'
            printf ' // Another context row before the changed string.\n'
            printf -- '-    "before_%s"\n' "$long_value"
            printf '+    "after_%s"\n' "$long_value"
            printf ' // A context row after the changed string.\n'
            printf ' // Another context row after the changed string.\n'
            printf ' }\n'
            hunk=$((hunk + 1))
        done
    done
}

worst_step_ns() {
    sed -n 's/.* step_ns=\([0-9]*\)$/\1/p' "$1" | sort -n | tail -n 1
}

preparation_ns() {
    sed -n 's/^preparation duration_ns=\([0-9]*\) .*/\1/p' "$1"
}

repo_root=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
run_id=$(date -u '+%Y%m%dT%H%M%SZ')-$(git -C "$repo_root" rev-parse --short HEAD)-$$
run_root="$repo_root/.ace/bench/fixtures/$run_id"
binary="$repo_root/target/release/hdiff"
summary="$run_root/summary.txt"

mkdir -p "$run_root"
cargo build --release --locked --manifest-path "$repo_root/Cargo.toml"
[ -x "$binary" ] || fail "missing release binary: $binary"
movement_workload > "$run_root/movement-$movement_hunks.patch"

printf 'run_id=%s binary=%s\n' "$run_id" "$binary" > "$summary"
for fixture in "$repo_root"/tests/fixtures/*.patch "$run_root/movement-$movement_hunks.patch"; do
    name=$(basename "$fixture" .patch)
    for driver in $drivers; do
        output="$run_root/$name.$driver.txt"
        "$binary" --bench "$driver" "$fixture" > "$output" ||
            fail "--bench $driver failed on $fixture"
    done
    printf 'fixture=%s preparation_ns=%s movement_worst_step_ns=%s\n' "$name" \
        "$(preparation_ns "$run_root/$name.prepare.txt")" \
        "$(worst_step_ns "$run_root/$name.movement.txt")" >> "$summary"
done

cat "$summary"
