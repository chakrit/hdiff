#!/bin/sh
# Probes each derived trait in slice 17a's files: a trait is unused when the crate and
# every test target still type-check without it. Each probe edits one derive line in
# place and restores the file from a copy afterwards.
set -u

CARGO_TARGET_DIR=/Users/chakrit/Documents/chakrit/hdiff/target
export CARGO_TARGET_DIR

results=tmp-17a-derive-results.txt
backup=tmp-17a-derive-backup.rs
: > "$results"

files="src/main.rs src/cli.rs src/input.rs src/comparison.rs src/parser.rs
src/document.rs src/file_tree.rs src/prepared.rs src/source_text.rs src/measurement.rs
src/bench.rs src/actions/backup_global_git_config.rs src/actions/child_process.rs
src/actions/install_git_diff_pager.rs src/actions/mod.rs src/file_identity.rs
src/side.rs src/headers.rs src/exit_status.rs src/non_empty.rs src/utf8_path.rs
src/preferences.rs tests/common/mod.rs tests/cli.rs tests/git_pager.rs
tests/benchmark.rs"

without() {
    list=$1
    shift
    for trait in "$@"; do
        list=$(printf '%s' ",$list," | sed "s/,$trait,/,/; s/^,//; s/,\$//")
    done
    printf '%s' "$list"
}

probe() {
    file=$1
    line=$2
    label=$3
    remaining=$4

    cp "$file" "$backup"
    if [ -z "$remaining" ]; then
        replacement=''
    else
        replacement="#[derive($(printf '%s' "$remaining" | sed 's/,/, /g'))]"
    fi
    sed -i '' "${line}s/#\[derive([^)]*)\]/$replacement/" "$file"
    if cargo check --all-targets --locked -q > /dev/null 2>&1; then
        echo "$file:$line removable $label" >> "$results"
    fi
    cp "$backup" "$file"
}

for file in $files; do
    grep -n '#\[derive(' "$file" | while IFS=: read -r line text; do
        traits=$(printf '%s' "$text" | sed -E 's/.*derive\(([^)]*)\).*/\1/' | tr -d ' ')
        for trait in Copy Clone Eq PartialEq Debug Default Hash; do
            case ",$traits," in
                *",$trait,"*) ;;
                *) continue ;;
            esac
            case $trait in
                Clone) probe "$file" "$line" "Clone(+Copy)" "$(without "$traits" Clone Copy)" ;;
                PartialEq) probe "$file" "$line" "PartialEq(+Eq)" "$(without "$traits" PartialEq Eq)" ;;
                *) probe "$file" "$line" "$trait" "$(without "$traits" "$trait")" ;;
            esac
        done
    done
done

echo "probe done" >> "$results"
