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

# Export the desktop Qt into the current shell. Idempotent: re-sourcing must not
# stack PATH entries, which it otherwise would on every direnv reload.
qt_env_activate() {
    local prefix
    prefix="$(qt_prefix_for LINUX)" || return 1

    # Strip any previously-added prefix before re-adding, so this is safe to run
    # repeatedly.
    if [ -n "${QT_PREFIX:-}" ]; then
        PATH="$(printf '%s' "$PATH" | sed -e "s#${QT_PREFIX}/bin:##g")"
        if [ -n "${LD_LIBRARY_PATH:-}" ]; then
            LD_LIBRARY_PATH="$(printf '%s' "$LD_LIBRARY_PATH" | sed -e "s#${QT_PREFIX}/lib:##g")"
        fi
    fi

    QT_PREFIX="$prefix"
    QMAKE="$prefix/bin/qmake6"
    PATH="$prefix/bin:$PATH"
    LD_LIBRARY_PATH="$prefix/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

    # Exported so nothing downstream has to hardcode the Android version either.
    # This is the version only -- not a kit path, which is per-ABI.
    QT_ANDROID_VERSION="${QT_ANDROID_VERSION:-$(qt_version_for ANDROID)}"

    export QT_PREFIX QMAKE PATH LD_LIBRARY_PATH QT_ANDROID_VERSION
}

# Sourcing with no arguments activates the desktop environment. Build scripts
# that only want the lookup helpers can set QT_ENV_NO_ACTIVATE=1 first.
if [ -z "${QT_ENV_NO_ACTIVATE:-}" ]; then
    qt_env_activate
fi
