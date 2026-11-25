#!/usr/bin/env bash
# Set up the qtbase source work tree and apply Simsapa's Qt patches.
#
#   scripts/qt-patched-kit/prepare-src.sh [--with-debug]
#
# First run: copies pristine ~/Qt/$QT_VERSION/Src/qtbase to
# $WORK_ROOT/qtbase-src, makes it a git repo with one commit, tagged
# pristine-$QT_VERSION.
# Every run: resets branch `patched` to that tag and `git am`s
# android/qt-patches/*.patch (plus android/qt-patches/debug/*.patch with
# --with-debug).
#
# Refuses to run if the work tree has uncommitted changes. Commits made on
# `patched` by hand and not exported to android/qt-patches/ are dropped from the
# branch (they stay reachable through `git reflog`).
# Never writes under ~/Qt. See docs/qt-android-patched-kit.md.
set -euo pipefail
source "$(dirname "$0")/env.sh"

WITH_DEBUG=0
case "${1:-}" in
    --with-debug) WITH_DEBUG=1 ;;
    "") ;;
    *) echo "usage: $0 [--with-debug]" >&2; exit 2 ;;
esac

PRISTINE="$QT_STOCK_ROOT/Src/qtbase"
SRC="$WORK_ROOT/qtbase-src"
TAG="pristine-$QT_VERSION"
[ -f "$PRISTINE/CMakeLists.txt" ] || { echo "error: no pristine qtbase at $PRISTINE" >&2; exit 1; }

# Commit identity for the work tree: the same as Simsapa's.
GIT_NAME="$(git -C "$REPO_ROOT" config user.name || echo "qt-thai-fix")"
GIT_EMAIL="$(git -C "$REPO_ROOT" config user.email || echo "qt-thai-fix@localhost")"
g() { git -C "$SRC" -c user.name="$GIT_NAME" -c user.email="$GIT_EMAIL" "$@"; }

if [ ! -d "$SRC/.git" ]; then
    echo "==> Creating work tree $SRC from $PRISTINE"
    mkdir -p "$SRC"
    rsync -a --delete --exclude .git "$PRISTINE/" "$SRC/"
    g init -q -b patched
    g add -A
    g commit -q -m "pristine qtbase $QT_VERSION from ~/Qt/$QT_VERSION/Src"
    g tag "$TAG"
else
    if [ -d "$SRC/.git/rebase-apply" ]; then
        echo "==> Aborting an unfinished git am in $SRC"
        g am --abort
    fi
    if [ -n "$(g status --porcelain)" ]; then
        echo "error: $SRC has uncommitted changes; commit or discard them first:" >&2
        g status --short >&2
        exit 1
    fi
fi

g checkout -q -B patched "$TAG"

patches=("$QT_PATCHES_DIR"/*.patch)
[ "$WITH_DEBUG" = 1 ] && patches+=("$QT_PATCHES_DIR"/debug/*.patch)
applied=0
for p in "${patches[@]}"; do
    [ -f "$p" ] || continue          # unmatched glob
    echo "==> git am $(basename "$p")"
    g am -q --keep-cr --committer-date-is-author-date "$p"
    applied=$((applied + 1))
done

echo "==> $SRC on branch patched, $applied patch(es) on top of $TAG:"
g log --oneline "$TAG"..HEAD
