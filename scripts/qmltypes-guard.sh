#!/usr/bin/env bash
#
# Run a build, then make sure the QML AOT code it generated was compiled
# against the bridge API the binary actually links.
#
# `qmlcachegen` bakes a hard-coded QMetaObject method index into every AOT
# binding that calls a bridge singleton, and it takes that index from
# `plugin.qmltypes` -- which `qmltyperegistrar` writes LATER IN THE SAME BUILD.
# So a build that changes the bridge API emits AOT code indexed against the
# previous build's metaobject, and the binary segfaults in
# QMetaObject::methodOffset() the first time such a binding evaluates.
#
# The fix is to notice that `plugin.qmltypes` moved and build once more, so the
# second pass reads the current file. See docs/qmltypes-aot-staleness.md for
# the measured build-step ordering and why a clean build is NOT the answer
# (it costs ~30% of the AOT lookup sites instead).
#
# Usage:
#   scripts/qmltypes-guard.sh <build-dir> <command> [args...]
#
# <build-dir> is the directory holding the cargo OUT_DIRs to watch; every
# `plugin.qmltypes` under it is hashed as one set, so a multi-ABI Android build
# is covered by a single invocation.

set -euo pipefail

if [ "$#" -lt 2 ]; then
    echo "usage: $0 <build-dir> <command> [args...]" >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
build_dir="$1"
shift

qml_dir="$repo_root/bridges/assets/qml"
stamp_file="$build_dir/.qmltypes-guard-stamp"

# macOS ships `shasum`, not GNU coreutils' `sha256sum`, and `make macos` is a
# supported target -- so pick whichever exists rather than assuming GNU.
sha256_filter() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum
    else
        shasum -a 256
    fi
}

# One hash over every plugin.qmltypes under the build directory, paths
# included -- so a newly added ABI counts as a change rather than hiding
# behind an unchanged sibling.
#
# Deliberately avoids `sort -z` and `xargs -r`, which are GNU extensions BSD
# userland does not share.
hash_qmltypes() {
    if [ ! -d "$build_dir" ]; then
        printf 'no-build-dir' | sha256_filter | cut -d' ' -f1
        return
    fi
    {
        find "$build_dir" -name plugin.qmltypes -type f 2>/dev/null \
            | LC_ALL=C sort \
            | while IFS= read -r f; do
                printf '%s  ' "$f"
                sha256_filter < "$f" | cut -d' ' -f1
            done
    } | sha256_filter | cut -d' ' -f1
}

before="$(hash_qmltypes)"

stamp=""
if [ -f "$stamp_file" ]; then
    stamp="$(cat "$stamp_file")"
fi

"$@"

after="$(hash_qmltypes)"

# Three ways the AOT code can be stale, and only the first is the obvious one.
#
#  1. The API changed during this build: qmlcachegen read the pre-build file.
#  2. No stamp: this build directory has never converged under the guard --
#     a clean tree (where qmlcachegen had no qmltypes at all and silently
#     dropped the bridge bindings from the AOT path), or one built by a bare
#     `cmake --build` that bypassed the guard.
#  3. The stamp disagrees with the current file: a previous guarded build did
#     not converge. This is the case that a before/after comparison alone
#     misses -- if a build fails AFTER the cargo build script ran, the stale
#     AOT code is already on disk, and the next build may legitimately skip
#     the build script (cargo saw no source change), leaving before == after
#     over stale artifacts.
needs_retry=""
if [ "$before" != "$after" ]; then
    needs_retry="the bridge API changed during this build"
elif [ -z "$stamp" ]; then
    needs_retry="this build directory has not converged under the guard before"
elif [ "$stamp" != "$after" ]; then
    needs_retry="the previous build in this directory did not converge"
fi

if [ -z "$needs_retry" ]; then
    printf '%s\n' "$after" > "$stamp_file"
    exit 0
fi

echo ""
echo "qmltypes-guard: $needs_retry,"
echo "qmltypes-guard: so the QML AOT code just generated may be indexed against"
echo "qmltypes-guard: a different bridge API. Rebuilding once to regenerate it."
echo ""

# Cargo decides for itself whether to re-run the build script, and nothing in
# the source tree changed between the two passes -- so without a trigger the
# second pass is a no-op and the stale AOT code survives. Touching the QML tree
# is the cheap trigger: it re-runs the build script (which regenerates the
# WHOLE QML tree, not only the touched file) without forcing a Rust recompile.
# Contents are untouched; only mtimes move.
#
# Verified rather than assumed: if the tree ever moves, a touch that matches
# nothing would make the retry a silent no-op and the guard would then report
# success over stale code.
if [ ! -d "$qml_dir" ]; then
    echo "qmltypes-guard: ERROR -- $qml_dir does not exist, so the retry has no" >&2
    echo "qmltypes-guard: trigger. Update this script to the QML tree's new path." >&2
    exit 1
fi

qml_count="$(find "$qml_dir" -name '*.qml' -type f | wc -l | tr -d ' ')"
if [ "$qml_count" -eq 0 ]; then
    echo "qmltypes-guard: ERROR -- no .qml files under $qml_dir, so the retry" >&2
    echo "qmltypes-guard: would be a silent no-op over stale AOT code." >&2
    exit 1
fi

find "$qml_dir" -name '*.qml' -type f -exec touch {} +

"$@"

final="$(hash_qmltypes)"

if [ "$final" != "$after" ]; then
    echo "" >&2
    echo "qmltypes-guard: ERROR -- plugin.qmltypes changed again on the retry," >&2
    echo "qmltypes-guard: so the AOT code may still be stale. Build again and, if" >&2
    echo "qmltypes-guard: this repeats, see docs/qmltypes-aot-staleness.md." >&2
    exit 1
fi

printf '%s\n' "$final" > "$stamp_file"
echo "qmltypes-guard: AOT code regenerated against the current bridge API."
