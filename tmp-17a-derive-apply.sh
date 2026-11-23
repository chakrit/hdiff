#!/bin/sh
# Rewrites each probed derive line to the traits something still uses; an empty list
# blanks the line, and the blank lines are removed afterwards by line pattern.
set -eu

set_derive() {
    file=$1
    line=$2
    traits=$3
    if [ -z "$traits" ]; then
        sed -i '' "${line}s/^#\[derive([^)]*)\]\$/__DROP_DERIVE__/" "$file"
    else
        sed -i '' "${line}s/#\[derive([^)]*)\]/#[derive($traits)]/" "$file"
    fi
}

set_derive src/main.rs 121 ''
set_derive src/cli.rs 46 'Debug, PartialEq'
set_derive src/cli.rs 55 'Debug, PartialEq'
set_derive src/input.rs 60 ''
set_derive src/input.rs 84 ''
set_derive src/comparison.rs 21 'Debug, PartialEq, Eq'
set_derive src/comparison.rs 37 'Debug, PartialEq, Eq'
set_derive src/parser.rs 18 'Debug, PartialEq'
set_derive src/parser.rs 122 'PartialEq'
set_derive src/document.rs 17 'Debug, PartialEq'
set_derive src/document.rs 209 ''
set_derive src/document.rs 248 'Clone, Copy, PartialEq'
set_derive src/document.rs 355 ''
set_derive src/file_tree.rs 67 'Debug, PartialEq'
set_derive src/file_tree.rs 88 'Debug, PartialEq'
set_derive src/file_tree.rs 139 'Clone, Copy, Default, PartialEq'
set_derive src/prepared.rs 34 'Debug, PartialEq'
set_derive src/prepared.rs 151 ''
set_derive src/measurement.rs 4 'Debug, Clone, Copy, PartialEq'
set_derive src/measurement.rs 60 'Debug, Clone, Copy, PartialEq'
set_derive src/bench.rs 180 ''
set_derive src/actions/install_git_diff_pager.rs 52 'Clone, Copy, Debug'
set_derive src/headers.rs 41 ''
set_derive src/exit_status.rs 5 'Debug, Clone, Copy, PartialEq'
set_derive src/non_empty.rs 7 'Debug, PartialEq, Eq'
set_derive src/utf8_path.rs 6 'Debug'

for file in src/main.rs src/input.rs src/document.rs src/prepared.rs src/bench.rs \
    src/headers.rs; do
    sed -i '' '/^__DROP_DERIVE__$/d' "$file"
done
