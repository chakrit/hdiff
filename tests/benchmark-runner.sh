#!/bin/sh
set -eu

repo=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
mkdir -p "$repo/.ace/bench/runner-tests"
suite=$(mktemp -d "$repo/.ace/bench/runner-tests/run.XXXXXX")
mkdir -p "$suite/tools"

cat > "$suite/tools/git" <<'EOF'
#!/bin/sh
set -eu
[ "$1" = -C ] && shift 2
case "$1" in
    --version) printf 'git fixture\n' ;;
    clone) mkdir -p "$3/.git" ;;
    cat-file) exit 0 ;;
    rev-parse) printf '%040d\n' 1 ;;
    ls-files) exit 0 ;;
    status) exit 0 ;;
    diff)
        case "$*" in
            *--quiet*) [ "${FAKE_CLEAN:-no}" = yes ]; exit ;;
            *--binary*)
                if [ -f "$FAKE_ROOT/source-changed" ]; then
                    printf 'mutated source\n'
                else
                    [ "${FAKE_CLEAN:-no}" = yes ] || printf 'source change\n'
                fi
                ;;
            *) printf 'patch\n' ;;
        esac
        ;;
    worktree) mkdir -p "$4"; touch "$4/.git" ;;
    rev-list) printf 'a\nb\nc\nd\ne\nf\ng\nh\ni\n' ;;
    *) printf 'unexpected git command: %s\n' "$*" >&2; exit 1 ;;
esac
EOF
cat > "$suite/tools/cargo" <<'EOF'
#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$FAKE_ROOT/builds.txt"
case "$*" in *--version*) printf 'cargo fixture\n'; exit ;; esac
case "$FAKE_CASE" in build-failure) exit 17 ;; esac
target=
manifest=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --target-dir) target=$2; shift 2 ;;
        --manifest-path) manifest=$2; shift 2 ;;
        *) shift ;;
    esac
done
[ -n "$target" ] || target="${manifest%/Cargo.toml}/target"
mkdir -p "$target/release"
cp "$FAKE_ROOT/binary" "$target/release/hdiff"
chmod +x "$target/release/hdiff"
EOF
cat > "$suite/tools/rustc" <<'EOF'
#!/bin/sh
printf 'rustc fixture\n'
EOF
cat > "$suite/tools/time" <<'EOF'
#!/bin/sh
set -eu
[ "$1" = -lp ] || exit 1
shift
role=candidate
case "$1" in *baseline*) role=baseline ;; esac
count_file="$FAKE_ROOT/$role-count"
count=0
[ ! -f "$count_file" ] || count=$(cat "$count_file")
count=$((count + 1))
printf '%s\n' "$count" > "$count_file"
"$@"
cycles=100
instructions=100
memory=100
if [ "$role" = candidate ]; then
    case "$FAKE_CASE" in
        regressed) cycles=120 ;;
        noisy) cycles=80; [ "$count" -ne 2 ] || cycles=140 ;;
        instruction-regression) instructions=120; cycles=80 ;;
        memory-regression) memory=120; cycles=80 ;;
        *) cycles=80 ;;
    esac
fi
cat >&2 <<TIMING
real 0.10
user 0.09
sys 0.01
100 maximum resident set size
0 page faults
1 voluntary context switches
2 involuntary context switches
$instructions instructions retired
$cycles cycles elapsed
$memory peak memory footprint
TIMING
case "$FAKE_CASE" in
    duplicate-timing) printf '80 cycles elapsed\n' >&2 ;;
    measured-failure) exit 9 ;;
esac
EOF
cat > "$suite/tools/sample" <<'EOF'
#!/bin/sh
set -eu
printf 'sample started\n' > "$FAKE_ROOT/sample-signal"
case "$FAKE_CASE" in
    sample-failure) exit 7 ;;
    empty-stacks) : > "$5" ;;
    *) printf 'fixture sampled stacks\n' > "$5" ;;
esac
EOF
cat > "$suite/binary" <<'EOF'
#!/bin/sh
set -eu
cat > /dev/null
role=candidate
case "$0" in *baseline*) role=baseline ;; esac
printf '%s %s\n' "$role" "$1" >> "$FAKE_ROOT/executions.txt"
case "$FAKE_CASE" in
    failed) exit 9 ;;
    late-failure) [ "$(wc -l < "$FAKE_ROOT/executions.txt")" -le 10 ] || exit 9 ;;
    source-change) touch "$FAKE_ROOT/source-changed" ;;
    corpus-change)
        for patch in "$FAKE_ROOT/.ace/bench/kubernetes/runs/"*/7-8.patch; do
            printf 'other\n' > "$patch"
        done
        ;;
    runner-change) printf '# source mutation\n' >> "$FAKE_ROOT/bench/kubernetes.sh" ;;
esac
case "$FAKE_CASE:$1" in
    profile:--bench|sample-failure:--bench|empty-stacks:--bench)
        IFS= read -r _signal < "$FAKE_ROOT/sample-signal"
        ;;
esac
label=preparation
[ "$1" != --profile ] || label=diagnostic
duration=80
[ "$role" != baseline ] || duration=100
records=2
case "$FAKE_CASE:$role" in mismatch:candidate) records=3 ;; esac
printf '%s duration_ns=%s files=1 records=%s\n' "$label" "$duration" "$records"
case "$FAKE_CASE:$role" in legacy:baseline) exit ;; esac
ready=$((duration + 20))
case "$FAKE_CASE" in invalid-total) ready=1 ;; esac
printf 'startup version=1 input_ns=10 parse_ns=10 preparation_ns=%s ready_ns=%s bytes=6\n' \
    "$duration" "$ready"
case "$FAKE_CASE" in malformed) printf 'garbage\n' ;; esac
if [ "$1" = --profile ]; then
    printf 'profile version=1\n'
    for stage in labels layout syntax detail unattributed; do
        stage_duration=0
        [ "$stage" != unattributed ] || stage_duration=$duration
        case "$FAKE_CASE:$stage" in invalid-stages:syntax) stage_duration=1 ;; esac
        printf 'stage name=%s parent=preparation duration_ns=%s\n' "$stage" "$stage_duration"
    done
    for stage in language projection parse query unattributed; do
        printf 'stage name=%s parent=syntax duration_ns=0\n' "$stage"
    done
    printf 'work hunks=1 parse_calls=2 captures=3 projected_bytes=4\n'
fi
EOF
chmod +x "$suite/tools/"* "$suite/binary"

run_case() {
    case_name=$1
    mode=$2
    clean=$3
    fixture="$suite/$case_name"
    mkdir -p "$fixture/bench"
    cp "$repo/bench/kubernetes.sh" "$fixture/bench/kubernetes.sh"
    cp "$suite/binary" "$fixture/binary"
    printf '[package]\n' > "$fixture/Cargo.toml"
    mkfifo "$fixture/sample-signal"
    set -- --range 7/8
    [ "$case_name" != late-failure ] || set --
    [ "$mode" = compare ] || set -- "$@" --mode "$mode"
    if FAKE_ROOT="$fixture" FAKE_CASE="$case_name" FAKE_CLEAN="$clean" \
        PATH="$suite/tools:$PATH" HDIFF_TIME_COMMAND="$suite/tools/time" \
        HDIFF_SAMPLE_COMMAND="$suite/tools/sample" \
        sh "$fixture/bench/kubernetes.sh" "$@" \
        > "$fixture/output.txt" 2>&1; then
        case_status=0
    else
        case_status=$?
    fi
}

assert_contains() {
    if ! rg -q -- "$2" "$1"; then
        printf 'FAIL: %s missing %s\n' "$1" "$2" >&2
        cat "$fixture/output.txt" >&2
        exit 1
    fi
}

run_case improved compare no
[ "$case_status" -eq 0 ] || {
    printf 'FAIL: valid versioned measurements must produce a comparison\n' >&2
    cat "$fixture/output.txt" >&2
    exit 1
}
assert_contains "$fixture/output.txt" 'verdict=improved'
assert_contains "$fixture/output.txt" 'cycle_saving=20'
assert_contains "$fixture/builds.txt" '\+stable build --release --locked'
[ "$(wc -l < "$fixture/executions.txt")" -eq 10 ]
expected='baseline --bench
candidate --bench
baseline --bench
candidate --bench
baseline --bench
candidate --bench
candidate --bench
baseline --bench
baseline --bench
candidate --bench'
[ "$(cat "$fixture/executions.txt")" = "$expected" ]

for case_name in regressed noisy instruction-regression memory-regression legacy; do
    run_case "$case_name" compare no
    [ "$case_status" -eq 0 ] || exit 1
    case "$case_name" in
        regressed|instruction-regression|memory-regression) verdict=regressed ;;
        noisy) verdict=inconclusive ;;
        legacy) verdict=improved ;;
    esac
    assert_contains "$fixture/output.txt" "verdict=$verdict"
done
assert_contains "$fixture/output.txt" 'ready_saving_ns=unavailable'

for case_name in malformed invalid-total mismatch failed build-failure duplicate-timing measured-failure; do
    run_case "$case_name" compare no
    [ "$case_status" -ne 0 ] || { printf 'FAIL: accepted %s\n' "$case_name"; exit 1; }
    for run in "$fixture/.ace/bench/kubernetes/runs/"*; do
        assert_contains "$run/status.txt" 'status=failed'
        [ -f "$run/provenance.txt" ]
        if [ -f "$run/results.txt" ] && rg -q '^summary ' "$run/results.txt"; then
            printf 'FAIL: failed run has successful summary\n'; exit 1
        fi
    done
done

for case_name in source-change corpus-change runner-change; do
    run_case "$case_name" measure yes
    [ "$case_status" -ne 0 ] || { printf 'FAIL: accepted %s\n' "$case_name"; exit 1; }
    for run in "$fixture/.ace/bench/kubernetes/runs/"*; do
        assert_contains "$run/status.txt" 'status=failed'
    done
done

run_case late-failure compare no
[ "$case_status" -ne 0 ] || { printf 'FAIL: accepted late failure\n'; exit 1; }
if [ -f "$fixture/.ace/bench/kubernetes/results.txt" ]; then
    if rg -q '^summary ' "$fixture/.ace/bench/kubernetes/results.txt"; then
        printf 'FAIL: incomplete run published global summaries\n'; exit 1
    fi
fi

run_case measure measure yes
[ "$case_status" -eq 0 ] || { cat "$fixture/output.txt"; exit 1; }
[ "$(wc -l < "$fixture/executions.txt")" -eq 5 ]
for run in "$fixture/.ace/bench/kubernetes/runs/"*; do
    assert_contains "$run/status.txt" 'status=complete'
    for key in run_id runner_sha256 candidate_binary_sha256 corpus_sha256 toolchain; do
        assert_contains "$run/provenance.txt" "$key="
    done
    assert_contains "$run/7-8.corpus.command" '=--no-ext-diff$'
    assert_contains "$run/7-8.corpus.command" '=--no-textconv$'
    assert_contains "$run/provenance.txt" 'git fixture'
done

run_case invalid-stages profile yes
[ "$case_status" -ne 0 ] || { printf 'FAIL: accepted invalid stage partition\n'; exit 1; }
for run in "$fixture/.ace/bench/kubernetes/runs/"*; do
    assert_contains "$run/status.txt" 'status=failed'
done

run_case profile profile yes
[ "$case_status" -eq 0 ] || { cat "$fixture/output.txt"; exit 1; }
for run in "$fixture/.ace/bench/kubernetes/runs/"*; do
    assert_contains "$run/7-8.profile.txt" '^profile version=1$'
    assert_contains "$run/7-8.sampling.txt" 'coverage=partial'
    assert_contains "$run/7-8.sampling.txt" 'sample_status=0'
    assert_contains "$run/7-8.sampling.txt" 'stacks=captured'
    assert_contains "$run/7-8.stacks.txt" 'fixture sampled stacks'
    if [ -f "$run/results.txt" ] && rg -q '^sample |^summary ' "$run/results.txt"; then
        printf 'FAIL: diagnostic entered acceptance samples\n'; exit 1
    fi
done
run_case sample-failure profile yes
[ "$case_status" -eq 0 ] || { cat "$fixture/output.txt"; exit 1; }
for run in "$fixture/.ace/bench/kubernetes/runs/"*; do
    assert_contains "$run/7-8.sampling.txt" 'sample_status=7'
    assert_contains "$run/7-8.sampling.txt" 'stacks=unavailable'
done
run_case empty-stacks profile yes
[ "$case_status" -eq 0 ] || { cat "$fixture/output.txt"; exit 1; }
for run in "$fixture/.ace/bench/kubernetes/runs/"*; do
    assert_contains "$run/7-8.sampling.txt" 'sample_status=0'
    assert_contains "$run/7-8.sampling.txt" 'stacks=empty'
done

printf 'benchmark runner tests passed\n'
