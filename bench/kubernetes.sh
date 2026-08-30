#!/bin/sh
set -eu

readonly kubernetes_commit=70d3cc986aa8221cd1dfb1121852688902d3bf53
readonly kubernetes_remote=https://github.com/kubernetes/kubernetes.git
readonly digest_command=/usr/bin/shasum
readonly time_command=/usr/bin/time

usage() {
    printf 'usage: %s [--range 7/8|6/8|4/8]\n' "$0" >&2
}

fail() {
    printf '%s\n' "$1" >&2
    exit 1
}

require_unsigned() {
    validation_name=$1
    validation_value=$2
    case "$validation_value" in
        ''|*[!0-9]*) fail "invalid $validation_name: $validation_value" ;;
    esac
}

require_decimal() {
    validation_name=$1
    validation_value=$2
    case "$validation_value" in
        ''|*[!0-9.]*|.*|*.|*.*.*) fail "invalid $validation_name: $validation_value" ;;
    esac
}

require_sha256() {
    validation_name=$1
    validation_value=$2
    if [ "${#validation_value}" -ne 64 ]; then
        fail "invalid $validation_name: $validation_value"
    fi
    case "$validation_value" in
        *[!0-9a-f]*) fail "invalid $validation_name: $validation_value" ;;
    esac
}

max_absolute_deviation() {
    deviation_mean=$1
    shift
    maximum_deviation=0

    for deviation_value do
        deviation=$((deviation_value - deviation_mean))
        if [ "$deviation" -lt 0 ]; then
            deviation=$((-deviation))
        fi
        if [ "$deviation" -gt "$maximum_deviation" ]; then
            maximum_deviation=$deviation
        fi
    done

    printf '%s\n' "$maximum_deviation"
}

measure() {
    measured_binary=$1
    measured_revision=$2
    measured_role=$3
    measured_range=$4
    measured_pair=$5
    measured_order=$6
    measured_patch=$7
    sample_name="$measured_range-8-$measured_pair-$measured_order-$measured_role"
    benchmark_output="$run_root/$sample_name.benchmark.txt"
    time_output="$run_root/$sample_name.time.txt"

    LC_ALL=C "$time_command" -lp "$measured_binary" --bench \
        < "$measured_patch" > "$benchmark_output" 2> "$time_output"

    benchmark_sample=$(sed -n '1,$p' "$benchmark_output")
    benchmark_label=
    duration_field=
    files_field=
    records_field=
    benchmark_extra=
    IFS=' ' read -r \
        benchmark_label duration_field files_field records_field benchmark_extra <<EOF
$benchmark_sample
EOF

    duration=${duration_field#duration_ns=}
    file_count=${files_field#files=}
    record_count=${records_field#records=}
    canonical_sample="preparation duration_ns=$duration"
    canonical_sample="$canonical_sample files=$file_count records=$record_count"
    require_unsigned duration_ns "$duration"
    require_unsigned files "$file_count"
    require_unsigned records "$record_count"
    if [ "$benchmark_label" != preparation ] || \
        [ -n "$benchmark_extra" ] || \
        [ "$benchmark_sample" != "$canonical_sample" ]; then
        fail "invalid benchmark result: $benchmark_sample"
    fi

    real_seconds=
    user_seconds=
    system_seconds=
    maximum_rss=
    page_faults=
    voluntary_switches=
    involuntary_switches=
    instructions=
    cycles=
    peak_memory=
    while IFS= read -r timing_line; do
        case "$timing_line" in
            'real '*) real_seconds=${timing_line#real } ;;
            'user '*) user_seconds=${timing_line#user } ;;
            'sys '*) system_seconds=${timing_line#sys } ;;
            *' maximum resident set size')
                IFS=' ' read -r maximum_rss _ <<EOF
$timing_line
EOF
                ;;
            *' page faults')
                IFS=' ' read -r page_faults _ <<EOF
$timing_line
EOF
                ;;
            *' voluntary context switches')
                IFS=' ' read -r voluntary_switches _ <<EOF
$timing_line
EOF
                ;;
            *' involuntary context switches')
                IFS=' ' read -r involuntary_switches _ <<EOF
$timing_line
EOF
                ;;
            *' instructions retired')
                IFS=' ' read -r instructions _ <<EOF
$timing_line
EOF
                ;;
            *' cycles elapsed')
                IFS=' ' read -r cycles _ <<EOF
$timing_line
EOF
                ;;
            *' peak memory footprint')
                IFS=' ' read -r peak_memory _ <<EOF
$timing_line
EOF
                ;;
        esac
    done < "$time_output"

    require_unsigned maximum_rss "$maximum_rss"
    require_unsigned page_faults "$page_faults"
    require_unsigned voluntary_switches "$voluntary_switches"
    require_unsigned involuntary_switches "$involuntary_switches"
    require_unsigned instructions "$instructions"
    require_unsigned cycles "$cycles"
    require_unsigned peak_memory "$peak_memory"
    require_decimal real_seconds "$real_seconds"
    require_decimal user_seconds "$user_seconds"
    require_decimal system_seconds "$system_seconds"

    raw_sample="sample range=$measured_range/8 pair=$measured_pair"
    raw_sample="$raw_sample order=$measured_order role=$measured_role"
    raw_sample="$raw_sample revision=$measured_revision duration_ns=$duration"
    raw_sample="$raw_sample cycles=$cycles instructions=$instructions"
    raw_sample="$raw_sample maximum_rss=$maximum_rss peak_memory=$peak_memory"
    raw_sample="$raw_sample real_s=$real_seconds user_s=$user_seconds"
    raw_sample="$raw_sample system_s=$system_seconds page_faults=$page_faults"
    raw_sample="$raw_sample voluntary_switches=$voluntary_switches"
    raw_sample="$raw_sample involuntary_switches=$involuntary_switches"
    raw_sample="$raw_sample files=$file_count records=$record_count"
    printf '%s\n' "$raw_sample" >> "$results"
    printf 'measured range=%s/8 pair=%s role=%s\n' \
        "$measured_range" "$measured_pair" "$measured_role" >&2
    printf '%s %s %s %s %s %s %s\n' \
        "$duration" "$cycles" "$instructions" "$peak_memory" \
        "$maximum_rss" "$file_count" "$record_count"
}

range_numerators='7 6 4'
while [ "$#" -gt 0 ]; do
    case "$1" in
        --range)
            [ "$#" -ge 2 ] || {
                usage
                exit 1
            }
            case "$2" in
                7/8) range_numerators=7 ;;
                6/8) range_numerators=6 ;;
                4/8) range_numerators=4 ;;
                *)
                    usage
                    exit 1
                    ;;
            esac
            shift 2
            ;;
        *)
            usage
            exit 1
            ;;
    esac
done

repo_root=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
benchmark_root="$repo_root/.ace/bench/kubernetes"
checkout="$benchmark_root/checkout"
history="$benchmark_root/first-parent.txt"
results="$benchmark_root/results.txt"
candidate_binary="$repo_root/target/release/hdiff"

[ -x "$time_command" ] || fail "missing executable: $time_command"
[ -x "$digest_command" ] || fail "missing executable: $digest_command"

if [ ! -d "$checkout/.git" ]; then
    mkdir -p "$benchmark_root"
    git clone "$kubernetes_remote" "$checkout"
fi

git -C "$checkout" cat-file -e "$kubernetes_commit^{commit}"
baseline_commit=$(git -C "$repo_root" rev-parse --verify HEAD)
baseline_root="$benchmark_root/hdiff-baselines/$baseline_commit"
baseline_checkout="$baseline_root/source"
baseline_target="$baseline_root/target"
baseline_binary="$baseline_target/release/hdiff"

untracked_source_paths=$(git -C "$repo_root" ls-files --others --exclude-standard -- \
    Cargo.toml Cargo.lock build.rs rust-toolchain.toml rust-toolchain .cargo src)
if [ -n "$untracked_source_paths" ]; then
    fail 'hdiff candidate contains untracked build inputs; add them to the index before benchmarking'
fi
if git -C "$repo_root" diff --quiet "$baseline_commit" -- \
    Cargo.toml Cargo.lock build.rs rust-toolchain.toml rust-toolchain .cargo src; then
    fail 'hdiff candidate has no tracked build-input changes from its baseline commit'
fi

run_id=$(date -u '+%Y%m%dT%H%M%SZ')-$baseline_commit-$$
run_root="$benchmark_root/runs/$run_id"
candidate_diff="$run_root/hdiff-candidate-source.patch"
mkdir -p "$run_root"
git -C "$repo_root" diff --binary "$baseline_commit" -- \
    Cargo.toml Cargo.lock build.rs rust-toolchain.toml rust-toolchain .cargo src \
    > "$candidate_diff"
candidate_digest_line=$("$digest_command" -a 256 "$candidate_diff")
IFS=' ' read -r candidate_digest _ <<EOF
$candidate_digest_line
EOF
require_sha256 candidate_sha256 "$candidate_digest"
candidate_revision="$baseline_commit-dirty-$candidate_digest"

if [ ! -e "$baseline_checkout/.git" ]; then
    mkdir -p "$baseline_root"
    git -C "$repo_root" worktree add --detach "$baseline_checkout" "$baseline_commit"
fi

checked_out_baseline=$(git -C "$baseline_checkout" rev-parse HEAD)
if [ "$checked_out_baseline" != "$baseline_commit" ]; then
    fail "baseline worktree has unexpected revision: $checked_out_baseline"
fi

cargo build --release --manifest-path "$baseline_checkout/Cargo.toml" \
    --target-dir "$baseline_target"
cargo build --release --manifest-path "$repo_root/Cargo.toml"
git -C "$checkout" rev-list --first-parent --reverse "$kubernetes_commit" > "$history"

edge_count=$(($(wc -l < "$history") - 1))

for numerator in $range_numerators; do
    base_index=$((edge_count * numerator / 8))
    base_commit=$(sed -n "$((base_index + 1))p" "$history")
    patch="$benchmark_root/$numerator-8.patch"

    git -C "$checkout" diff "$base_commit" "$kubernetes_commit" -- '*.go' > "$patch"
    for _ in 1 2; do
        "$baseline_binary" --bench < "$patch" > /dev/null
        "$candidate_binary" --bench < "$patch" > /dev/null
    done

    baseline_cycle_sum=0
    candidate_cycle_sum=0
    baseline_instruction_sum=0
    candidate_instruction_sum=0
    baseline_memory_sum=0
    candidate_memory_sum=0
    baseline_rss_sum=0
    candidate_rss_sum=0
    baseline_duration_sum=0
    candidate_duration_sum=0
    cycle_saving_sum=0
    instruction_saving_sum=0
    memory_saving_sum=0
    measured_files=
    measured_records=

    for pair in 1 2 3; do
        if [ "$pair" -eq 2 ]; then
            pair_order=candidate-baseline
            candidate_sample=$(measure \
                "$candidate_binary" "$candidate_revision" candidate \
                "$numerator" "$pair" first "$patch")
            baseline_sample=$(measure \
                "$baseline_binary" "$baseline_commit" baseline \
                "$numerator" "$pair" second "$patch")
        else
            pair_order=baseline-candidate
            baseline_sample=$(measure \
                "$baseline_binary" "$baseline_commit" baseline \
                "$numerator" "$pair" first "$patch")
            candidate_sample=$(measure \
                "$candidate_binary" "$candidate_revision" candidate \
                "$numerator" "$pair" second "$patch")
        fi

        IFS=' ' read -r \
            baseline_duration baseline_cycles baseline_instructions \
            baseline_memory baseline_rss baseline_files baseline_records <<EOF
$baseline_sample
EOF
        IFS=' ' read -r \
            candidate_duration candidate_cycles candidate_instructions \
            candidate_memory candidate_rss candidate_files candidate_records <<EOF
$candidate_sample
EOF

        if [ "$baseline_files" -ne "$candidate_files" ] || \
            [ "$baseline_records" -ne "$candidate_records" ]; then
            fail "baseline and candidate counts differ in range $numerator/8 pair $pair"
        fi
        if [ -z "$measured_files" ]; then
            measured_files=$baseline_files
            measured_records=$baseline_records
        elif [ "$baseline_files" -ne "$measured_files" ] || \
            [ "$baseline_records" -ne "$measured_records" ]; then
            fail "sample counts changed in range $numerator/8 pair $pair"
        fi

        cycle_saving=$((baseline_cycles - candidate_cycles))
        instruction_saving=$((baseline_instructions - candidate_instructions))
        memory_saving=$((baseline_memory - candidate_memory))
        baseline_cycle_sum=$((baseline_cycle_sum + baseline_cycles))
        candidate_cycle_sum=$((candidate_cycle_sum + candidate_cycles))
        baseline_instruction_sum=$((baseline_instruction_sum + baseline_instructions))
        candidate_instruction_sum=$((candidate_instruction_sum + candidate_instructions))
        baseline_memory_sum=$((baseline_memory_sum + baseline_memory))
        candidate_memory_sum=$((candidate_memory_sum + candidate_memory))
        baseline_rss_sum=$((baseline_rss_sum + baseline_rss))
        candidate_rss_sum=$((candidate_rss_sum + candidate_rss))
        baseline_duration_sum=$((baseline_duration_sum + baseline_duration))
        candidate_duration_sum=$((candidate_duration_sum + candidate_duration))
        cycle_saving_sum=$((cycle_saving_sum + cycle_saving))
        instruction_saving_sum=$((instruction_saving_sum + instruction_saving))
        memory_saving_sum=$((memory_saving_sum + memory_saving))

        case "$pair" in
            1)
                cycle_saving_1=$cycle_saving
                instruction_saving_1=$instruction_saving
                memory_saving_1=$memory_saving
                ;;
            2)
                cycle_saving_2=$cycle_saving
                instruction_saving_2=$instruction_saving
                memory_saving_2=$memory_saving
                ;;
            3)
                cycle_saving_3=$cycle_saving
                instruction_saving_3=$instruction_saving
                memory_saving_3=$memory_saving
                ;;
        esac

        printf 'paired range=%s/8 pair=%s order=%s\n' \
            "$numerator" "$pair" "$pair_order" >&2
    done

    average_baseline_cycles=$((baseline_cycle_sum / 3))
    average_candidate_cycles=$((candidate_cycle_sum / 3))
    average_baseline_instructions=$((baseline_instruction_sum / 3))
    average_candidate_instructions=$((candidate_instruction_sum / 3))
    average_baseline_memory=$((baseline_memory_sum / 3))
    average_candidate_memory=$((candidate_memory_sum / 3))
    average_baseline_rss=$((baseline_rss_sum / 3))
    average_candidate_rss=$((candidate_rss_sum / 3))
    average_baseline_duration=$((baseline_duration_sum / 3))
    average_candidate_duration=$((candidate_duration_sum / 3))
    average_cycle_saving=$((cycle_saving_sum / 3))
    average_instruction_saving=$((instruction_saving_sum / 3))
    average_memory_saving=$((memory_saving_sum / 3))

    cycle_dispersion=$(max_absolute_deviation \
        "$average_cycle_saving" \
        "$cycle_saving_1" "$cycle_saving_2" "$cycle_saving_3")
    instruction_dispersion=$(max_absolute_deviation \
        "$average_instruction_saving" \
        "$instruction_saving_1" "$instruction_saving_2" "$instruction_saving_3")
    memory_dispersion=$(max_absolute_deviation \
        "$average_memory_saving" \
        "$memory_saving_1" "$memory_saving_2" "$memory_saving_3")

    verdict=inconclusive
    if [ "$average_cycle_saving" -gt "$cycle_dispersion" ] && \
        [ $((average_instruction_saving + instruction_dispersion)) -ge 0 ] && \
        [ $((average_memory_saving + memory_dispersion)) -ge 0 ]; then
        verdict=improved
    fi

    summary="summary range=$numerator/8 corpus_base=$base_commit"
    summary="$summary corpus_target=$kubernetes_commit"
    summary="$summary baseline=$baseline_commit candidate=$candidate_revision"
    summary="$summary verdict=$verdict"
    summary="$summary baseline_cycles=$average_baseline_cycles"
    summary="$summary candidate_cycles=$average_candidate_cycles"
    summary="$summary cycle_saving=$average_cycle_saving"
    summary="$summary cycle_dispersion=$cycle_dispersion"
    summary="$summary baseline_instructions=$average_baseline_instructions"
    summary="$summary candidate_instructions=$average_candidate_instructions"
    summary="$summary instruction_saving=$average_instruction_saving"
    summary="$summary instruction_dispersion=$instruction_dispersion"
    summary="$summary baseline_peak_memory=$average_baseline_memory"
    summary="$summary candidate_peak_memory=$average_candidate_memory"
    summary="$summary memory_saving=$average_memory_saving"
    summary="$summary memory_dispersion=$memory_dispersion"
    summary="$summary baseline_maximum_rss=$average_baseline_rss"
    summary="$summary candidate_maximum_rss=$average_candidate_rss"
    summary="$summary baseline_duration_ns=$average_baseline_duration"
    summary="$summary candidate_duration_ns=$average_candidate_duration"
    summary="$summary files=$measured_files records=$measured_records"
    printf '%s\n' "$summary" | tee -a "$results"
done
