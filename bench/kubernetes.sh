#!/bin/sh
set -eu

readonly kubernetes_commit=70d3cc986aa8221cd1dfb1121852688902d3bf53
readonly kubernetes_remote=https://github.com/kubernetes/kubernetes.git
readonly digest_command=/usr/bin/shasum
readonly time_command="${HDIFF_TIME_COMMAND:-/usr/bin/time}"
readonly sample_command="${HDIFF_SAMPLE_COMMAND:-/usr/bin/sample}"
readonly toolchain=stable

usage() {
    printf 'usage: %s [--mode compare|measure|profile] [--range 7/8|6/8|4/8]\n' "$0" >&2
}

fail() {
    printf '%s\n' "$1" >&2
    exit 1
}

require_unsigned() {
    validation_name=$1
    validation_value=$2
    case "$validation_value" in
        ''|*[!0-9]*|0[0-9]*) fail "invalid $validation_name: $validation_value" ;;
    esac
    [ "${#validation_value}" -le 17 ] || fail "$validation_name exceeds safe arithmetic range"
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

digest() {
    digest_line=$("$digest_command" -a 256 "$1")
    printf '%s\n' "${digest_line%% *}"
}

finish() {
    exit_status=$?
    trap - 0
    if [ -n "${process_pid-}" ]; then
        if kill -0 "$process_pid" 2> "$run_root/cleanup-process-check.stderr"; then
            cleanup_status=0
            kill "$process_pid" 2> "$run_root/cleanup-kill.stderr" || cleanup_status=$?
            printf 'cleanup_signal_status=%s\n' "$cleanup_status" >> "$run_root/status.txt"
        fi
        child_status=0
        wait "$process_pid" || child_status=$?
        printf 'cleanup_child_status=%s\n' "$child_status" >> "$run_root/status.txt"
    fi
    if [ "$exit_status" -eq 0 ]; then
        printf 'status=complete\n' >> "$run_root/status.txt"
    else
        printf 'status=failed exit_code=%s\n' "$exit_status" >> "$run_root/status.txt"
    fi
    printf 'artifacts=%s\n' "$run_root" >&2
    exit "$exit_status"
}

record_command() {
    command_file=$1
    command_input=$2
    command_output=$3
    command_error=$4
    shift 4
    command_index=0

    {
        printf 'stdin=%s\nstdout=%s\nstderr=%s\n' "$command_input" "$command_output" "$command_error"
        for argument do
            printf 'argv[%s]=%s\n' "$command_index" "$argument"
            command_index=$((command_index + 1))
        done
    } > "$command_file"
}

validate_report() {
    report_file=$1
    report_kind=$2
    report_line=0
    ready_ns=unavailable
    input_ns=unavailable
    parse_ns=unavailable
    report_bytes=unavailable
    stage_names=
    preparation_stage_sum=0
    syntax_stage_sum=0
    syntax_duration=0

    while IFS= read -r line || [ -n "$line" ]; do
        report_line=$((report_line + 1))
        case "$report_line" in
            1)
                IFS=' ' read -r label duration_field files_field records_field _extra <<EOF
$line
EOF
                duration=${duration_field#duration_ns=}
                file_count=${files_field#files=}
                record_count=${records_field#records=}
                require_unsigned duration_ns "$duration"
                require_unsigned files "$file_count"
                require_unsigned records "$record_count"
                canonical="$report_kind duration_ns=$duration files=$file_count records=$record_count"
                [ "$line" = "$canonical" ] || fail "invalid benchmark result: $line"
                ;;
            2)
                IFS=' ' read -r label _version input_field parse_field prep_field ready_field bytes_field _extra <<EOF
$line
EOF
                input_ns=${input_field#input_ns=}
                parse_ns=${parse_field#parse_ns=}
                preparation_ns=${prep_field#preparation_ns=}
                ready_ns=${ready_field#ready_ns=}
                report_bytes=${bytes_field#bytes=}
                require_unsigned input_ns "$input_ns"
                require_unsigned parse_ns "$parse_ns"
                require_unsigned preparation_ns "$preparation_ns"
                require_unsigned ready_ns "$ready_ns"
                require_unsigned bytes "$report_bytes"
                canonical="startup version=1 input_ns=$input_ns parse_ns=$parse_ns"
                canonical="$canonical preparation_ns=$preparation_ns ready_ns=$ready_ns bytes=$report_bytes"
                [ "$line" = "$canonical" ] || fail "invalid startup result: $line"
                [ "$duration" -eq "$preparation_ns" ] || fail 'preparation durations differ'
                [ "$ready_ns" -ge $((input_ns + parse_ns + preparation_ns)) ] || fail 'invalid ready total'
                [ "$report_bytes" -eq "$corpus_bytes" ] || fail 'input bytes differ from corpus'
                ;;
            3)
                [ "$report_kind" = diagnostic ] && [ "$line" = 'profile version=1' ] ||
                    fail "unexpected report line: $line"
                ;;
            *)
                [ "$report_kind" = diagnostic ] || fail "unexpected report line: $line"
                validate_profile_line "$line"
                ;;
        esac
    done < "$report_file"

    [ "$report_line" -gt 0 ] || fail 'empty benchmark result'
    if [ "$report_kind" = diagnostic ]; then
        expected=' preparation:labels preparation:layout preparation:syntax preparation:detail preparation:unattributed syntax:language syntax:projection syntax:parse syntax:query syntax:unattributed work'
        [ "$stage_names" = "$expected" ] || fail 'incomplete or reordered profile stages'
        [ "$preparation_stage_sum" -eq "$duration" ] || fail 'preparation stages do not partition their parent'
        [ "$syntax_stage_sum" -eq "$syntax_duration" ] || fail 'syntax stages do not partition their parent'
    fi
}

validate_profile_line() {
    IFS=' ' read -r label first second third fourth _extra <<EOF
$1
EOF
    case "$label" in
        stage)
            stage_name=${first#name=}
            stage_parent=${second#parent=}
            stage_duration=${third#duration_ns=}
            require_unsigned stage_duration_ns "$stage_duration"
            [ "$1" = "stage name=$stage_name parent=$stage_parent duration_ns=$stage_duration" ] ||
                fail "invalid stage: $1"
            stage_names="$stage_names $stage_parent:$stage_name"
            case "$stage_parent:$stage_name" in
                preparation:syntax) syntax_duration=$stage_duration ;;
            esac
            case "$stage_parent" in
                preparation) preparation_stage_sum=$((preparation_stage_sum + stage_duration)) ;;
                syntax) syntax_stage_sum=$((syntax_stage_sum + stage_duration)) ;;
                *) fail "unknown stage parent: $stage_parent" ;;
            esac
            ;;
        work)
            hunks=${first#hunks=}
            parse_calls=${second#parse_calls=}
            captures=${third#captures=}
            projected_bytes=${fourth#projected_bytes=}
            require_unsigned hunks "$hunks"
            require_unsigned parse_calls "$parse_calls"
            require_unsigned captures "$captures"
            require_unsigned projected_bytes "$projected_bytes"
            canonical="work hunks=$hunks parse_calls=$parse_calls captures=$captures projected_bytes=$projected_bytes"
            [ "$1" = "$canonical" ] || fail "invalid work counters: $1"
            stage_names="$stage_names work"
            ;;
        *) fail "invalid profile line: $1" ;;
    esac
}

check_identity() {
    [ "$(digest "$runner_path")" = "$runner_digest" ] || fail 'runner changed during run'
    [ "$(git -C "$repo_root" rev-parse --verify HEAD)" = "$baseline_commit" ] || fail 'source revision changed during run'
    git -C "$repo_root" diff --binary "$baseline_commit" -- \
        Cargo.toml Cargo.lock build.rs rust-toolchain.toml rust-toolchain .cargo src \
        > "$run_root/identity-source.patch"
    [ "$(digest "$run_root/identity-source.patch")" = "$candidate_digest" ] || fail 'source changed during run'
    untracked=$(git -C "$repo_root" ls-files --others --exclude-standard -- \
        Cargo.toml Cargo.lock build.rs rust-toolchain.toml rust-toolchain .cargo src)
    [ -z "$untracked" ] || fail 'untracked build inputs appeared during run'
    [ "$(digest "$candidate_binary")" = "$candidate_binary_digest" ] || fail 'candidate binary changed during run'
    if [ "$mode" = compare ]; then
        [ "$(digest "$baseline_binary")" = "$baseline_binary_digest" ] || fail 'baseline binary changed during run'
        baseline_status=$(git -C "$baseline_checkout" status --porcelain --untracked-files=all)
        [ -z "$baseline_status" ] || fail 'baseline source changed during run'
    fi
    if [ -n "${patch-}" ]; then
        [ "$(digest "$patch")" = "$corpus_digest" ] || fail 'corpus changed during run'
    fi
}

warmup() {
    warmup_binary=$1
    warmup_role=$2
    warmup_number=$3
    warmup_output="$run_root/$numerator-8-warmup-$warmup_number-$warmup_role.benchmark.txt"
    record_command "$warmup_output.command" "$patch" "$warmup_output" "$warmup_output.stderr" "$warmup_binary" --bench
    "$warmup_binary" --bench < "$patch" > "$warmup_output" 2> "$warmup_output.stderr"
    validate_report "$warmup_output" preparation
}

profile() {
    record_command "$run_root/$numerator-8.profile.command" "$patch" \
        "$run_root/$numerator-8.profile.txt" "$run_root/$numerator-8.profile.stderr" "$candidate_binary" --profile
    "$candidate_binary" --profile < "$patch" > "$run_root/$numerator-8.profile.txt" \
        2> "$run_root/$numerator-8.profile.stderr"
    validate_report "$run_root/$numerator-8.profile.txt" diagnostic
    sampling_log="$run_root/$numerator-8.sampling.txt"
    printf 'requested_duration_s=10 interval_ms=1\n' > "$sampling_log"
    printf 'process_start_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$sampling_log"
    record_command "$run_root/$numerator-8.sampled-benchmark.command" "$patch" \
        "$run_root/$numerator-8.sampled-benchmark.txt" "$run_root/$numerator-8.sampled-benchmark.stderr" "$candidate_binary" --bench
    "$candidate_binary" --bench < "$patch" > "$run_root/$numerator-8.sampled-benchmark.txt" \
        2> "$run_root/$numerator-8.sampled-benchmark.stderr" &
    process_pid=$!
    printf 'pid=%s command=%s %s 10 1 -file %s\n' "$process_pid" "$sample_command" \
        "$process_pid" "$run_root/$numerator-8.stacks.txt" >> "$sampling_log"
    record_command "$run_root/$numerator-8.sample.command" /dev/null \
        "$run_root/$numerator-8.sample.stdout" "$run_root/$numerator-8.sample.stderr" \
        "$sample_command" "$process_pid" 10 1 -file "$run_root/$numerator-8.stacks.txt"
    if kill -0 "$process_pid" 2> "$run_root/$numerator-8.process-check.stderr"; then
        sample_status=0
        printf 'sample_start_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$sampling_log"
        "$sample_command" "$process_pid" 10 1 -file "$run_root/$numerator-8.stacks.txt" \
            < /dev/null > "$run_root/$numerator-8.sample.stdout" \
            2> "$run_root/$numerator-8.sample.stderr" || sample_status=$?
        printf 'sample_end_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$sampling_log"
    else
        sample_status=not-started
    fi
    process_status=0
    wait "$process_pid" || process_status=$?
    process_pid=
    printf 'sample_status=%s process_status=%s process_end_utc=%s\n' "$sample_status" \
        "$process_status" "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$sampling_log"
    stacks=unavailable
    coverage=unavailable
    if [ "$sample_status" = 0 ]; then
        stacks=empty
        if [ -s "$run_root/$numerator-8.stacks.txt" ]; then
            stacks=captured
            coverage=partial
        fi
    fi
    printf 'stacks=%s coverage=%s\n' "$stacks" "$coverage" >> "$sampling_log"
    [ "$process_status" -eq 0 ] || fail 'sampled benchmark process failed'
    validate_report "$run_root/$numerator-8.sampled-benchmark.txt" preparation
    printf 'profile run_id=%s range=%s/8 sample_status=%s stacks=%s coverage=%s\n' \
        "$run_id" "$numerator" "$sample_status" "$stacks" "$coverage"
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

    record_command "$run_root/$sample_name.command" "$measured_patch" "$benchmark_output" \
        "$time_output" "$time_command" -lp "$measured_binary" --bench
    LC_ALL=C "$time_command" -lp "$measured_binary" --bench \
        < "$measured_patch" > "$benchmark_output" 2> "$time_output"

    validate_report "$benchmark_output" preparation

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
    timing_fields=
    while IFS= read -r timing_line; do
        timing_key=
        case "$timing_line" in
            'real '*) timing_key=real ;;
            'user '*) timing_key=user ;;
            'sys '*) timing_key=sys ;;
            *' maximum resident set size') timing_key=rss ;;
            *' page faults') timing_key=faults ;;
            *' voluntary context switches') timing_key=voluntary ;;
            *' involuntary context switches') timing_key=involuntary ;;
            *' instructions retired') timing_key=instructions ;;
            *' cycles elapsed') timing_key=cycles ;;
            *' peak memory footprint') timing_key=memory ;;
        esac
        if [ -n "$timing_key" ]; then
            case "$timing_fields" in *" $timing_key "*) fail "duplicate timing field: $timing_key" ;; esac
            timing_fields="$timing_fields $timing_key "
        fi
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

    raw_sample="sample run_id=$run_id range=$measured_range/8 pair=$measured_pair"
    raw_sample="$raw_sample order=$measured_order role=$measured_role"
    raw_sample="$raw_sample revision=$measured_revision duration_ns=$duration"
    raw_sample="$raw_sample cycles=$cycles instructions=$instructions"
    raw_sample="$raw_sample maximum_rss=$maximum_rss peak_memory=$peak_memory"
    raw_sample="$raw_sample real_s=$real_seconds user_s=$user_seconds"
    raw_sample="$raw_sample system_s=$system_seconds page_faults=$page_faults"
    raw_sample="$raw_sample voluntary_switches=$voluntary_switches"
    raw_sample="$raw_sample involuntary_switches=$involuntary_switches"
    raw_sample="$raw_sample files=$file_count records=$record_count"
    raw_sample="$raw_sample input_ns=$input_ns parse_ns=$parse_ns ready_ns=$ready_ns bytes=$report_bytes"
    printf '%s\n' "$raw_sample" >> "$results"
    printf 'measured range=%s/8 pair=%s role=%s\n' \
        "$measured_range" "$measured_pair" "$measured_role" >&2
    printf '%s %s %s %s %s %s %s %s\n' \
        "$duration" "$cycles" "$instructions" "$peak_memory" \
        "$maximum_rss" "$file_count" "$record_count" "$ready_ns"
}

range_numerators='7 6 4'
mode=compare
while [ "$#" -gt 0 ]; do
    case "$1" in
        --mode)
            [ "$#" -ge 2 ] || fail 'missing --mode value'
            case "$2" in compare|measure|profile) mode=$2 ;; *) usage; exit 1 ;; esac
            shift 2
            ;;
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
runner_path="$repo_root/bench/kubernetes.sh"
benchmark_root="$repo_root/.ace/bench/kubernetes"
checkout="$benchmark_root/checkout"
baseline_commit=$(git -C "$repo_root" rev-parse --verify HEAD)
run_id=$(date -u '+%Y%m%dT%H%M%SZ')-$baseline_commit-$$
run_root="$benchmark_root/runs/$run_id"
mkdir -p "$run_root"
trap finish 0
trap 'exit 130' INT
trap 'exit 143' TERM
history="$run_root/first-parent.txt"
results="$run_root/results.txt"
provenance="$run_root/provenance.txt"
candidate_binary="$run_root/candidate-hdiff"
cp "$runner_path" "$run_root/runner.sh"
runner_digest=$(digest "$run_root/runner.sh")
printf 'status=running\n' > "$run_root/status.txt"
printf 'run_id=%s mode=%s utc=%s toolchain=%s\n' "$run_id" "$mode" \
    "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$toolchain" > "$provenance"
{
printf 'runner_sha256=%s time_command=%s sample_command=%s\n' \
    "$runner_digest" "$time_command" "$sample_command"
case "$mode" in
    compare)
        printf 'protocol=two-warmups-three-pairs order=baseline-candidate,candidate-baseline,baseline-candidate heuristic=max-absolute-deviation\n'
        ;;
    measure) printf 'protocol=two-warmups-three-repetitions role=candidate\n' ;;
    profile) printf 'protocol=one-stage-profile-one-separate-stack-sample\n' ;;
esac
printf 'ranges=%s\n' "$range_numerators"
printf 'RUSTFLAGS=%s\nCARGO_ENCODED_RUSTFLAGS=%s\nRUSTC_WRAPPER=%s\nRUSTC_WORKSPACE_WRAPPER=%s\n' \
    "${RUSTFLAGS-}" "${CARGO_ENCODED_RUSTFLAGS-}" "${RUSTC_WRAPPER-}" \
    "${RUSTC_WORKSPACE_WRAPPER-}"
printf 'CARGO_BUILD_JOBS=%s\nCARGO_HOME=%s\nRUSTUP_HOME=%s\n' \
    "${CARGO_BUILD_JOBS-}" "${CARGO_HOME-}" "${RUSTUP_HOME-}"
env | LC_ALL=C sort | while IFS= read -r environment_line; do
    case "$environment_line" in
        CARGO_PROFILE_RELEASE_*=*|CARGO_TARGET_*_RUSTFLAGS=*|CARGO_TARGET_*_LINKER=*|\
        CC=*|CC_*=*|CXX=*|CXX_*=*|CFLAGS=*|CFLAGS_*=*|CXXFLAGS=*|CXXFLAGS_*=*|\
        AR=*|AR_*=*|HOST_CC=*|TARGET_CC=*|HOST_CXX=*|TARGET_CXX=*|\
        HOST_CFLAGS=*|TARGET_CFLAGS=*|HOST_CXXFLAGS=*|TARGET_CXXFLAGS=*|\
        HOST_AR=*|TARGET_AR=*|SDKROOT=*|MACOSX_DEPLOYMENT_TARGET=*)
            printf '%s\n' "$environment_line"
            ;;
    esac
done
printf 'LC_ALL=C measured_environment=inherit cwd=%s\n' "$PWD"
printf 'measurement_commands=*.command warmup_commands=*.command profile_commands=*.command\n'
uname -a
if [ -x /usr/sbin/sysctl ]; then
    /usr/sbin/sysctl hw.model hw.memsize hw.ncpu
fi
cargo +"$toolchain" --version
rustc +"$toolchain" -Vv
git --version
if command -v cc > /dev/null 2>&1; then
    cc --version
fi
} >> "$provenance"

[ -x "$time_command" ] || fail "missing executable: $time_command"
[ -x "$digest_command" ] || fail "missing executable: $digest_command"
[ "$mode" != profile ] || [ -x "$sample_command" ] || fail "missing executable: $sample_command"

if [ ! -d "$checkout/.git" ]; then
    mkdir -p "$benchmark_root"
    git clone "$kubernetes_remote" "$checkout"
fi

git -C "$checkout" cat-file -e "$kubernetes_commit^{commit}"
baseline_root="$benchmark_root/hdiff-baselines/$baseline_commit"
baseline_checkout="$baseline_root/source"
baseline_target="$baseline_root/target"
baseline_binary="$run_root/baseline-hdiff"

untracked_source_paths=$(git -C "$repo_root" ls-files --others --exclude-standard -- \
    Cargo.toml Cargo.lock build.rs rust-toolchain.toml rust-toolchain .cargo src)
if [ -n "$untracked_source_paths" ]; then
    fail 'hdiff candidate contains untracked build inputs; add them to the index before benchmarking'
fi
if [ "$mode" = compare ]; then
    if git -C "$repo_root" diff --quiet "$baseline_commit" -- \
        Cargo.toml Cargo.lock build.rs rust-toolchain.toml rust-toolchain .cargo src; then
        fail 'hdiff candidate has no tracked build-input changes from its baseline commit'
    fi
fi

candidate_diff="$run_root/hdiff-candidate-source.patch"
git -C "$repo_root" diff --binary "$baseline_commit" -- \
    Cargo.toml Cargo.lock build.rs rust-toolchain.toml rust-toolchain .cargo src \
    > "$candidate_diff"
candidate_digest=$(digest "$candidate_diff")
require_sha256 candidate_sha256 "$candidate_digest"
candidate_revision="$baseline_commit-dirty-$candidate_digest"
[ -s "$candidate_diff" ] || candidate_revision=$baseline_commit
printf 'baseline=%s candidate=%s source_patch_sha256=%s\n' \
    "$baseline_commit" "$candidate_revision" "$candidate_digest" >> "$provenance"

if [ "$mode" = compare ]; then
    if [ ! -e "$baseline_checkout/.git" ]; then
        mkdir -p "$baseline_root"
        git -C "$repo_root" worktree add --detach "$baseline_checkout" "$baseline_commit"
    fi
    checked_out_baseline=$(git -C "$baseline_checkout" rev-parse HEAD)
    [ "$checked_out_baseline" = "$baseline_commit" ] || fail 'baseline worktree has unexpected revision'
    baseline_status=$(git -C "$baseline_checkout" status --porcelain --untracked-files=all)
    [ -z "$baseline_status" ] || fail 'baseline worktree is dirty'
    printf 'build=cargo +%s build --release --locked --manifest-path %s --target-dir %s\n' \
        "$toolchain" "$baseline_checkout/Cargo.toml" "$baseline_target" >> "$provenance"
    record_command "$run_root/baseline-build.command" /dev/null \
        "$run_root/baseline-build.stdout" "$run_root/baseline-build.stderr" \
        cargo +"$toolchain" build --release --locked --manifest-path "$baseline_checkout/Cargo.toml" \
        --target-dir "$baseline_target"
    cargo +"$toolchain" build --release --locked --manifest-path "$baseline_checkout/Cargo.toml" \
        --target-dir "$baseline_target" < /dev/null \
        > "$run_root/baseline-build.stdout" 2> "$run_root/baseline-build.stderr"
    cp "$baseline_target/release/hdiff" "$baseline_binary"
    baseline_binary_digest=$(digest "$baseline_binary")
    printf 'baseline_binary_sha256=%s\n' "$baseline_binary_digest" >> "$provenance"
fi

printf 'build=cargo +%s build --release --locked --manifest-path %s --target-dir %s\n' \
    "$toolchain" "$repo_root/Cargo.toml" "$repo_root/target" >> "$provenance"
record_command "$run_root/candidate-build.command" /dev/null \
    "$run_root/candidate-build.stdout" "$run_root/candidate-build.stderr" \
    cargo +"$toolchain" build --release --locked --manifest-path "$repo_root/Cargo.toml" \
    --target-dir "$repo_root/target"
cargo +"$toolchain" build --release --locked --manifest-path "$repo_root/Cargo.toml" \
    --target-dir "$repo_root/target" < /dev/null \
    > "$run_root/candidate-build.stdout" 2> "$run_root/candidate-build.stderr"
cp "$repo_root/target/release/hdiff" "$candidate_binary"
candidate_binary_digest=$(digest "$candidate_binary")
printf 'candidate_binary_sha256=%s\n' "$candidate_binary_digest" >> "$provenance"
check_identity
git -C "$checkout" rev-list --first-parent --reverse "$kubernetes_commit" > "$history"

edge_count=$(($(wc -l < "$history") - 1))
[ "$edge_count" -gt 0 ] || fail 'corpus history is incomplete'

for numerator in $range_numerators; do
    base_index=$((edge_count * numerator / 8))
    base_commit=$(sed -n "$((base_index + 1))p" "$history")
    patch="$run_root/$numerator-8.patch"

    record_command "$run_root/$numerator-8.corpus.command" /dev/null "$patch" \
        "$run_root/$numerator-8.corpus.stderr" git -C "$checkout" diff --no-ext-diff --no-textconv \
        "$base_commit" "$kubernetes_commit" -- '*.go'
    git -C "$checkout" diff --no-ext-diff --no-textconv "$base_commit" "$kubernetes_commit" -- '*.go' \
        < /dev/null > "$patch" 2> "$run_root/$numerator-8.corpus.stderr"
    corpus_bytes=$(wc -c < "$patch" | tr -d ' ')
    corpus_digest=$(digest "$patch")
    printf 'range=%s/8 corpus_base=%s corpus_target=%s corpus_sha256=%s corpus_bytes=%s\n' \
        "$numerator" "$base_commit" "$kubernetes_commit" "$corpus_digest" "$corpus_bytes" >> "$provenance"
    check_identity
    if [ "$mode" = profile ]; then
        profile
        check_identity
        continue
    fi
    for warmup_number in 1 2; do
        [ "$mode" != compare ] || warmup "$baseline_binary" baseline "$warmup_number"
        warmup "$candidate_binary" candidate "$warmup_number"
    done

    if [ "$mode" = measure ]; then
        measured_counts=
        for repeat in 1 2 3; do
            measurement=$(measure "$candidate_binary" "$candidate_revision" candidate "$numerator" "$repeat" only "$patch")
            IFS=' ' read -r _duration _cycles _instructions _memory _rss files records _ready <<EOF
$measurement
EOF
            counts="$files $records"
            [ -z "$measured_counts" ] || [ "$measured_counts" = "$counts" ] || fail 'sample counts changed'
            measured_counts=$counts
        done
        check_identity
        printf 'measurement run_id=%s range=%s/8 samples=3\n' "$run_id" "$numerator" | tee -a "$results"
        continue
    fi

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
    baseline_ready_sum=0
    candidate_ready_sum=0
    baseline_schema=
    candidate_schema=

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
            baseline_memory baseline_rss baseline_files baseline_records baseline_ready <<EOF
$baseline_sample
EOF
        IFS=' ' read -r \
            candidate_duration candidate_cycles candidate_instructions \
            candidate_memory candidate_rss candidate_files candidate_records candidate_ready <<EOF
$candidate_sample
EOF
        baseline_version=startup-v1
        candidate_version=startup-v1
        [ "$baseline_ready" != unavailable ] || baseline_version=legacy
        [ "$candidate_ready" != unavailable ] || candidate_version=legacy
        [ -z "$baseline_schema" ] || [ "$baseline_schema" = "$baseline_version" ] || fail 'baseline schema changed'
        [ -z "$candidate_schema" ] || [ "$candidate_schema" = "$candidate_version" ] || fail 'candidate schema changed'
        baseline_schema=$baseline_version
        candidate_schema=$candidate_version
        if [ "$baseline_version" = startup-v1 ] && [ "$candidate_version" = startup-v1 ]; then
            baseline_ready_sum=$((baseline_ready_sum + baseline_ready))
            candidate_ready_sum=$((candidate_ready_sum + candidate_ready))
        fi

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
    elif [ $((average_cycle_saving + cycle_dispersion)) -lt 0 ] || \
        [ $((average_instruction_saving + instruction_dispersion)) -lt 0 ] || \
        [ $((average_memory_saving + memory_dispersion)) -lt 0 ]; then
        verdict=regressed
    fi

    ready_saving=unavailable
    if [ "$baseline_schema" = startup-v1 ] && [ "$candidate_schema" = startup-v1 ]; then
        ready_saving=$(((baseline_ready_sum - candidate_ready_sum) / 3))
    fi
    check_identity
    summary="summary run_id=$run_id range=$numerator/8 corpus_base=$base_commit"
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
    summary="$summary preparation_saving_ns=$((average_baseline_duration - average_candidate_duration))"
    summary="$summary ready_saving_ns=$ready_saving baseline_schema=$baseline_schema candidate_schema=$candidate_schema"
    summary="$summary files=$measured_files records=$measured_records"
    printf '%s\n' "$summary" | tee -a "$results"
done

if [ "$mode" = compare ]; then
    sed -n '/^summary /p' "$results" >> "$benchmark_root/results.txt"
fi
