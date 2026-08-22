#!/bin/sh

set -o errexit

SCRIPT_DIRECTORY=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
HDIFF="$SCRIPT_DIRECTORY/target/release/hdiff"

(cd "$SCRIPT_DIRECTORY" && cargo build --release)
git -c "core.pager=$HDIFF" "$@"
