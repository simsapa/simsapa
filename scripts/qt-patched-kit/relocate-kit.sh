#!/usr/bin/env bash
# Make a copied Android kit independent of the stock kit it was copied from.
#
#   [KIT_ABI=arm64-v8a|x86_64|armeabi-v7a] scripts/qt-patched-kit/relocate-kit.sh <kit-root>
#
# Called by build-kit.sh; there is normally no reason to run it by hand.
#
# <kit-root> has the layout <kit-root>/{android_<abi dir>,gcc_64}, where
# android_<abi dir> is a copy of $QT_STOCK_ROOT/android_<abi dir> for KIT_ABI
# (default arm64-v8a) and gcc_64 is the host kit (normally a symlink to
# $QT_STOCK_ROOT/gcc_64).
#
# The installed kit is almost fully relocatable: bin/qt-cmake and
# qt.toolchain.cmake find their paths relative to themselves, and
# bin/target_qt.conf is relative. The one file with an absolute path to the
# Android kit itself is lib/cmake/Qt6BuildInternals/QtBuildInternalsExtra.cmake
# (only used when building Qt modules); it is rewritten here. The absolute
# $QT_STOCK_ROOT/gcc_64 paths in bin/qmake*, bin/qtpaths* and
# Qt6Dependencies.cmake are left alone: they name the same host kit.
#
# Then it checks: no text file in the copy still names the stock Android kit,
# and qmake -query (which cxx-qt uses) resolves every install path inside
# <kit-root>. Exits non-zero if either check fails.
set -euo pipefail
source "$(dirname "$0")/env.sh"

[ $# -eq 1 ] || { echo "usage: $0 <kit-root>" >&2; exit 2; }
KIT_ROOT="$(cd "$1" && pwd)"
KIT_ABI="${KIT_ABI:-arm64-v8a}"
KIT_DIR="$(kit_dir_for_abi "$KIT_ABI")"
KIT="$KIT_ROOT/$KIT_DIR"
STOCK_KIT="$QT_STOCK_ROOT/$KIT_DIR"
[ -d "$KIT" ] || { echo "error: no $KIT" >&2; exit 1; }
[ "$KIT" != "$STOCK_KIT" ] || { echo "error: refusing to touch the stock kit" >&2; exit 1; }

BI="$KIT/lib/cmake/Qt6BuildInternals/QtBuildInternalsExtra.cmake"
echo "==> Rewriting $BI"
sed -i \
    -e "s|\"$STOCK_KIT\"|\"$KIT\"|g" \
    -e "s|set(qtbi_orig_prefix \"$(dirname "$QT_STOCK_ROOT")\")|set(qtbi_orig_prefix \"$KIT_ROOT\")|" \
    "$BI"

fail=0

echo "==> Checking for leftover paths to $STOCK_KIT"
left="$(grep -rlIF "$STOCK_KIT" "$KIT" || true)"
if [ -n "$left" ]; then
    echo "FAIL: these files still name the stock Android kit:"
    echo "$left" | sed 's/^/  /'
    fail=1
else
    echo "  ok: none"
fi

echo "==> Checking qmake -query"
query="$("$KIT/bin/qmake" -query)"
# Docs and Examples point outside any kit (../../Docs, ../../Examples) in the
# stock kit too; QT_INSTALL_CONFIGURATION is empty. They are not used by builds.
bad="$(echo "$query" | grep -E '^(QT_INSTALL_[A-Z_]+|QT_HOST_DATA):' \
        | grep -vE '^QT_INSTALL_(DOCS|EXAMPLES|DEMOS|CONFIGURATION):' \
        | grep -vF ":$KIT_ROOT/" | grep -vE ":$KIT_ROOT\$" || true)"
echo "$query" | grep -E '^(QT_INSTALL_PREFIX|QT_INSTALL_PLUGINS|QT_HOST_DATA|QT_HOST_PREFIX|QT_HOST_BINS):' | sed 's/^/  /'
if [ -n "$bad" ]; then
    echo "FAIL: these qmake -query paths are outside $KIT_ROOT:"
    echo "$bad" | sed 's/^/  /'
    fail=1
else
    echo "  ok: every QT_INSTALL_* (except docs/examples) and QT_HOST_DATA is under $KIT_ROOT"
fi

exit $fail
