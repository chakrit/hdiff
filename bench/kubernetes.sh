#!/bin/sh
set -eu

readonly kubernetes_commit=70d3cc986aa8221cd1dfb1121852688902d3bf53
readonly kubernetes_remote=https://github.com/kubernetes/kubernetes.git

repo_root=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
benchmark_root="$repo_root/.ace/bench/kubernetes"
checkout="$benchmark_root/checkout"
history="$benchmark_root/first-parent.txt"
results="$benchmark_root/results.txt"
binary="$repo_root/target/release/hdiff"

if [ ! -d "$checkout/.git" ]; then
    mkdir -p "$benchmark_root"
    nice -n 19 git clone "$kubernetes_remote" "$checkout"
fi

git -C "$checkout" cat-file -e "$kubernetes_commit^{commit}"
nice -n 19 cargo build --release --manifest-path "$repo_root/Cargo.toml"

git -C "$checkout" rev-list --first-parent --reverse "$kubernetes_commit" > "$history"
edge_count=$(($(wc -l < "$history") - 1))

for numerator in 7 6 4; do
    base_index=$((edge_count * numerator / 8))
    base_commit=$(sed -n "$((base_index + 1))p" "$history")
    patch="$benchmark_root/$numerator-8.patch"

    git -C "$checkout" diff "$base_commit" "$kubernetes_commit" -- '*.go' > "$patch"
    result=$("$binary" --bench < "$patch")
    printf 'range=%s/8 base=%s target=%s %s\n' \
        "$numerator" "$base_commit" "$kubernetes_commit" "$result" | tee -a "$results"
done
