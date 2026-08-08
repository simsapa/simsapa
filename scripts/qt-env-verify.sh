#!/usr/bin/env bash
# Build environment report and pre-flight gate.
#
# EVERY build script sources/calls this before it builds anything, so the
# toolchain a build actually used is recorded in its own log, and a build with a
# wrong or missing toolchain STOPS HERE instead of failing later somewhere less
# legible.
#
# Two kinds of check, and the distinction is the whole design:
#
#   CRITICAL  -- wrong output or no output. Aborts the build (exit 1).
#                e.g. the Qt kit is missing, or the kit's real version does not
#                match what CMakeLists.txt declares.
#   ADVISORY  -- worth knowing, not worth stopping for. Prints a warning.
#                e.g. an optional Android ABI's Rust target is not installed.
#
# Nothing here changes any state; it only reads and reports.
#
# Usage:
#   qt-env-verify.sh --platform linux|macos|android   # build-time gate
#   qt-env-verify.sh --all                            # repo-wide consistency
#   qt-env-verify.sh --platform linux --report-only   # never exit non-zero
#
# Windows is deliberately absent: build-windows.ps1 is PowerShell and cannot
# source this. It carries an equivalent `Invoke-EnvVerify` -- keep the two in
# step, and see docs/qt-kit-selection.md.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$script_dir")"

platform=""
mode="platform"
report_only=0

while [ $# -gt 0 ]; do
    case "$1" in
        --platform) platform="$(printf '%s' "${2:?--platform needs a value}" | tr '[:upper:]' '[:lower:]')"; shift 2 ;;
        --platform=*) platform="$(printf '%s' "${1#*=}" | tr '[:upper:]' '[:lower:]')"; shift ;;
        --all) mode="all"; shift ;;
        --report-only) report_only=1; shift ;;
        -h|--help) sed -n '2,27p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "qt-env-verify: unknown argument: $1" >&2; exit 2 ;;
    esac
done

critical_failures=0
advisories=0

_c_red=$'\033[31m'; _c_grn=$'\033[32m'; _c_yel=$'\033[33m'; _c_bold=$'\033[1m'; _c_off=$'\033[0m'
[ -t 1 ] || { _c_red=""; _c_grn=""; _c_yel=""; _c_bold=""; _c_off=""; }

section() { printf '\n%s%s%s\n' "$_c_bold" "$1" "$_c_off"; }
item()    { printf '  %-22s %s\n' "$1" "$2"; }
ok()      { printf '  %sOK%s       %s\n' "$_c_grn" "$_c_off" "$1"; }
critical(){ printf '  %sCRITICAL%s %s\n' "$_c_red" "$_c_off" "$1"; critical_failures=$((critical_failures + 1)); }
advise()  { printf '  %sWARN%s     %s\n' "$_c_yel" "$_c_off" "$1"; advisories=$((advisories + 1)); }

# Report a tool's version, or "(not found)". Never fails on its own -- absence
# is judged by the critical checks, which know whether this platform needs it.
tool_version() {
    local label="$1" bin="$2"; shift 2
    if command -v "$bin" >/dev/null 2>&1; then
        item "$label" "$("$@" 2>&1 | head -n1)"
    else
        item "$label" "(not found)"
    fi
}

qt_declared_version() {
    sed -n "s/^[[:space:]]*set(QT_${1}[[:space:]]*\"\([^\"]*\)\").*/\1/p" "$root/CMakeLists.txt" | head -n1
}

# ---------------------------------------------------------------------------
# Shared report: things every platform builds with.
# ---------------------------------------------------------------------------
report_common() {
    section "Build environment"
    item "date" "$(date '+%Y-%m-%d %H:%M:%S %Z')"
    item "host" "$(uname -srm)"
    item "working dir" "$root"
    if git -C "$root" rev-parse --short HEAD >/dev/null 2>&1; then
        local dirty=""
        git -C "$root" diff --quiet 2>/dev/null || dirty=" (uncommitted changes)"
        item "git" "$(git -C "$root" rev-parse --short HEAD)$dirty on $(git -C "$root" rev-parse --abbrev-ref HEAD)"
    fi

    section "Toolchain"
    tool_version "C++ compiler" "${CXX:-c++}" "${CXX:-c++}" --version
    tool_version "cmake" cmake cmake --version
    tool_version "ninja" ninja ninja --version
    tool_version "rustc" rustc rustc --version
    tool_version "cargo" cargo cargo --version
    if command -v rustup >/dev/null 2>&1; then
        item "rust targets" "$(rustup target list --installed 2>/dev/null | tr '\n' ' ')"
    fi
}

# Compare the version CMakeLists.txt DECLARES against what the kit actually IS.
# This is the check that catches a kit directory whose name lies -- a partial
# install, a hand-moved folder, or a MaintenanceTool leftover.
check_qt_kit() {
    local platform_var="$1" kit_path="$2" qmake_name="$3"
    local declared; declared="$(qt_declared_version "$platform_var")"

    section "Qt"
    item "declared (QT_$platform_var)" "${declared:-<unreadable>}"
    item "kit path" "$kit_path"

    if [ -z "$declared" ]; then
        critical "could not read QT_$platform_var from CMakeLists.txt"
        return
    fi
    if [ ! -d "$kit_path" ]; then
        critical "Qt kit not found: $kit_path"
        item "" "Install Qt $declared for this platform, or fix QT_$platform_var."
        return
    fi

    local qmake="$kit_path/bin/$qmake_name"
    if [ ! -x "$qmake" ]; then
        critical "qmake not found or not executable: $qmake (incomplete Qt install?)"
        return
    fi

    local actual; actual="$("$qmake" -query QT_VERSION 2>/dev/null)"
    item "kit reports" "${actual:-<no answer>}"
    if [ -z "$actual" ]; then
        critical "$qmake did not answer -query QT_VERSION"
    elif [ "$actual" != "$declared" ]; then
        critical "Qt version mismatch: CMakeLists.txt declares $declared, kit at $kit_path is $actual"
    else
        ok "Qt $actual matches the declared version"
    fi
}

# The readers of QT_* must agree. Cheap, and a disagreement means some part of
# the build is looking at a different version than the rest.
check_reader_agreement() {
    section "Qt version readers agree"
    local disagreed=0 p declared got
    for p in LINUX MACOS WINDOWS ANDROID IOS; do
        declared="$(qt_declared_version "$p")"
        got="$(QT_ENV_NO_ACTIVATE=1; . "$script_dir/qt-env.sh" >/dev/null 2>&1; qt_version_for "$p" 2>/dev/null)"
        [ "$got" != "$declared" ] && { critical "bash reader: QT_$p = '$got', CMakeLists.txt says '$declared'"; disagreed=1; }
        got="$(cd "$root" && make -s -f Makefile -f /dev/stdin __probe <<EOF 2>/dev/null
__probe:
	@echo \$(call qt_version_for,$p)
EOF
)"
        [ "$got" != "$declared" ] && { critical "make reader: QT_$p = '$got', CMakeLists.txt says '$declared'"; disagreed=1; }
    done
    [ "$disagreed" -eq 0 ] && ok "bash and make readers agree with CMakeLists.txt for all platforms"
}

check_rust_target() {
    local target="$1" why="$2" severity="$3"
    if rustup target list --installed 2>/dev/null | grep -qx "$target"; then
        ok "Rust target installed: $target"
    elif [ "$severity" = "critical" ]; then
        critical "Rust target NOT installed: $target ($why) -- rustup target add $target"
    else
        advise "Rust target not installed: $target ($why)"
    fi
}

# ---------------------------------------------------------------------------
# Android specifics. build-android.sh has already resolved these and exported
# them, so this reports the values the build will really use rather than
# re-deriving (and possibly disagreeing with) them.
# ---------------------------------------------------------------------------
report_android() {
    local sdk="${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}"
    local ndk="${ANDROID_NDK_ROOT:-}"
    local abis="${ANDROID_ABIS:-arm64-v8a}"

    section "Android SDK / NDK"
    item "SDK root" "$sdk"
    if [ ! -d "$sdk" ]; then
        critical "Android SDK not found: $sdk"
    fi

    if [ -z "$ndk" ]; then
        critical "ANDROID_NDK_ROOT is not set (build-android.sh should resolve it before calling this)"
    elif [ ! -d "$ndk" ]; then
        critical "Android NDK not found: $ndk"
    else
        local ndk_ver="$(basename "$ndk")"
        local ndk_props="$ndk/source.properties"
        item "NDK root" "$ndk"
        if [ -f "$ndk_props" ]; then
            item "NDK revision" "$(sed -n 's/^Pkg.Revision *= *//p' "$ndk_props" | head -n1)"
        fi
        local clang="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/clang"
        [ -x "$clang" ] && item "NDK clang" "$("$clang" --version 2>&1 | head -n1)"

        # r28 is a KNOWN-BAD compiler for this project, not a preference: at our
        # minSdk its libc++ references pthread_cond_clockwait (bionic API 30+),
        # which breaks the cxx C++ build. Both Qt's auto-detect and
        # build-android.sh take the HIGHEST installed NDK, so merely installing
        # r28 silently swaps the compiler -- hence a hard stop, not a warning.
        # See docs/pure-rust-audio-backend.md.
        local ndk_major="${ndk_ver%%.*}"
        if [ "$ndk_major" -ge 28 ] 2>/dev/null; then
            critical "NDK $ndk_ver is not supported (r28+ breaks the cxx build at this minSdk). Use r26b/r27."
        else
            ok "NDK $ndk_ver is a supported revision"
        fi
    fi

    section "Android JDK"
    local java_bin="${JAVA_HOME:+$JAVA_HOME/bin/java}"
    java_bin="${java_bin:-$(command -v java || true)}"
    if [ -z "$java_bin" ] || [ ! -x "$java_bin" ]; then
        critical "No JDK found (JAVA_HOME unset and no java on PATH)"
    else
        local jver jmajor
        jver="$("$java_bin" -version 2>&1 | head -n1)"
        item "JAVA_HOME" "${JAVA_HOME:-<unset, using PATH>}"
        item "java" "$jver"
        # NOTE the leading [^"]* rather than .* -- a greedy .* matches through to
        # the CLOSING quote of "26.0.2", captures an empty string, and the range
        # test below is then silently skipped. That bug shipped in the first
        # version of this script and made the check pass on a JDK it exists to
        # reject, which is worse than having no check at all.
        jmajor="$(printf '%s' "$jver" | sed -n 's/[^"]*"\([0-9]*\).*/\1/p')"
        # AGP 8.6.0's bundled lint cannot parse a Java 26 version string; the
        # failure is a lintVitalAnalyzeRelease crash whose ENTIRE message is the
        # JDK version, after all ABIs have compiled and signed. Catch it here
        # instead, where it costs seconds.
        if [ -z "$jmajor" ]; then
            advise "could not parse a major version from: $jver (JDK range NOT checked)"
        elif [ "$jmajor" -lt 17 ] || [ "$jmajor" -gt 21 ]; then
            critical "JDK $jmajor is outside the supported 17-21 range (AGP's lint fails late and cryptically)"
            item "" "build-android.sh selects a JDK itself; set JAVA_HOME to a 17-21 install."
        else
            ok "JDK $jmajor is within the supported 17-21 range"
        fi
    fi

    section "Android Gradle"
    local wrapper="$root/android/gradle/wrapper/gradle-wrapper.properties"
    if [ -f "$wrapper" ]; then
        item "gradle wrapper" "$(sed -n 's/.*gradle-\([0-9.]*\)-.*\.zip/\1/p' "$wrapper" | head -n1)"
    fi
    local agp; agp="$(sed -n "s/.*com\.android\.tools\.build:gradle:\([0-9.]*\).*/\1/p" "$root/android/build.gradle" 2>/dev/null | head -n1)"
    [ -n "$agp" ] && item "AGP" "$agp"
    local minsdk targetsdk
    minsdk="$(sed -n 's/^[[:space:]]*minSdkVersion[[:space:]]*\([0-9]*\).*/\1/p' "$root/android/build.gradle" 2>/dev/null | head -n1)"
    targetsdk="$(sed -n 's/^[[:space:]]*targetSdkVersion[[:space:]]*\([0-9]*\).*/\1/p' "$root/android/build.gradle" 2>/dev/null | head -n1)"
    [ -n "$minsdk" ] && item "minSdkVersion" "$minsdk"
    [ -n "$targetsdk" ] && item "targetSdkVersion" "$targetsdk"

    section "Android ABIs and Qt kits"
    item "ABIs" "$abis"
    local declared; declared="$(qt_declared_version ANDROID)"
    local abi kit rust_target
    local IFS=';'
    for abi in $abis; do
        case "$abi" in
            arm64-v8a)   kit="android_arm64_v8a"; rust_target="aarch64-linux-android" ;;
            x86_64)      kit="android_x86_64";    rust_target="x86_64-linux-android" ;;
            # NOT thumbv7neon: Qt's toolchain sets CMAKE_ANDROID_ARM_MODE, so
            # corrosion maps armeabi-v7a to the plain armv7 target.
            armeabi-v7a) kit="android_armv7";     rust_target="armv7-linux-androideabi" ;;
            x86)         kit="android_x86";       rust_target="i686-linux-android" ;;
            *)           advise "unrecognised ABI '$abi' -- cannot check its kit or Rust target"; continue ;;
        esac
        local kit_path="$HOME/Qt/$declared/$kit"
        if [ -d "$kit_path" ]; then
            ok "Qt kit for $abi: $kit_path"
        else
            critical "Qt kit for $abi NOT installed: $kit_path"
        fi
        unset IFS
        check_rust_target "$rust_target" "for ABI $abi" critical
        IFS=';'
    done
    unset IFS
}

report_macos() {
    section "macOS toolchain"
    tool_version "xcodebuild" xcodebuild xcodebuild -version
    if command -v xcrun >/dev/null 2>&1; then
        item "macOS SDK" "$(xcrun --sdk macosx --show-sdk-path 2>/dev/null || echo '(xcrun failed)')"
        item "SDK version" "$(xcrun --sdk macosx --show-sdk-version 2>/dev/null || echo '(unknown)')"
    else
        critical "xcrun not found -- Xcode command line tools are required"
    fi
    local declared; declared="$(qt_declared_version MACOS)"
    local macdeployqt="$HOME/Qt/$declared/macos/bin/macdeployqt"
    if [ -x "$macdeployqt" ]; then
        ok "macdeployqt present: $macdeployqt"
    else
        critical "macdeployqt not found at $macdeployqt (needed to bundle Qt into the .app)"
    fi
}

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------
printf '%s=== Build environment verification ===%s\n' "$_c_bold" "$_c_off"

if [ "$mode" = "all" ]; then
    report_common
    check_reader_agreement
    for p in LINUX MACOS WINDOWS ANDROID IOS; do
        item "QT_$p" "$(qt_declared_version "$p")"
    done
else
    case "$platform" in
        linux)
            report_common
            check_qt_kit LINUX "$HOME/Qt/$(qt_declared_version LINUX)/gcc_64" qmake6
            check_reader_agreement
            check_rust_target "x86_64-unknown-linux-gnu" "desktop build" critical
            ;;
        macos)
            report_common
            check_qt_kit MACOS "$HOME/Qt/$(qt_declared_version MACOS)/macos" qmake6
            check_reader_agreement
            report_macos
            ;;
        android)
            report_common
            # The primary ABI's kit is what supplies qt-cmake for the top-level
            # configure; per-ABI kits are checked in report_android.
            check_qt_kit ANDROID "$HOME/Qt/$(qt_declared_version ANDROID)/android_arm64_v8a" qmake
            check_reader_agreement
            report_android
            ;;
        "")
            echo "qt-env-verify: --platform or --all is required" >&2
            exit 2 ;;
        *)
            echo "qt-env-verify: unknown platform '$platform' (linux|macos|android)" >&2
            exit 2 ;;
    esac
fi

section "Result"
if [ "$critical_failures" -eq 0 ] && [ "$advisories" -eq 0 ]; then
    printf '  %sAll checks passed.%s\n' "$_c_grn" "$_c_off"
elif [ "$critical_failures" -eq 0 ]; then
    printf '  %s%d advisory warning(s), no critical failures -- continuing.%s\n' "$_c_yel" "$advisories" "$_c_off"
else
    printf '  %s%d CRITICAL failure(s)%s' "$_c_red" "$critical_failures" "$_c_off"
    [ "$advisories" -gt 0 ] && printf ' and %d advisory warning(s)' "$advisories"
    printf '.\n'
fi
printf '\n'

if [ "$critical_failures" -gt 0 ] && [ "$report_only" -eq 0 ]; then
    printf '%sBuild stopped: the environment cannot produce a correct build.%s\n' "$_c_red" "$_c_off" >&2
    printf 'Fix the CRITICAL items above, or re-run with --report-only to inspect without stopping.\n' >&2
    exit 1
fi

exit 0
