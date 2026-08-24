#!/bin/sh
set -eu

fixture=$1
binary=$2
session="hdiff-smoke-$$"
target="$session:0.0"

cleanup() {
    if tmux has-session -t "$session" 2>/dev/null; then
        tmux kill-session -t "$session"
    fi
}

wait_for() {
    expected=$1
    attempt=0

    while [ "$attempt" -lt 20 ]; do
        screen=$(tmux capture-pane -p -t "$target")
        case $screen in
            *"$expected"*)
                return 0
                ;;
        esac

        attempt=$((attempt + 1))
        sleep 0.05
    done

    printf 'terminal did not render %s\n' "$expected" >&2
    return 1
}

capture() {
    label=$1

    printf '=== %s ===\n' "$label"
    tmux capture-pane -p -e -t "$target"
}

wait_for_exit() {
    attempt=0

    while [ "$attempt" -lt 20 ]; do
        if ! tmux has-session -t "$session" 2>/dev/null; then
            return 0
        fi

        attempt=$((attempt + 1))
        sleep 0.05
    done

    printf 'terminal did not exit after q\n' >&2
    return 1
}

trap cleanup EXIT HUP INT TERM

tmux new-session -d -x 120 -y 36 -s "$session" env TERM=screen-256color "$binary" "$fixture"

wait_for 'pub fn greeting()'
capture Unified

tmux send-keys -t "$target" v
wait_for 'pub fn greeting()'
capture Vertical

tmux send-keys -t "$target" v
wait_for 'pub fn greeting()'
capture Stacked

tmux send-keys -t "$target" v
wait_for 'pub fn greeting()'

tmux send-keys -t "$target" Tab
wait_for 'export const greeting'
capture JavaScript

tmux send-keys -t "$target" Tab
wait_for 'def greeting()'
capture Python

tmux send-keys -t "$target" Tab
wait_for 'func greeting() string'
capture Go

tmux send-keys -t "$target" Tab
wait_for 'const char *greeting(void)'
capture C

tmux send-keys -t "$target" q
wait_for_exit
