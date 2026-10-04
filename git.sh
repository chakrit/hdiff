#!/bin/sh

set -o errexit

SCRIPT_DIRECTORY=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
HDIFF="$SCRIPT_DIRECTORY/target/release/hdiff"

# Git prefers a command's own pager.<command> over core.pager, so each configured pager
# command (such as the one `hdiff --install` writes) is pointed at this build too.
# Boolean pager.<command> values only switch paging on or off and stay as configured.
configured_pager_keys() {
    # --get-regexp exits 1 when no key matches.
    git config --name-only --get-regexp '^pager\.' || [ $? -eq 1 ]
}

is_boolean() {
    git config --type=bool --get "$1" >/dev/null 2>&1
}

(cd "$SCRIPT_DIRECTORY" && cargo build --release)

pager_keys=$(configured_pager_keys)
for key in $pager_keys; do
    if ! is_boolean "$key"; then
        set -- -c "$key=$HDIFF" "$@"
    fi
done

git -c "core.pager=$HDIFF" "$@"
