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

# The Qt version comes from CMakeLists.txt's QT_ANDROID -- it is declared in
# exactly one place and read here, never hardcoded a second time. Android
# deliberately targets a DIFFERENT Qt version than the desktop platforms, so a
# stale copy of the version here would silently build against the wrong kit.
#
# QT_ENV_NO_ACTIVATE=1 is required: sourcing qt-env.sh bare would also put the
# DESKTOP kit on PATH, which is wrong in an Android build. We want the lookup
# helpers and nothing else. See the header of scripts/qt-env.sh.
QT_ENV_NO_ACTIVATE=1 . ./scripts/qt-env.sh

# QT_ENV_NO_ACTIVATE stops THIS script from adding the desktop kit, but it does
# nothing about a desktop kit an OUTER shell already exported -- direnv (.envrc),
# .claude/settings.json, or a hand-run `source scripts/qt-env.sh` all leave
# QT_PREFIX set with $QT_PREFIX/bin on PATH and $QT_PREFIX/lib on
# LD_LIBRARY_PATH. Those must be removed here, and LD_LIBRARY_PATH is the
# dangerous half:
#
# The Android cross-build runs the ANDROID Qt's HOST TOOLS -- moc, rcc,
# androiddeployqt from $QT_ANDROID_ROOT/gcc_64, resolved automatically via
# __qt_platform_initial_qt_host_path. Those are dynamically linked against
# libQt6Core.so.6. With the desktop kit's lib/ first in the loader's search
# path, a 6.10.3 moc/rcc loads 6.9.3's libQt6Core -- a version mismatch that
# either aborts mid-build or, worse, appears to work.
#
# This is invisible while QT_ANDROID == QT_LINUX, and becomes live the moment
# they diverge, which is the whole point of the Android-only Qt bump. Same
# failure class as the QT_ANDROID_VERSION export removed from scripts/qt-env.sh:
# a convenience layer silently changing build output.
if [ -n "${QT_PREFIX:-}" ]; then
    echo "==> Scrubbing desktop Qt kit from this build's environment: $QT_PREFIX"
    PATH="$(printf '%s' "$PATH" | sed -e "s#${QT_PREFIX}/bin:##g" -e "s#:${QT_PREFIX}/bin##g")"
    export PATH
    # Cleared outright rather than filtered. The Android build needs no
    # LD_LIBRARY_PATH at all (Qt's own scripts set what they need), so an empty
    # value cannot be wrong here, whereas a filtered one can still carry another
    # Qt from somewhere else on the path.
    if [ -n "${LD_LIBRARY_PATH:-}" ]; then
        echo "==> Clearing LD_LIBRARY_PATH (was: $LD_LIBRARY_PATH)"
        unset LD_LIBRARY_PATH
    fi
    # Belongs to the desktop kit; the gate reports it, so leaving it set would
    # make the report describe an environment this build no longer has.
    unset QT_PREFIX
    unset QMAKE
fi

# Reported in the run header, so "which Qt is this build using, and who decided
# that" is answerable from the log rather than by re-deriving it afterwards.
if [ -n "${QT_ANDROID_VERSION:-}" ]; then
    qt_android_version_source="from environment"
else
    qt_android_version_source="from CMakeLists.txt QT_ANDROID"
fi
QT_ANDROID_VERSION="${QT_ANDROID_VERSION:-$(qt_version_for ANDROID)}"
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
# Stay on the Qt-supported NDK (r26b/r27). Do NOT use r28 — at this project's
# minSdk its libc++ references pthread_cond_clockwait (bionic API 30+), which
# breaks the cxx C++ build. The exclusion holds at minSdk 28 as well as 27.
# See docs/pure-rust-audio-backend.md.
ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-$(ls -d "$ANDROID_SDK_ROOT"/ndk/* 2>/dev/null | sort -V | tail -1)}"

# Export both, so scripts/qt-env-verify.sh (a subprocess) inspects the values
# this build will actually use rather than re-deriving its own. Without the
# export the gate saw ANDROID_NDK_ROOT as unset and stopped every Android build
# with a CRITICAL failure.
export ANDROID_SDK_ROOT ANDROID_NDK_ROOT

ANDROID_BUILD_DIR="${ANDROID_BUILD_DIR:-build/android-multiabi}"
ANDROID_BUILD_TYPE="${ANDROID_BUILD_TYPE:-Release}"

SIGNING_ENV="${SIGNING_ENV:-android/signing.env}"

PACKAGE_TARGET="aab"
DO_CLEAN=0
DO_SIGN=1
DO_BETA=0
# Explicit --sign / --no-sign wins over the default that --debug implies,
# regardless of argument order. Empty = not given.
SIGN_OVERRIDE=""

# ---------------------------------------------------------------------------
# Arguments
# ---------------------------------------------------------------------------

usage() {
    cat <<'EOF'
Usage: ./build-android.sh [options]

  --aab             Build an Android App Bundle (default; for Google Play)
  --apk             Build an APK instead (for sideloading / local testing)
  --abis "a;b;c"    Override the ABI list (default: arm64-v8a;x86_64;armeabi-v7a)
  --debug           CMAKE_BUILD_TYPE=Debug (defaults to --no-sign)
  --beta            Build the BETA package (io.github.simsapa.app.beta, label
                    "Simsapa (beta)", versionName suffixed "-beta"), which
                    installs alongside the released app instead of replacing
                    it. With --apk alone this is the NOT-debuggable artifact
                    for GitHub Releases; combine --debug --sign instead for a
                    debuggable local build of the same package.
  --sign            Sign even a --debug build, with the release keystore. The
                    APK is re-signed after the build (apksigner replaces the
                    debug-keystore signature), so a debuggable build installs
                    over a sideloaded release APK instead of being rejected
                    for a signature mismatch. Order-independent: --debug --sign
                    and --sign --debug are the same.
  --no-sign         Skip signing; produce an unsigned package
  --clean           Remove the build directory before configuring
  -h, --help        This message

Note: this only helps against packages signed with the SAME key. An install
that came from Google Play is signed by Play App Signing (Google's key, not
the upload key in android/signing.env), so no locally built package can ever
replace it — the Play copy has to be uninstalled first.

Version:
  versionCode comes from android/version.txt — edit that file before a Play
  upload; Play requires it to strictly increase. versionName comes from the
  [package] version in bridges/Cargo.toml. Neither needs a command-line
  argument.

Environment:
  ANDROID_VERSION_CODE   Overrides android/version.txt (must be non-empty)
  ANDROID_VERSION_NAME   Overrides the Cargo.toml version, e.g. 1.0.0-alpha.3
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
        --beta) DO_BETA=1 ;;
        --sign) SIGN_OVERRIDE=1 ;;
        --no-sign) SIGN_OVERRIDE=0 ;;
        --clean) DO_CLEAN=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

[ -n "$SIGN_OVERRIDE" ] && DO_SIGN="$SIGN_OVERRIDE"

# A signed Debug build is signed by this script after the build, not by
# androiddeployqt: Gradle's debug variant already carries a debug-keystore
# signature, and androiddeployqt's --sign path is only exercised for release
# variants. apksigner replaces any existing signature, so re-signing the
# finished artifact is the deterministic route.
RESIGN_AFTER_BUILD=0
if [ "$DO_SIGN" -eq 1 ] && [ "$ANDROID_BUILD_TYPE" = "Debug" ]; then
    RESIGN_AFTER_BUILD=1
fi

# The debug build type carries the beta id unconditionally (build.gradle), so
# --beta only means something for a release-type build.
if [ "$DO_BETA" -eq 1 ] && [ "$ANDROID_BUILD_TYPE" = "Debug" ]; then
    DO_BETA=0
fi

# The beta package is distributed as an APK from GitHub Releases. An AAB is a
# Play upload format, and the Play listing is the plain io.github.simsapa.app
# id — a beta bundle has nowhere to go.
if [ "$DO_BETA" -eq 1 ] && [ "$PACKAGE_TARGET" = "aab" ]; then
    echo "ERROR: --beta builds an APK for direct distribution; --aab is for the Play upload of the release package." >&2
    exit 2
fi

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
# select r28 if it were ever installed. r28 is incompatible with Qt at this
# project's minSdk: its libc++ references pthread_cond_clockwait, declared by
# bionic only at API 30+, which breaks the cxx C++ build. The exclusion holds at
# minSdk 28 as well as 27. Stay on r26b/r27.
# See docs/pure-rust-audio-backend.md.
ndk_major="$(basename "$ANDROID_NDK_ROOT" | cut -d. -f1)"
if [ "${ndk_major:-0}" -ge 28 ] 2>/dev/null; then
    die "NDK $(basename "$ANDROID_NDK_ROOT") is not supported with Qt $QT_ANDROID_VERSION at this project's minSdk.
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
# Package version
# ---------------------------------------------------------------------------
#
# versionCode comes from android/version.txt, versionName from the [package]
# version in bridges/Cargo.toml. Both are exported and read by CMakeLists.txt
# via $ENV{} — there are no -D arguments, so an edit to either file takes effect
# on the next run of this script (which always re-runs `cmake -S . -B`) even in
# an existing build directory.
#
# A non-empty inherited value wins, so `make android-aab ANDROID_VERSION_CODE=9`
# still overrides. The test must be `-n`, not an is-set test: the Makefile
# exports both names unconditionally and GNU make exports an undefined variable
# as the EMPTY STRING, so on a plain `make android-aab` both arrive set-but-empty
# and an is-set test would read that as a deliberate override.

VERSION_CODE_FILE="android/version.txt"
CARGO_TOML="bridges/Cargo.toml"

version_code_source="android/version.txt"
if [ -n "${ANDROID_VERSION_CODE:-}" ]; then
    version_code_source="environment"
else
    [ -f "$VERSION_CODE_FILE" ] \
        || die "$VERSION_CODE_FILE not found.
       It holds the Android versionCode (a single positive integer).
       Restore it, or export ANDROID_VERSION_CODE=<n> for this build."

    # First non-blank, non-comment line. Deliberately parsed rather than
    # sourced: the file is data, not shell.
    ANDROID_VERSION_CODE="$(sed -e 's/#.*//' -e 's/[[:space:]]//g' \
                                "$VERSION_CODE_FILE" | grep -m1 .)" || true

    case "$ANDROID_VERSION_CODE" in
        ""|*[!0-9]*)
            die "Could not read a versionCode from $VERSION_CODE_FILE.
       Expected a single positive integer on its own line, got: '${ANDROID_VERSION_CODE}'
       Google Play requires it to strictly increase on every upload." ;;
    esac
    [ "$ANDROID_VERSION_CODE" -gt 0 ] 2>/dev/null \
        || die "versionCode in $VERSION_CODE_FILE must be a positive integer, got: $ANDROID_VERSION_CODE"
fi

version_name_source="bridges/Cargo.toml"
if [ -n "${ANDROID_VERSION_NAME:-}" ]; then
    version_name_source="environment"
else
    [ -f "$CARGO_TOML" ] || die "$CARGO_TOML not found; cannot determine versionName."

    # The [package] version only — stop at the first table after it so a
    # dependency's `version = ` cannot be picked up instead.
    ANDROID_VERSION_NAME="$(awk '
        /^\[package\]/       { in_pkg = 1; next }
        /^\[/                { in_pkg = 0 }
        in_pkg && /^[[:space:]]*version[[:space:]]*=/ {
            if (match($0, /"[^"]*"/)) {
                print substr($0, RSTART + 1, RLENGTH - 2)
                exit
            }
        }
    ' "$CARGO_TOML")"

    [ -n "$ANDROID_VERSION_NAME" ] \
        || die "Could not parse the [package] version from $CARGO_TOML."
fi

export ANDROID_VERSION_CODE ANDROID_VERSION_NAME

# ---------------------------------------------------------------------------
# Configure & build
# ---------------------------------------------------------------------------

if [ "$DO_CLEAN" -eq 1 ]; then
    echo "==> Removing $ANDROID_BUILD_DIR"
    rm -rf "$ANDROID_BUILD_DIR"
fi

sign_flag_apk="OFF"
sign_flag_aab="OFF"
if [ "$DO_SIGN" -eq 1 ] && [ "$RESIGN_AFTER_BUILD" -eq 0 ]; then
    sign_flag_apk="ON"
    sign_flag_aab="ON"
fi

echo "==> Qt          : $QT_ANDROID_VERSION ($qt_android_version_source), $QT_ANDROID_ROOT (primary ABI $ANDROID_PRIMARY_ABI)"
echo "==> JDK         : $JAVA_HOME ($("$JAVA_HOME/bin/java" -version 2>&1 | head -1))"
echo "==> ABIs        : $ANDROID_ABIS"
echo "==> NDK         : $ANDROID_NDK_ROOT"
echo "==> Build type  : $ANDROID_BUILD_TYPE"
echo "==> Package     : $PACKAGE_TARGET"
if [ "$DO_SIGN" -eq 1 ] && [ "$RESIGN_AFTER_BUILD" -eq 1 ]; then
    echo "==> Signing     : yes, alias '$QT_ANDROID_KEYSTORE_ALIAS' (re-signed after the build)"
else
    echo "==> Signing     : $([ "$DO_SIGN" -eq 1 ] && echo "yes, alias '$QT_ANDROID_KEYSTORE_ALIAS'" || echo "no")"
fi
echo "==> versionCode : $ANDROID_VERSION_CODE (from $version_code_source)"
echo "==> versionName : $ANDROID_VERSION_NAME (from $version_name_source)"
echo

# Environment gate. Runs HERE, after JAVA_HOME / ANDROID_NDK_ROOT / ANDROID_ABIS
# are resolved, so it verifies the values this build will actually use rather
# than re-deriving its own. Aborts on a critical failure -- a wrong toolchain
# should stop the build now, not after three ABIs have compiled.
./scripts/qt-env-verify.sh --platform android || exit 1

# Tell android/build.gradle to disable the debug variant. androiddeployqt
# appends the bare `bundle` task, which otherwise builds, packages and signs the
# entire debug variant alongside the release one for nothing.
#
# Gradle is invoked by androiddeployqt, so there is no -P argument to pass;
# Gradle maps ORG_GRADLE_PROJECT_<name> environment variables to project
# properties instead.
#
# For a debug build the variable must be left UNSET, never set to "false":
# project.hasProperty() is true for any value, including "false" and "".
if [ "$ANDROID_BUILD_TYPE" != "Debug" ]; then
    export ORG_GRADLE_PROJECT_simsapaReleaseOnly=true
    echo "==> Debug variant: disabled (release build)"
else
    unset ORG_GRADLE_PROJECT_simsapaReleaseOnly
    echo "==> Debug variant: enabled"
fi

# Beta identity for a release-type build. Same mechanism and the same
# unset-not-false rule as simsapaReleaseOnly above; build.gradle applies the
# ".beta" applicationIdSuffix and the "Simsapa (beta)" manifest overlay to the
# release variant only when this property is present.
if [ "$DO_BETA" -eq 1 ]; then
    export ORG_GRADLE_PROJECT_simsapaBeta=true
    echo "==> Package id  : io.github.simsapa.app.beta (beta, not debuggable)"
else
    unset ORG_GRADLE_PROJECT_simsapaBeta
    if [ "$ANDROID_BUILD_TYPE" = "Debug" ]; then
        echo "==> Package id  : io.github.simsapa.app.beta (beta, DEBUGGABLE — do not distribute)"
    else
        echo "==> Package id  : io.github.simsapa.app"
    fi
fi
echo

# --- package-identity guard -------------------------------------------------
#
# The packaging step is driven by ninja, whose `apk` target is up to date as
# soon as android-build/simsapadhammareader.apk exists and no source changed.
# The beta identity, though, arrives through a Gradle project property, which
# is not one of ninja's inputs — so building --beta and then plain --apk in the
# same directory would skip androiddeployqt entirely and report the PREVIOUS
# build's artifact, with the wrong applicationId. Observed exactly that: a
# plain release build reporting io.github.simsapa.app.beta.
#
# So remember what identity this directory was last packaged with, and when it
# changes, delete the packaging outputs to force androiddeployqt and Gradle to
# run again.
#
# This deletes only *outputs*. It must never delete android-build/ itself: the
# per-ABI ExternalProject copy stamps live outside it and would then consider
# themselves up to date, leaving the staging directory unpopulated and the tree
# permanently wedged (use `make android-clean` for that).
#
# The signing state is part of the identity as well. `make android-apk-debug`
# (unsigned) straight after `make android-beta-debug` (release-signed) produces
# the same package id, so without it ninja stays up to date and the script
# reports the still-release-signed APK as an unsigned debug build. The re-sign
# is applied to the artifact in place, so nothing else would reveal it.
package_identity="$ANDROID_BUILD_TYPE-beta$DO_BETA-sign$DO_SIGN"
identity_marker="$ANDROID_BUILD_DIR/.simsapa-package-identity"

# A MISSING marker forces the re-package too: an existing build directory from
# before this guard existed, or one left by a build that was interrupted, has
# an unknown identity, and "unknown" must not be treated as "matching".
if [ -d "$ANDROID_BUILD_DIR/android-build" ] \
       && { [ ! -f "$identity_marker" ] || [ "$(cat "$identity_marker")" != "$package_identity" ]; }; then
    echo "==> Package identity is $(if [ -f "$identity_marker" ]; then cat "$identity_marker"; else echo unknown; fi), want $package_identity; forcing a re-package"
    rm -rf "$ANDROID_BUILD_DIR/android-build/build/outputs"
    rm -f  "$ANDROID_BUILD_DIR/android-build/simsapadhammareader.apk" \
           "$ANDROID_BUILD_DIR/android-build/simsapadhammareader.aab"
    echo
fi

# No -DANDROID_VERSION_* arguments: CMakeLists.txt reads the exported
# environment instead. They used to be CACHE variables, which are written once
# per build directory and would therefore ignore an edited version.txt on any
# subsequent build in the same tree.
"$QT_CMAKE" \
    -S . -B "$ANDROID_BUILD_DIR" \
    -G Ninja \
    -DCMAKE_BUILD_TYPE="$ANDROID_BUILD_TYPE" \
    -DQT_ANDROID_ABIS="$ANDROID_ABIS" \
    -DANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" \
    -DANDROID_NDK_ROOT="$ANDROID_NDK_ROOT" \
    -DQT_ANDROID_SIGN_APK="$sign_flag_apk" \
    -DQT_ANDROID_SIGN_AAB="$sign_flag_aab"

cmake --build "$ANDROID_BUILD_DIR" --target "$PACKAGE_TARGET"

# Recorded only after a successful package, so an interrupted build does not
# leave the marker claiming an identity that was never produced.
mkdir -p "$ANDROID_BUILD_DIR"
printf '%s' "$package_identity" > "$identity_marker"

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

# --- re-sign a Debug build with the release keystore ------------------------
#
# Only reached for `--debug --sign`. Purpose: a debuggable APK that carries the
# SAME signature as a sideloaded release APK, so `adb install -r` replaces it
# instead of failing with INSTALL_FAILED_UPDATE_INCOMPATIBLE. The debuggable
# flag lives in the manifest and is untouched by signing.
#
# Does NOT let the package replace a Play-installed copy: Play App Signing
# re-signs uploads with Google's key, which is not the upload key here.
if [ "$RESIGN_AFTER_BUILD" -eq 1 ]; then
    if [ "$PACKAGE_TARGET" != "apk" ]; then
        die "--debug --sign is only supported for --apk (an AAB is signed for upload, and Play re-signs it anyway)."
    fi

    apksigner_bin="$(ls -d "$ANDROID_SDK_ROOT"/build-tools/*/apksigner 2>/dev/null | sort -V | tail -1)"
    [ -x "$apksigner_bin" ] \
        || die "apksigner not found under $ANDROID_SDK_ROOT/build-tools — needed for --debug --sign."

    echo "==> Re-signing the debug APK with the release keystore (alias '$QT_ANDROID_KEYSTORE_ALIAS')"
    "$apksigner_bin" sign \
        --ks "$QT_ANDROID_KEYSTORE_PATH" \
        --ks-key-alias "$QT_ANDROID_KEYSTORE_ALIAS" \
        --ks-pass "pass:$QT_ANDROID_KEYSTORE_STORE_PASS" \
        --key-pass "pass:$QT_ANDROID_KEYSTORE_KEY_PASS" \
        "$artifact" \
        || die "apksigner failed to sign $artifact"

    # Prove it took, rather than trusting the exit status: print the signer the
    # device will actually see.
    "$apksigner_bin" verify --print-certs "$artifact" \
        | grep -E 'Signer #1 certificate (DN|SHA-256 digest)' \
        | sed 's/^/      /' \
        || die "apksigner could not verify the signature it just wrote to $artifact"
fi
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
echo "    Artifact    : $artifact"
echo "    versionCode : $ANDROID_VERSION_CODE (from $version_code_source)"
echo "    versionName : $ANDROID_VERSION_NAME (from $version_name_source)"
