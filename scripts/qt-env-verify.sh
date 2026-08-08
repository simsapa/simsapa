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

# The PowerShell reader, checked separately from the bash/make pair above
# because it needs an interpreter this project's dev machine may not have.
#
# build-windows.ps1 -Help exits right after Get-QtVersion and prints the derived
# default kit path, so this exercises the REAL derivation without building.
check_powershell_reader() {
    section "PowerShell reader (build-windows.ps1)"
    local declared; declared="$(qt_declared_version WINDOWS)"
    local ps; ps="$(command -v pwsh || command -v powershell || true)"
    local script="$root/build-windows.ps1"

    if [ ! -f "$script" ]; then
        critical "build-windows.ps1 not found at $script"
        return
    fi

    if [ -z "$ps" ]; then
        # SKIP loudly. A silently-skipped check is indistinguishable from a
        # passing one, which is the failure mode this whole script is against.
        advise "no PowerShell on this host -- Get-QtVersion NOT executed"
        if grep -qE '^[[:space:]]*set\(QT_WINDOWS[[:space:]]+"'"$declared"'"\)' "$root/CMakeLists.txt"; then
            item "" "Fallback only: the regex Get-QtVersion uses still matches CMakeLists.txt."
            item "" "That proves the PATTERN, not the script. Task 2.14 is the real test."
        else
            critical "the pattern Get-QtVersion greps for no longer matches CMakeLists.txt"
        fi
        return
    fi

    item "powershell" "$ps"
    local out
    out="$("$ps" -NoProfile -File "$script" -Help 2>&1)" || true
    if printf '%s' "$out" | grep -qF "C:\\Qt\\$declared\\msvc2022_64"; then
        ok "PowerShell reader derives Qt $declared from CMakeLists.txt"
    else
        critical "PowerShell reader did not report the declared Qt $declared"
        item "" "build-windows.ps1 -Help output did not contain C:\\Qt\\$declared\\msvc2022_64"
    fi
}

# The files that DERIVE the Qt version must not contain one as a literal. A
# reacquired hardcode is how the single-source property is lost -- silently, and
# only visibly wrong once two platforms target different versions.
#
# Deliberately narrow: only literals EQUAL to a currently-declared Qt version,
# only on non-comment lines, only in these files. Anything looser drowns in NDK,
# Gradle, AGP and crate versions, which are unrelated and correct.
# CMakeLists.txt is excluded because it is the source of the declarations.
check_no_hardcoded_versions() {
    section "No reacquired hardcodes in the deriving files"
    local files="Makefile build-android.sh build-appimage.sh build-macos.sh build-windows.ps1 scripts/qt-env.sh"
    local versions="" p v f line found=0
    for p in LINUX MACOS WINDOWS ANDROID IOS; do
        v="$(qt_declared_version "$p")"
        case " $versions " in *" $v "*) ;; *) versions="$versions $v" ;; esac
    done
    item "declared versions" "${versions# }"

    for f in $files; do
        [ -f "$root/$f" ] || { advise "$f not found -- cannot check for hardcodes"; continue; }
        for v in $versions; do
            # Strip comments before matching, so the explanatory comments that
            # name a version (there are several, and they are useful) do not
            # register as hardcodes.
            line="$(sed -e 's/#.*//' "$root/$f" | grep -nF "$v" | head -n3)"
            if [ -n "$line" ]; then
                critical "$f contains the literal Qt version $v on a non-comment line"
                printf '%s\n' "$line" | while IFS= read -r l; do item "" "$l"; done
                found=1
            fi
        done
    done
    [ "$found" -eq 0 ] && ok "no deriving file hardcodes a declared Qt version"
}

# Cheap syntax check. These scripts gate every build, so a syntax error in one
# of them stops all packaging -- and `bash -n` costs milliseconds.
check_script_syntax() {
    section "Script syntax"
    local bad=0 f
    for f in build-android.sh build-appimage.sh build-macos.sh \
             scripts/qt-env.sh scripts/qt-env-check.sh scripts/qt-env-verify.sh; do
        [ -f "$root/$f" ] || { advise "$f not found"; continue; }
        if ! bash -n "$root/$f" 2>/dev/null; then
            critical "bash -n failed for $f"
            bash -n "$root/$f" 2>&1 | head -n3 | while IFS= read -r l; do item "" "$l"; done
            bad=1
        fi
    done
    [ "$bad" -eq 0 ] && ok "all bash scripts parse"

    local ps; ps="$(command -v pwsh || command -v powershell || true)"
    if [ -z "$ps" ]; then
        advise "no PowerShell on this host -- build-windows.ps1 NOT parse-checked"
    elif "$ps" -NoProfile -Command \
            '$ErrorActionPreference="Stop"; [void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path $args[0]), [ref]$null, [ref]$e); if ($e) { exit 1 }' \
            "$root/build-windows.ps1" >/dev/null 2>&1; then
        ok "build-windows.ps1 parses"
    else
        critical "build-windows.ps1 failed to parse"
    fi
}

# List the entries of a colon-separated path list that live under a Qt install
# of some version OTHER than the one given. Used to catch a desktop Qt leaking
# into the Android cross-build via PATH / LD_LIBRARY_PATH.
#
# Matches any .../Qt/<version>/... layout, which is what both the Qt installer
# ($HOME/Qt) and /opt/Qt produce; anything else is not recognisably a Qt kit and
# is left alone rather than guessed at.
foreign_qt_entries() {
    local want="$1" list="$2" entry ver out=""
    [ -n "$list" ] || return 0
    local IFS=':'
    for entry in $list; do
        case "$entry" in
            */Qt/*)
                ver="${entry#*/Qt/}"
                ver="${ver%%/*}"
                [ "$ver" = "$want" ] || out="$out $entry"
                ;;
        esac
    done
    unset IFS
    printf '%s' "$out"
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

    # -----------------------------------------------------------------------
    # Host tools, and the desktop-Qt leak this section exists to catch.
    #
    # The cross-build runs the ANDROID Qt's host tools -- moc, rcc,
    # androiddeployqt from $HOME/Qt/<QT_ANDROID>/gcc_64, resolved automatically
    # via __qt_platform_initial_qt_host_path, which is why nothing sets
    # QT_HOST_PATH. They are dynamically linked against libQt6Core.so.6.
    #
    # So a DESKTOP kit on LD_LIBRARY_PATH (exported by scripts/qt-env.sh via
    # .envrc or .claude/settings.json) makes a QT_ANDROID moc/rcc load a
    # QT_LINUX libQt6Core. build-android.sh scrubs both variables before
    # calling this gate; this check is the independent backstop, because the
    # failure is invisible while QT_ANDROID == QT_LINUX and appears only once
    # they diverge -- i.e. exactly when it is least expected.
    # -----------------------------------------------------------------------
    section "Android host tools"
    local host_kit="$HOME/Qt/$declared/gcc_64"
    item "host kit" "$host_kit"
    if [ ! -d "$host_kit" ]; then
        critical "Qt $declared host kit NOT installed: $host_kit"
        item "" "The Android build needs its OWN gcc_64 kit for moc/rcc/androiddeployqt."
    else
        local host_qmake="$host_kit/bin/qmake6"
        [ -x "$host_qmake" ] || host_qmake="$host_kit/bin/qmake"
        if [ -x "$host_qmake" ]; then
            local host_ver; host_ver="$("$host_qmake" -query QT_VERSION 2>/dev/null)"
            if [ "$host_ver" != "$declared" ]; then
                critical "host kit at $host_kit reports Qt $host_ver, but QT_ANDROID declares $declared"
            else
                ok "host tools are Qt $host_ver, matching QT_ANDROID"
            fi
        else
            advise "no qmake in $host_kit/bin -- cannot verify the host tools' version"
        fi
    fi

    local foreign
    foreign="$(foreign_qt_entries "$declared" "${LD_LIBRARY_PATH:-}")"
    if [ -n "$foreign" ]; then
        critical "LD_LIBRARY_PATH carries a Qt other than $declared:$foreign"
        item "" "A host tool would load that Qt's libQt6Core. Unset LD_LIBRARY_PATH for Android builds."
    else
        ok "LD_LIBRARY_PATH carries no foreign Qt"
    fi
    foreign="$(foreign_qt_entries "$declared" "$PATH")"
    if [ -n "$foreign" ]; then
        # Advisory, not critical: the build addresses qt-cmake, rcc and moc by
        # absolute path, so a stray bin/ on PATH is far less likely to be
        # consulted than a stray lib/ is. Still worth reporting -- it means the
        # scrub in build-android.sh did not run.
        advise "PATH carries a Qt other than $declared:$foreign"
    else
        ok "PATH carries no foreign Qt"
    fi
    if [ -n "${QT_PREFIX:-}" ]; then
        advise "QT_PREFIX is still set ($QT_PREFIX) -- build-android.sh should have unset it"
    fi
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
    # Repo-wide consistency: five sections, in the order a reader wants them.
    #   1. the declarations parse at all
    #   2. every reader agrees with them (bash, make, PowerShell)
    #   3. no deriving file has reacquired a hardcode
    #   4. the scripts that enforce all of the above still parse
    #   5. which kits are actually installed
    report_common

    section "Qt version declarations"
    for p in LINUX MACOS WINDOWS ANDROID IOS; do
        v="$(qt_declared_version "$p")"
        if [ -z "$v" ]; then
            critical "QT_$p could not be read from CMakeLists.txt"
        else
            item "QT_$p" "$v"
        fi
    done

    check_reader_agreement
    check_powershell_reader
    check_no_hardcoded_versions
    check_script_syntax

    # Kit availability. Only the host platform's kit is required -- the others
    # cannot be installed here and their absence is not a fault.
    # check_qt_kit prints its own section header.
    case "$(uname -s)" in
        Linux)  check_qt_kit LINUX "$HOME/Qt/$(qt_declared_version LINUX)/gcc_64" qmake6 ;;
        Darwin) check_qt_kit MACOS "$HOME/Qt/$(qt_declared_version MACOS)/macos" qmake6 ;;
    esac
    section "Qt kits installed"
    for p in LINUX MACOS ANDROID; do
        case "$p" in
            LINUX)   kit="$HOME/Qt/$(qt_declared_version LINUX)/gcc_64" ;;
            MACOS)   kit="$HOME/Qt/$(qt_declared_version MACOS)/macos" ;;
            ANDROID) kit="$HOME/Qt/$(qt_declared_version ANDROID)/android_arm64_v8a" ;;
        esac
        if [ -d "$kit" ]; then item "QT_$p kit" "$kit"; else item "QT_$p kit" "(not installed: $kit)"; fi
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
