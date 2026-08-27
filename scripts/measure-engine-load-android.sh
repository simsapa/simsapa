#!/usr/bin/env bash
# The Android twin of measure-engine-load.sh: cold `engine.load()` time on
# device, from the two STARTUP-TRACE lines in cpp/sutta_search_window.cpp.
#
# Why measure Android at all: the device CPU is markedly slower than the
# desktop's (measured ~1.5x on the first sample), so if AOT-compiled QML units
# help anywhere they should help most here -- and a baseline is only useful if
# it is taken BEFORE the change.
#
# Two device-specific facts drive the implementation, both established by
# measurement rather than assumption:
#
#   1. The STARTUP-TRACE lines DO NOT reach logcat. They are written by
#      log_info_c() and land in the app's own log.txt inside its private data
#      directory; a logcat capture filtered on the documented tag set
#      (simsapa/Qt/QtCore/QtQml) contains none of them. So this script reads the
#      app's log.txt over `adb run-as`, not logcat.
#   2. `run-as PKG sh -c '...'` is blocked by SELinux ("Permission denied", and
#      with a relative path it cannot even find the file, because the shell does
#      not inherit the app home as cwd). But `run-as PKG <binary>` works
#      directly -- hence `run-as PKG rm files/log.txt` to clear between runs and
#      `run-as PKG cat files/log.txt` to read.
#
# Requires a debuggable build (the beta debug package). `run-as` refuses on a
# non-debuggable one, which is the whole reason the beta debug variant exists.
#
# Usage:
#   scripts/measure-engine-load-android.sh [-n N] [-l LABEL] [-p PACKAGE]

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$script_dir")"

runs=7
label="android"
pkg="io.github.simsapa.app.beta"

while [ $# -gt 0 ]; do
    case "$1" in
        -n) runs="${2:?-n needs a value}"; shift 2 ;;
        -l) label="${2:?-l needs a value}"; shift 2 ;;
        -p) pkg="${2:?-p needs a value}"; shift 2 ;;
        -h|--help) sed -n '2,30p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "measure-engine-load-android: unknown argument: $1" >&2; exit 2 ;;
    esac
done

command -v adb >/dev/null 2>&1 || { echo "adb not found" >&2; exit 1; }
[ -n "$(adb devices | sed -n '2p')" ] || { echo "no device attached" >&2; exit 1; }

adb shell pm list packages | grep -q "^package:$pkg$" || {
    echo "package not installed: $pkg" >&2; exit 1; }

# run-as only works on a debuggable package; fail with the reason rather than a
# confusing permission error later.
adb shell run-as "$pkg" true >/dev/null 2>&1 || {
    echo "run-as refused for $pkg -- is it the DEBUGGABLE beta build?" >&2
    echo "  make android-beta-debug && make android-beta-debug-install" >&2
    exit 1; }

log_rel="files/log.txt"

to_epoch_ms() {
    local stamp="$1"
    date -u -d "${stamp%Z} UTC" +%s%3N 2>/dev/null
}

deltas=()
echo "engine.load() on device -- $runs runs, label '$label'"
echo "package : $pkg"
echo "device  : $(adb shell getprop ro.product.model 2>/dev/null | tr -d '\r') (Android $(adb shell getprop ro.build.version.release 2>/dev/null | tr -d '\r'), API $(adb shell getprop ro.build.version.sdk 2>/dev/null | tr -d '\r'))"
echo

for i in $(seq 1 "$runs"); do
    adb shell am force-stop "$pkg" >/dev/null 2>&1
    # The app truncates log.txt at startup, but removing it outright makes "the
    # file exists again" an unambiguous signal that THIS run wrote it.
    adb shell run-as "$pkg" rm "$log_rel" >/dev/null 2>&1

    adb shell monkey -p "$pkg" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1

    got=""
    for _ in $(seq 1 120); do
        if adb shell run-as "$pkg" cat "$log_rel" 2>/dev/null | grep -q "engine.load() end"; then
            got=1
            break
        fi
        sleep 0.5
    done

    if [ -z "$got" ]; then
        echo "  run $i: FAILED -- no engine.load() end within timeout"
        adb shell am force-stop "$pkg" >/dev/null 2>&1
        continue
    fi

    chunk="$(adb shell run-as "$pkg" cat "$log_rel" 2>/dev/null | tr -d '\r')"
    adb shell am force-stop "$pkg" >/dev/null 2>&1

    start_stamp="$(printf '%s\n' "$chunk" | grep -m1 -F "engine.load() start" | sed -n 's/^\[\([^]]*\)\].*/\1/p')"
    end_stamp="$(printf '%s\n' "$chunk" | grep -m1 -F "engine.load() end" | sed -n 's/^\[\([^]]*\)\].*/\1/p')"
    pairs="$(printf '%s\n' "$chunk" | grep -c -F "engine.load() start")"

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
mkdir -p "$root/build"
{
    echo "label   : $label"
    echo "date    : $(date -Is)"
    echo "package : $pkg"
    echo "device  : $(adb shell getprop ro.product.model 2>/dev/null | tr -d '\r')"
    echo "runs    : $count of $runs"
    echo "all     : $(printf '%s ' "${deltas[@]}")"
    echo "min     : $min ms"
    echo "median  : $median ms"
    echo "max     : $max ms"
} > "$out"
echo
echo "recorded: $out"
