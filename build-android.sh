#!/usr/bin/env bash
#
# Build a signed multi-ABI Android package (AAB by default, APK optionally).
#
# Multi-ABI is CMake-only in Qt 6 (qmake dropped it). The mechanism: configure
# with the *primary* ABI's qt-cmake, and Qt generates one ExternalProject per
# additional ABI that re-configures this same source tree with that ABI's
# toolchain file, then serialises the androiddeployqt runs and merges the
# results into a single package.
#
# Signing uses androiddeployqt's --sign, which reads the keystore from the
# QT_ANDROID_KEYSTORE_* environment variables. Those come from the gitignored
# android/signing.env (see android/signing.env.example).
#
# See docs/android-multi-abi-and-chromeos.md for the full design.

set -euo pipefail

cd "$(dirname "$0")"

# ---------------------------------------------------------------------------
# Configuration (override via environment or `make android-aab VAR=value`)
# ---------------------------------------------------------------------------

QT_ANDROID_VERSION="${QT_ANDROID_VERSION:-6.9.3}"
QT_ANDROID_ROOT="${QT_ANDROID_ROOT:-$HOME/Qt/$QT_ANDROID_VERSION}"

# The primary ABI supplies qt-cmake and the toolchain the top-level build uses.
ANDROID_PRIMARY_ABI="${ANDROID_PRIMARY_ABI:-arm64-v8a}"

# ABIs to ship. arm64-v8a covers modern phones and ARM Chromebooks; x86_64 is
# what Intel/AMD Chromebooks (ARCVM) need and is the reason a Chromebook user
# saw "not compatible" against an arm64-only bundle; armeabi-v7a covers old
# 32-bit phones and is optional.
#
# Deliberately NOT using QT_ANDROID_BUILD_ALL_ABIS: it would autodetect the
# installed android_x86 kit too, and the 32-bit x86 Rust target
# (i686-linux-android) is not installed. Nothing needs it.
ANDROID_ABIS="${ANDROID_ABIS:-arm64-v8a;x86_64;armeabi-v7a}"

ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}"
# Stay on the Qt 6.9.3-supported NDK (r26b/r27). Do NOT use r28 — at minSdk 27
# its libc++ references pthread_cond_clockwait (bionic API 30+), which breaks
# the cxx C++ build. See docs/pure-rust-audio-backend.md.
ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-$(ls -d "$ANDROID_SDK_ROOT"/ndk/* 2>/dev/null | sort -V | tail -1)}"

ANDROID_BUILD_DIR="${ANDROID_BUILD_DIR:-build/android-multiabi}"
ANDROID_BUILD_TYPE="${ANDROID_BUILD_TYPE:-Release}"

SIGNING_ENV="${SIGNING_ENV:-android/signing.env}"

PACKAGE_TARGET="aab"
DO_CLEAN=0
DO_SIGN=1

# ---------------------------------------------------------------------------
# Arguments
# ---------------------------------------------------------------------------

usage() {
    cat <<'EOF'
Usage: ./build-android.sh [options]

  --aab             Build an Android App Bundle (default; for Google Play)
  --apk             Build an APK instead (for sideloading / local testing)
  --abis "a;b;c"    Override the ABI list (default: arm64-v8a;x86_64;armeabi-v7a)
  --debug           CMAKE_BUILD_TYPE=Debug (implies --no-sign)
  --no-sign         Skip signing; produce an unsigned package
  --clean           Remove the build directory before configuring
  -h, --help        This message

Environment:
  ANDROID_VERSION_CODE   Play requires this to strictly increase per upload
  ANDROID_VERSION_NAME   Human-readable version, e.g. 1.0.0-alpha.3
  ANDROID_ABIS, ANDROID_SDK_ROOT, ANDROID_NDK_ROOT, ANDROID_BUILD_DIR,
  QT_ANDROID_VERSION, QT_ANDROID_ROOT, ANDROID_PRIMARY_ABI

Signing credentials are read from android/signing.env (gitignored); any
QT_ANDROID_KEYSTORE_* already exported in the shell takes precedence.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --aab)   PACKAGE_TARGET="aab" ;;
        --apk)   PACKAGE_TARGET="apk" ;;
        --abis)  ANDROID_ABIS="$2"; shift ;;
        --debug) ANDROID_BUILD_TYPE="Debug"; DO_SIGN=0 ;;
        --no-sign) DO_SIGN=0 ;;
        --clean) DO_CLEAN=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------

die() { echo "ERROR: $*" >&2; exit 1; }

# Qt's kit directory names are not a mechanical transform of the ABI name
# (armeabi-v7a lives in android_armv7), so map them explicitly.
case "$ANDROID_PRIMARY_ABI" in
    arm64-v8a)   QT_CMAKE="$QT_ANDROID_ROOT/android_arm64_v8a/bin/qt-cmake" ;;
    armeabi-v7a) QT_CMAKE="$QT_ANDROID_ROOT/android_armv7/bin/qt-cmake" ;;
    x86_64)      QT_CMAKE="$QT_ANDROID_ROOT/android_x86_64/bin/qt-cmake" ;;
    x86)         QT_CMAKE="$QT_ANDROID_ROOT/android_x86/bin/qt-cmake" ;;
    *) die "Unsupported ANDROID_PRIMARY_ABI: $ANDROID_PRIMARY_ABI" ;;
esac

[ -x "$QT_CMAKE" ] || die "qt-cmake not found at $QT_CMAKE
Install the 'Android for $ANDROID_PRIMARY_ABI' component of Qt $QT_ANDROID_VERSION."

# Every requested ABI needs its own Qt for Android kit installed, or Qt aborts
# the configure with 'Cannot find toolchain files for the manually specified
# Android ABIs'. Check up front so the failure is legible.
missing_kits=""
IFS=';' read -ra _abi_list <<< "$ANDROID_ABIS"
for abi in "${_abi_list[@]}"; do
    case "$abi" in
        arm64-v8a)   kit="android_arm64_v8a" ;;
        armeabi-v7a) kit="android_armv7" ;;
        x86_64)      kit="android_x86_64" ;;
        x86)         kit="android_x86" ;;
        *) die "Unsupported ABI in ANDROID_ABIS: $abi" ;;
    esac
    [ -d "$QT_ANDROID_ROOT/$kit" ] || missing_kits="$missing_kits $abi ($kit)"
done
[ -z "$missing_kits" ] || die "Missing Qt for Android kits for:$missing_kits
Install them with the Qt Maintenance Tool under Qt $QT_ANDROID_VERSION."

# Corrosion (FindRust.cmake) maps CMAKE_ANDROID_ARCH_ABI to a Rust target
# triple. Note armeabi-v7a resolves to armv7-linux-androideabi, NOT
# thumbv7neon-linux-androideabi: corrosion only picks the thumb/NEON triple
# when CMAKE_ANDROID_ARM_MODE is false, and Qt's android_armv7 toolchain sets
# it true. Getting this wrong surfaces late, as a corrosion configure error in
# the ExternalProject sub-build ("Target ... is not installed for toolchain"),
# so check it here where the message is actionable.
missing_targets=""
installed_targets="$(rustup target list --installed 2>/dev/null || true)"
for abi in "${_abi_list[@]}"; do
    case "$abi" in
        arm64-v8a)   triple="aarch64-linux-android" ;;
        armeabi-v7a) triple="armv7-linux-androideabi" ;;
        x86_64)      triple="x86_64-linux-android" ;;
        x86)         triple="i686-linux-android" ;;
    esac
    grep -qx "$triple" <<< "$installed_targets" || missing_targets="$missing_targets $triple"
done
if [ -n "$missing_targets" ]; then
    die "Missing Rust targets for the requested ABIs:$missing_targets
Install with: rustup target add$missing_targets"
fi

[ -d "$ANDROID_SDK_ROOT" ] || die "ANDROID_SDK_ROOT not found: $ANDROID_SDK_ROOT"
[ -n "$ANDROID_NDK_ROOT" ] && [ -d "$ANDROID_NDK_ROOT" ] \
    || die "ANDROID_NDK_ROOT not found: ${ANDROID_NDK_ROOT:-<unset>}"

# The default above picks the highest installed NDK, which would silently
# select r28 if it were ever installed. r28 is incompatible with Qt 6.9.3 at
# minSdk 27: its libc++ references pthread_cond_clockwait, declared by bionic
# only at API 30+, which breaks the cxx C++ build. Stay on r26b/r27.
# See docs/pure-rust-audio-backend.md.
ndk_major="$(basename "$ANDROID_NDK_ROOT" | cut -d. -f1)"
if [ "${ndk_major:-0}" -ge 28 ] 2>/dev/null; then
    die "NDK $(basename "$ANDROID_NDK_ROOT") is not supported with Qt $QT_ANDROID_VERSION at minSdk 27.
NDK r28+ breaks the cxx C++ build (libc++ pthread_cond_clockwait needs API 30+).
Install r26b or r27 and point ANDROID_NDK_ROOT at it."
fi

command -v ninja >/dev/null 2>&1 || die "ninja not found.
Multi-ABI needs the Ninja generator: Qt warns that ExternalProject step
ordering is unreliable with other generators."

# --- JDK selection ----------------------------------------------------------
#
# The Android Gradle Plugin pinned in android/build.gradle (8.6.0) supports JDK
# 17-21. On a rolling distro the *system default* java is newer than that, and
# the failure it produces is spectacularly unhelpful: the
# `lintVitalAnalyzeRelease` task dies with the JDK's own version string as the
# entire error message —
#
#     Execution failed for task ':lintVitalAnalyzeRelease'.
#     > A failure occurred while executing ...AndroidLintWorkAction
#        > 26.0.1
#
# — because the IntelliJ core bundled in that AGP's lint cannot parse a Java 26
# version string. Everything else (all ABIs, signing) succeeds first, so this
# surfaces only at the very end of a long build.
#
# Pin an explicit, supported JDK instead of inheriting whatever `java` happens
# to be. That also makes the build reproducible across machines, and is
# effectively what Qt Creator did implicitly with its own configured JDK.
# jarsigner (which androiddeployqt uses to sign an AAB) comes from here too.
MAX_JDK_MAJOR=21

jdk_major_of() {
    # `openjdk version "21.0.11"` -> 21 ; `"1.8.0_452"` -> 8
    "$1/bin/java" -version 2>&1 | head -1 \
        | sed -E 's/.*version "([0-9]+)(\.([0-9]+))?.*/\1 \3/' \
        | awk '{ if ($1 == 1) print $2; else print $1 }'
}

if [ -z "${ANDROID_JAVA_HOME:-}" ]; then
    # Prefer an explicitly-set JAVA_HOME when it is in range.
    if [ -n "${JAVA_HOME:-}" ] && [ -x "${JAVA_HOME}/bin/java" ] \
        && [ "$(jdk_major_of "$JAVA_HOME")" -le "$MAX_JDK_MAJOR" ] 2>/dev/null
    then
        ANDROID_JAVA_HOME="$JAVA_HOME"
    else
        # Otherwise the highest supported JDK installed, newest first.
        for candidate in $(ls -d /usr/lib/jvm/java-*-openjdk /usr/lib/jvm/jdk-* \
                                 /usr/lib/jvm/temurin-* 2>/dev/null | sort -Vr); do
            [ -x "$candidate/bin/java" ] || continue
            major="$(jdk_major_of "$candidate" 2>/dev/null || echo 0)"
            if [ "${major:-0}" -ge 17 ] 2>/dev/null \
                && [ "${major:-0}" -le "$MAX_JDK_MAJOR" ] 2>/dev/null
            then
                ANDROID_JAVA_HOME="$candidate"
                break
            fi
        done
    fi
fi

if [ -z "${ANDROID_JAVA_HOME:-}" ] || [ ! -x "${ANDROID_JAVA_HOME}/bin/java" ]; then
    die "No JDK 17-$MAX_JDK_MAJOR found (system default is $(java -version 2>&1 | head -1)).
The Android Gradle Plugin 8.6.0 in android/build.gradle does not support newer
JDKs: lint fails with the JDK version string as its only error message.
Install one (e.g. 'jdk21-openjdk') or set ANDROID_JAVA_HOME to its path."
fi

export JAVA_HOME="$ANDROID_JAVA_HOME"
export PATH="$JAVA_HOME/bin:$PATH"

[ -x "$JAVA_HOME/bin/jarsigner" ] \
    || die "jarsigner not found in $JAVA_HOME/bin — androiddeployqt needs it to sign an AAB."

# ---------------------------------------------------------------------------
# Signing credentials
# ---------------------------------------------------------------------------

if [ "$DO_SIGN" -eq 1 ]; then
    if [ -f "$SIGNING_ENV" ]; then
        # Values already exported in the shell win: capture, source, restore.
        _pre_path="${QT_ANDROID_KEYSTORE_PATH:-}"
        _pre_alias="${QT_ANDROID_KEYSTORE_ALIAS:-}"
        _pre_store="${QT_ANDROID_KEYSTORE_STORE_PASS:-}"
        _pre_key="${QT_ANDROID_KEYSTORE_KEY_PASS:-}"
        set -a
        # shellcheck disable=SC1090
        . "./$SIGNING_ENV"
        set +a
        [ -n "$_pre_path" ]  && export QT_ANDROID_KEYSTORE_PATH="$_pre_path"
        [ -n "$_pre_alias" ] && export QT_ANDROID_KEYSTORE_ALIAS="$_pre_alias"
        [ -n "$_pre_store" ] && export QT_ANDROID_KEYSTORE_STORE_PASS="$_pre_store"
        [ -n "$_pre_key" ]   && export QT_ANDROID_KEYSTORE_KEY_PASS="$_pre_key"
    fi

    : "${QT_ANDROID_KEYSTORE_PATH:?not set. Create $SIGNING_ENV from android/signing.env.example, or export it.}"
    : "${QT_ANDROID_KEYSTORE_ALIAS:?not set. Create $SIGNING_ENV from android/signing.env.example, or export it.}"
    : "${QT_ANDROID_KEYSTORE_STORE_PASS:?not set. Create $SIGNING_ENV from android/signing.env.example, or export it.}"
    # androiddeployqt falls back to the store password when the key password is
    # unset, which is the usual case for a single-key upload keystore.
    export QT_ANDROID_KEYSTORE_KEY_PASS="${QT_ANDROID_KEYSTORE_KEY_PASS:-$QT_ANDROID_KEYSTORE_STORE_PASS}"

    [ -f "$QT_ANDROID_KEYSTORE_PATH" ] \
        || die "Keystore not found: $QT_ANDROID_KEYSTORE_PATH"

    export QT_ANDROID_KEYSTORE_PATH QT_ANDROID_KEYSTORE_ALIAS QT_ANDROID_KEYSTORE_STORE_PASS
fi

# ---------------------------------------------------------------------------
# Configure & build
# ---------------------------------------------------------------------------

if [ "$DO_CLEAN" -eq 1 ]; then
    echo "==> Removing $ANDROID_BUILD_DIR"
    rm -rf "$ANDROID_BUILD_DIR"
fi

sign_flag_apk="OFF"
sign_flag_aab="OFF"
if [ "$DO_SIGN" -eq 1 ]; then
    sign_flag_apk="ON"
    sign_flag_aab="ON"
fi

echo "==> Qt          : $QT_ANDROID_ROOT (primary ABI $ANDROID_PRIMARY_ABI)"
echo "==> JDK         : $JAVA_HOME ($("$JAVA_HOME/bin/java" -version 2>&1 | head -1))"
echo "==> ABIs        : $ANDROID_ABIS"
echo "==> NDK         : $ANDROID_NDK_ROOT"
echo "==> Build type  : $ANDROID_BUILD_TYPE"
echo "==> Package     : $PACKAGE_TARGET"
echo "==> Signing     : $([ "$DO_SIGN" -eq 1 ] && echo "yes, alias '$QT_ANDROID_KEYSTORE_ALIAS'" || echo "no")"
echo "==> versionCode : ${ANDROID_VERSION_CODE:-<CMake default>}"
echo "==> versionName : ${ANDROID_VERSION_NAME:-<CMake default>}"
echo

# Google Play rejects an upload whose versionCode is not strictly greater than
# every previous upload of the package. Unset here means "whatever is already
# in the CMake cache", which on a fresh build directory is the CMakeLists
# default of 1 — i.e. an upload Play will refuse.
if [ "$DO_SIGN" -eq 1 ] && [ "$PACKAGE_TARGET" = "aab" ] && [ -z "${ANDROID_VERSION_CODE:-}" ]; then
    echo "WARNING: ANDROID_VERSION_CODE is not set. The build will reuse the"
    echo "         CMake cache value (1 on a fresh build dir). Google Play"
    echo "         requires it to strictly increase on every upload:"
    echo "           make android-aab ANDROID_VERSION_CODE=<n> ANDROID_VERSION_NAME=<v>"
    echo
fi

version_args=()
[ -n "${ANDROID_VERSION_CODE:-}" ] && version_args+=("-DANDROID_VERSION_CODE=$ANDROID_VERSION_CODE")
[ -n "${ANDROID_VERSION_NAME:-}" ] && version_args+=("-DANDROID_VERSION_NAME=$ANDROID_VERSION_NAME")

"$QT_CMAKE" \
    -S . -B "$ANDROID_BUILD_DIR" \
    -G Ninja \
    -DCMAKE_BUILD_TYPE="$ANDROID_BUILD_TYPE" \
    -DQT_ANDROID_ABIS="$ANDROID_ABIS" \
    -DANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" \
    -DANDROID_NDK_ROOT="$ANDROID_NDK_ROOT" \
    -DQT_ANDROID_SIGN_APK="$sign_flag_apk" \
    -DQT_ANDROID_SIGN_AAB="$sign_flag_aab" \
    "${version_args[@]}"

cmake --build "$ANDROID_BUILD_DIR" --target "$PACKAGE_TARGET"

# ---------------------------------------------------------------------------
# Locate and verify the artifact
# ---------------------------------------------------------------------------

# Pick the most recently modified matching artifact. Deliberately NOT filtered
# by mtime: an up-to-date incremental rebuild does not rewrite the package, and
# a mtime window would then report a spurious failure. The mtime is printed
# instead so a stale artifact is visible rather than silently trusted.
#
# Release and debug variants both land under outputs/, so restrict to the
# variant that was actually asked for.
if [ "$ANDROID_BUILD_TYPE" = "Debug" ]; then
    variant_dir="debug"
else
    variant_dir="release"
fi

if [ "$PACKAGE_TARGET" = "aab" ]; then
    artifact="$(find "$ANDROID_BUILD_DIR" -path "*/build/outputs/bundle/$variant_dir/*" \
                    -name '*.aab' -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)"
else
    artifact="$(find "$ANDROID_BUILD_DIR" -path "*/build/outputs/apk/$variant_dir/*" \
                    -name '*.apk' -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)"
fi

echo
if [ -z "$artifact" ]; then
    die "Build reported success but no $variant_dir $PACKAGE_TARGET was found under $ANDROID_BUILD_DIR"
fi

echo "==> Artifact: $artifact"
echo "==> Modified: $(date -r "$artifact" '+%Y-%m-%d %H:%M:%S')"
echo "==> ABIs in the package:"
unzip -l "$artifact" \
    | grep -oE '(base/)?lib/[a-z0-9_-]+/' \
    | sed 's|.*lib/||; s|/$||' \
    | sort -u \
    | sed 's/^/      /'

# --- cross-ABI contamination check ------------------------------------------
#
# The multi-ABI packaging pass copies the PRIMARY ABI's Qt plugins into the
# other ABIs' library folders — reproducible from a clean tree, and visible in
# `androiddeployqt --verbose` as
#   -- Copied .../libs/armeabi-v7a/libplugins_platforms_qtforandroid_arm64-v8a.so
# while the same files are rejected as "architecture mismatch" in another phase.
# Observed as 28 aarch64 .so files in base/lib/x86_64/ and base/lib/armeabi-v7a/.
#
# android/build.gradle excludes them via packagingOptions.jniLibs.excludes; this
# is the independent check that the exclusion still works. Qt suffixes every
# library it deploys with the ABI name, so a suffix disagreeing with its
# containing directory is unambiguous. Hard failure: do not upload such a bundle.
echo "==> Cross-ABI contamination check:"
contaminated=""
for abi in arm64-v8a armeabi-v7a x86_64 x86; do
    # Names suffixed with a *different* ABI than the directory they sit in.
    strays="$(unzip -l "$artifact" | awk '{print $4}' \
        | grep -E "^(base/)?lib/$abi/.*\.so$" \
        | grep -vE "_${abi}\.so$" \
        | grep -E '_(arm64-v8a|armeabi-v7a|x86_64|x86)\.so$' || true)"
    if [ -n "$strays" ]; then
        count="$(printf '%s\n' "$strays" | wc -l)"
        contaminated="$contaminated
      $abi: $count librar$([ "$count" -eq 1 ] && echo y || echo ies) built for another ABI, e.g.
        $(printf '%s\n' "$strays" | head -3 | sed 's|.*/||' | paste -sd', ')"
    fi
done

if [ -n "$contaminated" ]; then
    echo "$contaminated"
    echo
    die "Wrong-architecture libraries are packaged in the artifact.

androiddeployqt stages the primary ABI's Qt plugins into the other ABIs' folders;
android/build.gradle is supposed to exclude them via
packagingOptions.jniLibs.excludes. That exclusion is not covering these files —
check whether Qt changed its plugin naming, or an ABI was added without a
matching exclude pattern.

See docs/android-multi-abi-and-chromeos.md.
Do not upload this artifact."
fi
echo "      OK — every library matches its ABI directory."

# --- ChromeOS compatibility check -------------------------------------------
#
# The trap that made Simsapa "not compatible" on Chromebooks: a REQUIRED
# hardware feature — either declared outright, or implied by a permission —
# silently removes the app from the Play Store on ChromeOS.
#
# This audits the *merged* manifest (plain XML, produced by the Android
# manifest merger) rather than the source one, because the whole point is to
# catch anything a Qt module or an AndroidX dependency contributed behind our
# back. `aapt2 dump badging` only works on APKs; the merged manifest works for
# both APK and AAB builds.
merged_manifest="$(find "$ANDROID_BUILD_DIR" -path "*/merged_manifests/$variant_dir/*" \
                       -name 'AndroidManifest.xml' -printf '%T@ %p\n' 2>/dev/null \
                       | sort -n | tail -1 | cut -d' ' -f2-)"

#
# Parsed as XML rather than grepped. Grepping is not good enough here: an
# explanatory XML *comment* in android/AndroidManifest.xml that merely mentions
# `<uses-feature>` is carried into the merged manifest verbatim and matches a
# naive pattern, which produced a false positive. ElementTree drops comments and
# normalises attribute wrapping, so neither issue can recur.
#
# Reports only — never fails the build. The package is already built and valid
# at this point; the exit status must reflect the build, not the advisory.
if [ -n "$merged_manifest" ] && command -v python3 >/dev/null 2>&1; then
    python3 - "$merged_manifest" <<'PY' || true
import sys, xml.etree.ElementTree as ET

NS = "{http://schemas.android.com/apk/res/android}"
INDENT = " " * 6

# Permissions from which Google Play infers a REQUIRED hardware feature.
# https://developer.android.com/topic/arc/manifest
IMPLIED = {
    "CAMERA": "android.hardware.camera + .camera.autofocus",
    "ACCESS_FINE_LOCATION": "android.hardware.location.gps",
    "ACCESS_COARSE_LOCATION": "android.hardware.location.gps",
    "CALL_PHONE": "android.hardware.telephony",
    "CALL_PRIVILEGED": "android.hardware.telephony",
    "PROCESS_OUTGOING_CALLS": "android.hardware.telephony",
    "MODIFY_PHONE_STATE": "android.hardware.telephony",
    "READ_SMS": "android.hardware.telephony",
    "WRITE_SMS": "android.hardware.telephony",
    "SEND_SMS": "android.hardware.telephony",
    "RECEIVE_SMS": "android.hardware.telephony",
    "RECEIVE_MMS": "android.hardware.telephony",
    "RECEIVE_WAP_PUSH": "android.hardware.telephony",
    "WRITE_APN_SETTINGS": "android.hardware.telephony",
}

root = ET.parse(sys.argv[1]).getroot()

perms = sorted({
    (e.get(NS + "name") or "").rsplit(".", 1)[-1]
    for e in root.findall("uses-permission")
    if e.get(NS + "name")
})
print("==> Permissions in the merged manifest:")
for p in perms:
    print(INDENT + p)

problems = []
for p in perms:
    if p in IMPLIED:
        problems.append("%s  ->  implies REQUIRED %s" % (p, IMPLIED[p]))

# A <uses-feature> without android:required defaults to REQUIRED.
for e in root.findall("uses-feature"):
    required = (e.get(NS + "required") or "true").lower()
    if required != "false":
        name = e.get(NS + "name") or e.get(NS + "glEsVersion") or "<unnamed>"
        problems.append('<uses-feature> without required="false": ' + name)

if problems:
    print()
    print("WARNING: ChromeOS / device-filtering risks in the merged manifest:")
    for line in problems:
        print(INDENT + line)
    print()
    print(INDENT + 'Declare the feature with android:required="false" in')
    print(INDENT + "android/AndroidManifest.xml, and drop the permission if unused.")
    print(INDENT + "See docs/android-multi-abi-and-chromeos.md.")
else:
    print("==> ChromeOS check: no required hardware features. OK.")
PY
elif [ -n "$merged_manifest" ]; then
    echo "==> ChromeOS check: SKIPPED (python3 not found)"
else
    echo "==> ChromeOS check: SKIPPED (merged manifest not found)"
fi

echo
echo "Done."
