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
source_revision=$(git -C "$repo_root" describe --always --dirty)

for numerator in 7 6 4; do
    base_index=$((edge_count * numerator / 8))
    base_commit=$(sed -n "$((base_index + 1))p" "$history")
    patch="$benchmark_root/$numerator-8.patch"

    git -C "$checkout" diff "$base_commit" "$kubernetes_commit" -- '*.go' > "$patch"
    for _ in 1 2; do
        nice -n 19 "$binary" --bench < "$patch" > /dev/null
    done

    total_duration=0
    sample_count=0
    measured_files=
    measured_records=
    for _ in 1 2 3; do
        sample=$(nice -n 19 "$binary" --bench < "$patch")
        sample_label=
        duration_field=
        files_field=
        records_field=
        extra_field=
        IFS=' ' read -r \
            sample_label duration_field files_field records_field extra_field <<EOF
$sample
EOF

        duration=${duration_field#duration_ns=}
        file_count=${files_field#files=}
        record_count=${records_field#records=}
        canonical_sample="preparation duration_ns=$duration files=$file_count records=$record_count"
        invalid_number=false
        case "$duration" in ''|*[!0-9]*) invalid_number=true ;; esac
        case "$file_count" in ''|*[!0-9]*) invalid_number=true ;; esac
        case "$record_count" in ''|*[!0-9]*) invalid_number=true ;; esac
        if [ "$sample_label" != preparation ] || \
            [ -n "$extra_field" ] || \
            [ "$sample" != "$canonical_sample" ] || \
            [ "$invalid_number" = true ]; then
            printf 'invalid benchmark result: %s\n' "$sample" >&2
            exit 1
        fi

        if [ "$sample_count" -eq 0 ]; then
            measured_files=$file_count
            measured_records=$record_count
        elif [ "$file_count" -ne "$measured_files" ] || \
            [ "$record_count" -ne "$measured_records" ]; then
            printf 'inconsistent benchmark result: %s\n' "$sample" >&2
            exit 1
        fi

        total_duration=$((total_duration + duration))
        sample_count=$((sample_count + 1))
    done

    average_duration=$((total_duration / sample_count))
    printf 'range=%s/8 base=%s target=%s source=%s preparation duration_ns=%s files=%s records=%s\n' \
        "$numerator" "$base_commit" "$kubernetes_commit" "$source_revision" \
        "$average_duration" "$measured_files" "$measured_records" \
        | tee -a "$results"
done
