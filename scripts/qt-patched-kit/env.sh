# Shared settings for the patched Qt for Android kit scripts in this folder.
# Source it, don't run it:
#   source "$(dirname "$0")/env.sh"
# Every value can be overridden from the environment.
#
# See docs/qt-android-patched-kit.md.

# The Simsapa repo root, whichever script sourced us.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)"

# The Qt version comes from CMakeLists.txt's QT_ANDROID, never a second copy.
if [ -z "${QT_VERSION:-}" ]; then
    QT_ENV_NO_ACTIVATE=1 . "$REPO_ROOT/scripts/qt-env.sh"
    QT_VERSION="$(qt_version_for ANDROID)"
fi
# The installed (stock) Qt, from the Qt online installer: its Android kits
# are copied, its gcc_64 host kit is used as is, and its Src/qtbase is the
# pristine source the patches apply to. Never written to.
QT_STOCK_ROOT="${QT_STOCK_ROOT:-$HOME/Qt/$QT_VERSION}"

# The patches carried by Simsapa (git format-patch files, applied in order).
QT_PATCHES_DIR="${QT_PATCHES_DIR:-$REPO_ROOT/android/qt-patches}"

ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}"
# The same NDK as build-android.sh. The stock kits were built with r27c.
ANDROID_NDK_VERSION="${ANDROID_NDK_VERSION:-27.3.13750724}"
ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-$ANDROID_SDK_ROOT/ndk/$ANDROID_NDK_VERSION}"

# dexdump from the newest installed build-tools.
if [ -z "${DEXDUMP:-}" ]; then
    DEXDUMP="$(ls -d "$ANDROID_SDK_ROOT"/build-tools/*/ 2>/dev/null | sort -V | tail -1)dexdump"
fi

# The Qt6Android jar is built with javac; the same JDK 17-21 range as
# build-android.sh (Gradle / AGP 8.x). Only set when not already in range.
if [ -z "${JAVA_HOME:-}" ] || [ ! -x "${JAVA_HOME}/bin/javac" ]; then
    for candidate in /usr/lib/jvm/java-21-openjdk /usr/lib/jvm/java-17-openjdk; do
        if [ -x "$candidate/bin/javac" ]; then JAVA_HOME="$candidate"; break; fi
    done
fi

# Work dirs and the kit, beside the repo (not inside it, and not under ~/Qt).
# The patched kit is the default QT_ANDROID_ROOT of build-android.sh.
PROJECT_PARENT="${PROJECT_PARENT:-$(dirname "$REPO_ROOT")}"
WORK_ROOT="${WORK_ROOT:-$PROJECT_PARENT/qt-work}"
PATCHED_KIT_ROOT="${PATCHED_KIT_ROOT:-$PROJECT_PARENT/qt-$QT_VERSION-thai-fix}"

# The ABIs a patched kit is built for: the ones build-android.sh packages.
# Each ABI has its own kit directory, with the platform plugin named after
# the ABI, all in the same $PATCHED_KIT_ROOT, so Qt's multi-ABI build finds
# the extra ABIs as siblings of the primary kit (`<kit>/../android_<suffix>`).
KIT_ABIS="arm64-v8a x86_64 armeabi-v7a"
kit_dir_for_abi() {     # ABI -> kit directory name under a Qt root
    case "$1" in
        arm64-v8a)   echo android_arm64_v8a ;;
        x86_64)      echo android_x86_64 ;;
        armeabi-v7a) echo android_armv7 ;;
        *) echo "error: unsupported ABI '$1' (supported: $KIT_ABIS)" >&2; return 1 ;;
    esac
}
plugin_rel_for_abi() {  # ABI -> the qtforandroid plugin, relative to the kit
    echo "plugins/platforms/libplugins_platforms_qtforandroid_$1.so"
}
manifest_for_abi() {    # [kit root] ABI -> that ABI's manifest
    if [ $# -eq 2 ]; then echo "$1/PATCHED-KIT-MANIFEST-$2.txt"
    else echo "$PATCHED_KIT_ROOT/PATCHED-KIT-MANIFEST-$1.txt"; fi
}

export QT_VERSION QT_STOCK_ROOT QT_PATCHES_DIR ANDROID_SDK_ROOT \
       ANDROID_NDK_VERSION ANDROID_NDK_ROOT DEXDUMP JAVA_HOME \
       WORK_ROOT PATCHED_KIT_ROOT REPO_ROOT

# A host-Qt LD_LIBRARY_PATH (e.g. from scripts/qt-env.sh in an outer shell)
# makes the Android host tools load the wrong libraries; build-android.sh
# clears it for the same reason (docs/qt-kit-selection.md §8.1).
unset LD_LIBRARY_PATH
