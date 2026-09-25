#!/bin/sh
set -eu

state="$1"
lock="$state/lock"

acquire_lock() {
    while ! mkdir "$lock" 2>/dev/null; do
        sleep 0.01
    done
}

acquire_lock
current=0
if [ -f "$state/current" ]; then
    current=$(cat "$state/current")
fi
current=$((current + 1))
printf '%s\n' "$current" > "$state/current"
peak=0
if [ -f "$state/peak" ]; then
    peak=$(cat "$state/peak")
fi
if [ "$current" -gt "$peak" ]; then
    printf '%s\n' "$current" > "$state/peak"
fi
rmdir "$lock"

sleep 1

acquire_lock
current=$(cat "$state/current")
printf '%s\n' "$((current - 1))" > "$state/current"
printf '%s\n' "${_XYLEM_INPUT}" >> "$state/completed"
rmdir "$lock"
