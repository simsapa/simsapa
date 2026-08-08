#!/usr/bin/env bash
# Qt version/kit resolution for this project's shell tooling.
#
# CMakeLists.txt is the SINGLE SOURCE of the per-platform Qt version (the QT_*
# variables at the top of it). This script reads them; it never hardcodes a
# version. Build scripts source it so the version is declared in exactly one
# place -- see docs/qt-kit-selection.md.
#
# TWO SEPARATE ROLES, do not conflate them:
#
#   1. `qt_version_for <platform>` / `qt_prefix_for <platform>` -- pure lookup
#      helpers. Used by build-android.sh, build-appimage.sh, etc.
#
#   2. Sourcing this file with no arguments ALSO exports the DESKTOP Qt
#      (QT_PREFIX / QMAKE / PATH / LD_LIBRARY_PATH) for interactive and agent
#      shells, so a bare `qmake6` is the project's Qt rather than the system
#      one. This is the .envrc / .claude/settings.json convenience layer.
#
# CONVENIENCE ONLY -- NOTHING HERE IS LOAD-BEARING.
# The build must be correct with an empty Qt-related environment: CMakeLists.txt
# resolves its own CMAKE_PREFIX_PATH and asserts the found Qt matches the
# declared version. If deleting this file ever breaks `make build`, that is a
# bug in CMakeLists.txt, not a reason to make this file required.
#
# Desktop is the default here because Android work goes through
# build-android.sh, which derives its own (different) Qt version. There is
# deliberately no single "the Qt for this project".
#
# WHY ONE SHARED FILE RATHER THAN A DUPLICATED sed ONE-LINER
#
# The alternative considered was copying `qt_version_for` into build-android.sh
# and build-appimage.sh with a cross-reference comment. Rejected: the whole
# point of this work is that the Qt version is declared in exactly ONE place,
# and duplicating the READER re-creates the same class of drift one level down
# -- three copies of a sed expression that must all keep matching the same
# CMakeLists.txt syntax. If the `set(QT_LINUX "6.9.3")` line is ever reformatted
# (a comment moved onto it, single quotes, a cache entry), a duplicated reader
# fails in one script and not another, and the two disagree silently. That is
# precisely the failure this file exists to remove.
#
# The cost of sharing is that build scripts must not be ambushed by role 2:
# sourcing this file would otherwise put the DESKTOP kit on PATH, which is
# actively wrong inside build-android.sh. Hence QT_ENV_NO_ACTIVATE=1, which
# build scripts set before sourcing to get the lookup helpers and nothing else:
#
#     QT_ENV_NO_ACTIVATE=1 . "$(dirname "$0")/scripts/qt-env.sh"
#     qt_version="$(qt_version_for ANDROID)"
#
# Sourcing is safe from any working directory -- the repo root is resolved from
# BASH_SOURCE, not from $PWD.

# Resolve the repo root from this script's own location, so sourcing works from
# any working directory.
_qt_env_script_dir="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
QT_PROJECT_ROOT="$(dirname "$_qt_env_script_dir")"
QT_CMAKELISTS="$QT_PROJECT_ROOT/CMakeLists.txt"

# Read one `set(QT_<PLATFORM> "x.y.z")` value out of CMakeLists.txt.
# Platform argument is one of: LINUX MACOS WINDOWS ANDROID IOS
qt_version_for() {
    local platform="${1:?qt_version_for: platform argument required}"
    local version
    version="$(sed -n "s/^[[:space:]]*set(QT_${platform}[[:space:]]*\"\([^\"]*\)\").*/\1/p" \
        "$QT_CMAKELISTS" | head -n1)"
    if [ -z "$version" ]; then
        echo "qt-env.sh: could not read QT_${platform} from $QT_CMAKELISTS" >&2
        return 1
    fi
    printf '%s\n' "$version"
}

# Absolute path to a platform's Qt kit. Desktop Linux only for now; the Android
# kit path depends on the ABI and is build-android.sh's business.
qt_prefix_for() {
    local platform="${1:?qt_prefix_for: platform argument required}"
    local version
    version="$(qt_version_for "$platform")" || return 1
    case "$platform" in
        LINUX)
            if [ -d "$HOME/Qt/$version/gcc_64" ]; then
                printf '%s\n' "$HOME/Qt/$version/gcc_64"
            elif [ -d "/opt/Qt/$version/gcc_64" ]; then
                printf '%s\n' "/opt/Qt/$version/gcc_64"
            else
                echo "qt-env.sh: Qt $version not found under \$HOME/Qt or /opt/Qt" >&2
                return 1
            fi
            ;;
        *)
            echo "qt-env.sh: qt_prefix_for does not handle $platform" >&2
            return 1
            ;;
    esac
}

# Remove every exact occurrence of $2 from the colon-separated list in $1, and
# print what is left.
#
# This replaces a pair of sed substitutions that tried to match "<entry>:" and
# ":<entry>". Textual matching cannot get this right: it needs a separate form
# per position, and the case where the entry is the WHOLE value has neither a
# leading nor a trailing colon, so it matches nothing and survives forever.
# Splitting on ":" and comparing whole entries handles first/middle/last/only
# uniformly. See docs/qt-kit-selection.md §8.1.
#
# Empty entries are dropped rather than preserved. An empty entry in PATH means
# "the current directory", which nothing here wants and which is a hazard in its
# own right.
_qt_env_list_remove() {
    local list="$1" entry="$2" out="" item
    local IFS=:
    for item in $list; do
        [ -z "$item" ] && continue
        [ "$item" = "$entry" ] && continue
        out="${out:+$out:}$item"
    done
    printf '%s' "$out"
}

# Export the desktop Qt into the current shell. Idempotent: re-sourcing must not
# stack PATH entries, which it otherwise would on every direnv reload.
qt_env_activate() {
    local prefix
    prefix="$(qt_prefix_for LINUX)" || return 1

    # Strip any previously-added prefix before re-adding, so this is safe to run
    # repeatedly. Both the OLD prefix (a QT_LINUX bump moves it) and the NEW one
    # (an entry a hand-written export already added) are removed, or activating
    # twice would leave two copies of the kit we are about to prepend.
    if [ -n "${QT_PREFIX:-}" ]; then
        PATH="$(_qt_env_list_remove "$PATH" "$QT_PREFIX/bin")"
        LD_LIBRARY_PATH="$(_qt_env_list_remove "${LD_LIBRARY_PATH:-}" "$QT_PREFIX/lib")"
    fi
    PATH="$(_qt_env_list_remove "$PATH" "$prefix/bin")"
    LD_LIBRARY_PATH="$(_qt_env_list_remove "${LD_LIBRARY_PATH:-}" "$prefix/lib")"

    QT_PREFIX="$prefix"
    QMAKE="$prefix/bin/qmake6"
    PATH="$prefix/bin${PATH:+:$PATH}"
    LD_LIBRARY_PATH="$prefix/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

    # DELIBERATELY NOT EXPORTING QT_ANDROID_VERSION.
    #
    # It used to be exported here "so nothing downstream has to hardcode the
    # Android version". That is now build-android.sh's own job -- it reads
    # QT_ANDROID from CMakeLists.txt directly -- and the export had become
    # actively dangerous, because build-android.sh treats a set
    # QT_ANDROID_VERSION as a DELIBERATE OVERRIDE:
    #
    #   * Every interactive/agent shell that sourced this file carried the
    #     variable, so the "override" branch was taken on every ordinary build
    #     and CMakeLists.txt was never consulted.
    #   * The assignment was `:-` guarded, so re-sourcing (a direnv reload)
    #     did NOT refresh it. A shell opened before a QT_ANDROID bump kept the
    #     old version indefinitely.
    #
    # Combined, once Android and desktop target different Qt versions, that
    # silently builds Android against the DESKTOP version -- a package that
    # looks fine and ships without the Android-only fix the bump exists for.
    # Invisible today only because both versions are still equal.
    #
    # This file is a convenience layer (see the header). A variable exported
    # from it that changes build OUTPUT would make it load-bearing, which is
    # exactly what it must not be. Set QT_ANDROID_VERSION by hand when you
    # genuinely want to override; build-android.sh reports which source it used.
    export QT_PREFIX QMAKE PATH LD_LIBRARY_PATH
}

# Sourcing with no arguments activates the desktop environment. Build scripts
# that only want the lookup helpers can set QT_ENV_NO_ACTIVATE=1 first.
if [ -z "${QT_ENV_NO_ACTIVATE:-}" ]; then
    qt_env_activate
fi
