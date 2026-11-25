#!/usr/bin/env bash
# Build the patched Qt for Android kit: qtbase $QT_VERSION +
# android/qt-patches/, only the jar and/or platform plugin, swapped into a
# copy of the stock kit, for each ABI in $KIT_ABIS (or only KIT_ABI).
# `make android-qt-kit` runs it. See docs/qt-android-patched-kit.md.
#
#   [KIT_ABI=arm64-v8a|x86_64|armeabi-v7a] scripts/qt-patched-kit/build-kit.sh [--with-debug] [--jar-only]
#
#   --with-debug  also apply android/qt-patches/debug/*.patch (instrumentation;
#                 never for a build that is distributed)
#   --jar-only    build only the Qt6Android jar; fails if the patches touch
#                 the C++ plugin
#   --configure-only  stop after configuring (to check the feature set and
#                 the target names before the long build)
#
# Environment: KIT_ABI (default: all of $KIT_ABIS, one after the other),
# JOBS (default 4), MEMORY_MAX (default 12G). The ABIs share the source tree
# and the kit root, and each has its own build dir. The first build of an ABI
# compiles Core and Gui (about 2 minutes at -j4); a rebuild takes seconds.
#
# Result, at $PATCHED_KIT_ROOT:
#   android_<abi dir>/          copy of the stock kit for that ABI, patched
#                               files swapped in (android_arm64_v8a,
#                               android_x86_64, android_armv7)
#   gcc_64 -> $QT_STOCK_ROOT/gcc_64
#   PATCHED-KIT-MANIFEST-<abi>.txt  patches applied, and per replaced file:
#                               <relative path> <stock sha256> <patched sha256>
#
# Safe to re-run: files replaced by an earlier run are first restored from
# the stock kit. Never writes under ~/Qt.
set -euo pipefail
source "$(dirname "$0")/env.sh"

WITH_DEBUG=0; JAR_ONLY=0; CONFIGURE_ONLY=0
for a in "$@"; do
    case "$a" in
        --with-debug) WITH_DEBUG=1 ;;
        --jar-only) JAR_ONLY=1 ;;
        --configure-only) CONFIGURE_ONLY=1 ;;
        *) echo "usage: $0 [--with-debug] [--jar-only] [--configure-only]" >&2; exit 2 ;;
    esac
done

# No KIT_ABI: build every ABI, each in its own run of this script.
if [ -z "${KIT_ABI:-}" ]; then
    for abi in $KIT_ABIS; do
        echo "######## $abi"
        KIT_ABI="$abi" "$0" "$@"
    done
    exit 0
fi

JOBS="${JOBS:-4}"
MEMORY_MAX="${MEMORY_MAX:-12G}"
KIT_DIR="$(kit_dir_for_abi "$KIT_ABI")"
SRC="$WORK_ROOT/qtbase-src"
# arm64 keeps its original build dir name (android_arm64_v8a -> arm64).
BUILD="$WORK_ROOT/qtbase-build-android-${KIT_DIR#android_}"
[ "$KIT_ABI" = arm64-v8a ] && BUILD="$WORK_ROOT/qtbase-build-android-arm64"
STOCK_KIT="$QT_STOCK_ROOT/$KIT_DIR"
KIT="$PATCHED_KIT_ROOT/$KIT_DIR"
MANIFEST="$(manifest_for_abi "$KIT_ABI")"
[ -d "$STOCK_KIT" ] || { echo "error: no stock kit for $KIT_ABI at $STOCK_KIT" >&2; exit 1; }
LLVM_STRIP="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip"

# Files in the kit that this script knows how to rebuild, and where the
# qtbase build dir puts them.
JAR_REL="jar/Qt6Android.jar"
PLUGIN_REL="$(plugin_rel_for_abi "$KIT_ABI")"
JAR_TARGET="Qt6Android"
PLUGIN_TARGET="QAndroidIntegrationPlugin"

# --- (a) heavy-job pre-checks ----------------------------------------------
echo "==> Pre-checks (ABI $KIT_ABI, kit $KIT)"
ls_lines="$(ps -eo pid,ppid,rss,args | grep -E '[r]ust-analyzer|[c]langd|[t]ypescript-language-server|[p]yright|[j]dtls' || true)"
if [ -n "$ls_lines" ]; then
    echo "  Language servers running:"
    echo "$ls_lines" | sed 's/^/    /'
    # A language server whose parent is an editor (not this agent) would sit
    # beside the build in the terminal's scope, uncapped. Only clangd and
    # jdtls would react to the qtbase sources; stop if one runs.
    while read -r pid ppid _ args; do
        parent="$(ps -o args= -p "$ppid" 2>/dev/null || true)"
        case "$args" in
            *clangd*|*jdtls*)
                echo "error: $args (pid $pid, parent: $parent) would index qtbase beside the build." >&2
                echo "       Close it (or its editor) first." >&2
                exit 1 ;;
        esac
    done <<< "$ls_lines"
fi
free -h
mkdir -p "$WORK_ROOT"
df -h "$WORK_ROOT" | tail -1
echo "  Build cap: MemoryMax=$MEMORY_MAX MemorySwapMax=2G, -j$JOBS"

# --- (b) source tree --------------------------------------------------------
if [ "$WITH_DEBUG" = 1 ]; then
    "$REPO_ROOT/scripts/qt-patched-kit/prepare-src.sh" --with-debug
else
    "$REPO_ROOT/scripts/qt-patched-kit/prepare-src.sh"
fi

changed="$(git -C "$SRC" diff --name-only "pristine-$QT_VERSION"..HEAD)"
NEED_JAR=0; NEED_PLUGIN=0
while read -r f; do
    [ -n "$f" ] || continue
    case "$f" in
        src/android/jar/*) NEED_JAR=1 ;;
        src/plugins/platforms/android/*) NEED_PLUGIN=1 ;;
        *) echo "error: the patches change $f, which is outside the jar and the" >&2
           echo "       qtforandroid plugin; this script does not rebuild it." >&2
           echo "       Extend it (and re-check overlay ABI compatibility) first." >&2
           exit 1 ;;
    esac
done <<< "$changed"
if [ "$JAR_ONLY" = 1 ] && [ "$NEED_PLUGIN" = 1 ]; then
    echo "error: --jar-only, but the patches change the C++ plugin" >&2; exit 1
fi
if [ "$NEED_JAR" = 0 ] && [ "$NEED_PLUGIN" = 0 ]; then
    echo "error: no patches applied; nothing to build" >&2; exit 1
fi

# --- (c) configure ----------------------------------------------------------
# Matches the stock kit's config_qtbase.opt (-release, -nomake examples,
# -android-abis <abi>) except for:
#  - no -force-debug-info: the installed plugin is stripped anyway, and the
#    built one is stripped below before it goes into the kit;
#  - no OpenSSL: the stock kit loads a prebuilt OpenSSL at runtime, which
#    only matters to QtNetwork, and QtNetwork is not part of the overlay;
#  - -nomake tests (the stock build had QT_BUILD_TESTS=OFF as well).
# The stock armeabi-v7a kit was built on Windows with -no-feature-pkg-config;
# pkg-config is off for an Android build here anyway (qconfig.pri says
# no-pkg-config for all three ABIs).
# The -prefix is never installed to, so all ABIs share it (and the arm64
# configure line, which decides whether to reconfigure, stays the same).
CONFIGURE=(
    "$SRC/configure"
    -platform android-clang
    -prefix "$WORK_ROOT/qtbase-install"
    -android-ndk "$ANDROID_NDK_ROOT"
    -android-sdk "$ANDROID_SDK_ROOT"
    -android-abis "$KIT_ABI"
    -qt-host-path "$QT_STOCK_ROOT/gcc_64"
    -release
    -no-openssl
    -nomake tests -nomake examples
    -- -G Ninja
)
configure_line="${CONFIGURE[*]}"
if [ ! -f "$BUILD/build.ninja" ] || [ "$(cat "$BUILD/.configure-line" 2>/dev/null)" != "$configure_line" ]; then
    echo "==> Configuring in $BUILD"
    rm -rf "$BUILD"
    mkdir -p "$BUILD"
    ( cd "$BUILD" && "${CONFIGURE[@]}" ) 2>&1 | tee "$BUILD.configure.log"
    printf '%s' "$configure_line" >| "$BUILD/.configure-line"
else
    echo "==> Configure line unchanged; reusing $BUILD"
fi

if [ "$CONFIGURE_ONLY" = 1 ]; then
    echo "==> --configure-only: stopping before the build"
    exit 0
fi

# --- (d) build only what the patches need -----------------------------------
targets=()
[ "$NEED_JAR" = 1 ] && targets+=("$JAR_TARGET")
[ "$NEED_PLUGIN" = 1 ] && targets+=("$PLUGIN_TARGET")
echo "==> Building ${targets[*]} (MemoryMax=$MEMORY_MAX, -j$JOBS)"
start=$(date +%s)
systemd-run --user --scope --quiet -p MemoryMax="$MEMORY_MAX" -p MemorySwapMax=2G -- \
    ninja -C "$BUILD" -j"$JOBS" "${targets[@]}"
echo "==> Build took $(( ($(date +%s) - start) / 60 )) min"

built_jar="$(find "$BUILD" -path '*/jar/*' -name 'Qt6Android.jar' | head -1)"
built_plugin="$(find "$BUILD" -name "$(basename "$PLUGIN_REL")" | head -1)"

# --- (e) the kit copy -------------------------------------------------------
if [ ! -d "$KIT" ]; then
    echo "==> Copying the stock kit to $KIT"
    mkdir -p "$PATCHED_KIT_ROOT"
    cp -a "$STOCK_KIT" "$PATCHED_KIT_ROOT/"
else
    echo "==> Restoring previously replaced files from the stock kit"
    for rel in "$JAR_REL" "$PLUGIN_REL"; do
        cp -a "$STOCK_KIT/$rel" "$KIT/$rel"
    done
fi
ln -sfn "$QT_STOCK_ROOT/gcc_64" "$PATCHED_KIT_ROOT/gcc_64"

# --- (f) swap in the patched files ------------------------------------------
replaced=()
if [ "$NEED_JAR" = 1 ]; then
    [ -f "$built_jar" ] || { echo "error: built Qt6Android.jar not found under $BUILD" >&2; exit 1; }
    cp "$built_jar" "$KIT/$JAR_REL"
    replaced+=("$JAR_REL")
fi
if [ "$NEED_PLUGIN" = 1 ]; then
    [ -f "$built_plugin" ] || { echo "error: built plugin not found under $BUILD" >&2; exit 1; }
    # The stock plugin is installed stripped; match it, and its file mode
    # (the stock armv7 plugin is 644, the others 755).
    "$LLVM_STRIP" --strip-unneeded -o "$KIT/$PLUGIN_REL" "$built_plugin"
    chmod --reference="$STOCK_KIT/$PLUGIN_REL" "$KIT/$PLUGIN_REL"
    replaced+=("$PLUGIN_REL")
fi

# --- (g) relocate -----------------------------------------------------------
KIT_ABI="$KIT_ABI" "$REPO_ROOT/scripts/qt-patched-kit/relocate-kit.sh" "$PATCHED_KIT_ROOT"

# --- (h) manifest -----------------------------------------------------------
{
    echo "# Patched Qt $QT_VERSION overlay kit ($KIT_DIR, ABI $KIT_ABI)"
    echo "# built:     $(date -Iseconds)"
    echo "# stock kit: $STOCK_KIT"
    echo "# configure: $configure_line"
    echo "# patches (git log --oneline pristine-$QT_VERSION..HEAD in $SRC):"
    git -C "$SRC" log --oneline "pristine-$QT_VERSION"..HEAD | sed 's/^/#   /'
    echo "# <relative path> <stock sha256> <patched sha256>"
    for rel in "${replaced[@]}"; do
        printf '%s %s %s\n' "$rel" \
            "$(sha256sum < "$STOCK_KIT/$rel" | cut -d' ' -f1)" \
            "$(sha256sum < "$KIT/$rel" | cut -d' ' -f1)"
    done
} >| "$MANIFEST"

echo "==> Done. Manifest:"
cat "$MANIFEST"
