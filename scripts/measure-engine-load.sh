#!/usr/bin/env bash
# Measure cold `engine.load()` time: the QML engine construction bracketed by the
# two STARTUP-TRACE lines in cpp/sutta_search_window.cpp.
#
# This is the metric the AOT qmlcachegen question turns on (PRD
# tasks/2026-08-16-193200-prd---minsdk-28-and-aot-qml-cache.md, FR-10/FR-17):
# if compiled QML units are actually loaded from the cache, this is where the
# saving shows up. It measures QML engine load ONLY -- not total app startup,
# which is dominated by other costs (see docs/startup-sequence-and-caches.md
# section 6, where a 9 s "QML" stall turned out to be a missing DB index).
#
# Existing to be run twice -- once before the layout move and once after, with
# the same N -- so the two columns are comparable. It launches the real GUI N
# times and kills each run once the trace line lands.
#
# Usage:
#   scripts/measure-engine-load.sh [-n N] [-l LABEL]
#
#   -n N       number of runs (default 7)
#   -l LABEL   tag for the output file under the results dir (default "run")
#
# Output: every run's delta plus the median, and a copy in
# scripts/../build/engine-load-<label>.txt for the record.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$script_dir")"

runs=7
label="run"

while [ $# -gt 0 ]; do
    case "$1" in
        -n) runs="${2:?-n needs a value}"; shift 2 ;;
        -l) label="${2:?-l needs a value}"; shift 2 ;;
        -h|--help) sed -n '2,26p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "measure-engine-load: unknown argument: $1" >&2; exit 2 ;;
    esac
done

app="$root/build/simsapadhammareader/simsapadhammareader"

# Set explicitly and exported, never left to chance: with SIMSAPA_DIR unset the
# app resolves its own data directory, and which one it picks is not obvious
# from here -- so a measurement could silently be taken against a different
# data set (different session to restore = different number of windows loaded =
# a different engine.load()). Reported below so the log says what was measured.
simsapa_dir="${SIMSAPA_DIR:-/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa}"
export SIMSAPA_DIR="$simsapa_dir"
log="$simsapa_dir/log.txt"

[ -x "$app" ] || { echo "Not built: $app" >&2; exit 1; }
[ -d "$simsapa_dir" ] || { echo "No SIMSAPA_DIR: $simsapa_dir" >&2; exit 1; }

# Timestamps look like: [2026-08-26 16:43:08.343Z] INFO: ...
# GNU date parses that once the brackets and the trailing Z are peeled off.
to_epoch_ms() {
    local stamp="$1"
    date -u -d "${stamp%Z} UTC" +%s%3N 2>/dev/null
}

# Kill the whole process group: a WebEngineView spawns Chromium helper
# processes, and killing only the parent leaves them behind. This is the reason
# CLAUDE.md tells agents not to launch the GUI ad hoc.
stop_run() {
    local pgid="$1"
    kill -TERM "-$pgid" 2>/dev/null
    for _ in $(seq 1 50); do
        kill -0 "-$pgid" 2>/dev/null || return 0
        sleep 0.1
    done
    kill -KILL "-$pgid" 2>/dev/null
    sleep 0.3
}

deltas=()
echo "engine.load() -- $runs runs, label '$label'"
echo "app         : $app"
echo "SIMSAPA_DIR : $simsapa_dir"
echo

for i in $(seq 1 "$runs"); do
    # The app TRUNCATES log.txt at every startup -- the file always holds
    # exactly one run, beginning at "gui::start()". So byte offsets into the
    # previous run's file are meaningless: after the truncation the file is
    # shorter than the recorded offset and reads back empty.
    #
    # Truncate it here instead, before the launch. That destroys nothing the app
    # would not have destroyed a moment later, and it removes the race where the
    # PREVIOUS run's "engine.load() end" is still in the file and matches
    # instantly, before the new process has even truncated it.
    : > "$log"

    setsid "$app" >/dev/null 2>&1 &
    pid=$!
    pgid="$(ps -o pgid= -p "$pid" 2>/dev/null | tr -d ' ')"
    pgid="${pgid:-$pid}"

    # Wait for this run's "engine.load() end" to appear.
    got=""
    for _ in $(seq 1 600); do
        if grep -q "STARTUP-TRACE: engine.load() end" "$log" 2>/dev/null; then
            got=1
            break
        fi
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.1
    done

    # Let the run reach a consistent point before killing it.
    #
    # The measured interval is already complete and written by now -- it brackets
    # the SYNCHRONOUS QQmlApplicationEngine construction, and the WebEngineView
    # loaders are asynchronous and cannot even start until app.exec()
    # (docs/startup-sequence-and-caches.md section 6). So this settle does not
    # change the number.
    #
    # It is here because killing the instant the trace lands stops the app
    # mid-initialisation, which was visible as the HTML reader panel sometimes
    # showing grey (Chromium not yet painted) and sometimes the normal yellow.
    # That made successive runs materially different from one another, which is
    # the one thing a before/after comparison cannot afford.
    for _ in $(seq 1 100); do
        grep -q "INFO: app.exec()" "$log" 2>/dev/null && break
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.1
    done
    sleep 1.0

    # Take this run's FIRST start/end pair -- that is window_0's initial load.
    # Session restore can create further windows, each logging its own pair;
    # counting them makes a changed window count visible instead of silently
    # shifting the number being compared.
    chunk="$(cat "$log" 2>/dev/null)"
    stop_run "$pgid"

    if [ -z "$got" ]; then
        echo "  run $i: FAILED -- no trace line (app exited early?)"
        continue
    fi

    start_stamp="$(printf '%s\n' "$chunk" | grep -m1 -F "STARTUP-TRACE: engine.load() start" | sed -n 's/^\[\([^]]*\)\].*/\1/p')"
    end_stamp="$(printf '%s\n' "$chunk" | grep -m1 -F "STARTUP-TRACE: engine.load() end" | sed -n 's/^\[\([^]]*\)\].*/\1/p')"
    pairs="$(printf '%s\n' "$chunk" | grep -c -F "STARTUP-TRACE: engine.load() start")"

    s_ms="$(to_epoch_ms "$start_stamp")"
    e_ms="$(to_epoch_ms "$end_stamp")"

    if [ -z "$s_ms" ] || [ -z "$e_ms" ]; then
        echo "  run $i: FAILED -- unparseable timestamps ('$start_stamp' / '$end_stamp')"
        continue
    fi

    d=$((e_ms - s_ms))
    deltas+=("$d")
    printf '  run %-2s %6s ms   (%s window load(s) this run)\n' "$i" "$d" "$pairs"
done

echo
if [ "${#deltas[@]}" -eq 0 ]; then
    echo "No successful runs."
    exit 1
fi

sorted="$(printf '%s\n' "${deltas[@]}" | sort -n)"
count="${#deltas[@]}"
median="$(printf '%s\n' "$sorted" | awk -v n="$count" 'NR==int((n+1)/2){print; exit}')"
min="$(printf '%s\n' "$sorted" | head -n1)"
max="$(printf '%s\n' "$sorted" | tail -n1)"

echo "runs   : $count of $runs"
echo "all    : $(printf '%s ' "${deltas[@]}")"
echo "min    : $min ms"
echo "median : $median ms"
echo "max    : $max ms"

out="$root/build/engine-load-$label.txt"
{
    echo "label  : $label"
    echo "date   : $(date -Is)"
    echo "app    : $app"
    echo "runs   : $count of $runs"
    echo "all    : $(printf '%s ' "${deltas[@]}")"
    echo "min    : $min ms"
    echo "median : $median ms"
    echo "max    : $max ms"
} > "$out"
echo
echo "recorded: $out"

# A stray process here means the cleanup above failed and the next measurement
# would be racing it.
#
# The bracket in '[s]imsapa...' is load-bearing: a plain -f pattern also matches
# the command line of the process doing the matching, so `pkill -f
# simsapadhammareader` from a shell whose own argv contains that string kills
# the shell. (Measured the hard way -- it terminated a build with exit 144.)
stray="$(pgrep -c -f '[s]imsapadhammareader' 2>/dev/null || true)"
if [ "${stray:-0}" -gt 0 ]; then
    echo
    echo "WARNING: $stray simsapadhammareader process(es) still running -- clean up before re-measuring:"
    echo "  pkill -f '[s]imsapadhammareader'"
fi
