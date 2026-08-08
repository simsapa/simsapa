#!/usr/bin/env bash
# Drift check for the ONE place the Qt version is necessarily duplicated.
#
# CMakeLists.txt's QT_LINUX is the single source. .claude/settings.json cannot
# run a script to derive it, so it carries literal paths. This check fails if
# the two disagree -- otherwise a QT_LINUX bump silently leaves agent shells
# pointed at the old, possibly uninstalled, kit.
#
# Run via `make qt-env-check`. See docs/qt-kit-selection.md.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$script_dir")"

QT_ENV_NO_ACTIVATE=1 source "$script_dir/qt-env.sh"

expected_version="$(qt_version_for LINUX)"
expected_prefix="$(qt_prefix_for LINUX)"
settings="$root/.claude/settings.json"

status=0

if [ ! -f "$settings" ]; then
    echo "qt-env-check: $settings not found (skipping agent-env check)"
else
    for var in QT_PREFIX QMAKE; do
        actual="$(sed -n "s/.*\"$var\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" "$settings" | head -n1)"
        if [ -z "$actual" ]; then
            echo "qt-env-check: FAIL - $var not set in $settings"
            status=1
            continue
        fi
        case "$actual" in
            "$expected_prefix"*) ;;
            *)
                echo "qt-env-check: FAIL - $var is '$actual'"
                echo "              but CMakeLists.txt QT_LINUX=$expected_version implies '$expected_prefix'"
                echo "              Update .claude/settings.json to match."
                status=1
                ;;
        esac
    done
fi

if [ "$status" -eq 0 ]; then
    echo "qt-env-check: OK - Qt $expected_version at $expected_prefix; .claude/settings.json agrees"
fi

exit "$status"
