#!/usr/bin/env bash
# Regenerate the measurements behind docs/android-api-levels-and-feature-dependencies.md.
#
# Answers two questions, both by measurement rather than by assumption:
#
#   1. What is the app's REAL minimum Android API level? Every .so in a built
#      APK is checked for imported (undefined) symbols that do not exist below a
#      given level. A GLOBAL undefined symbol is a hard floor -- the library
#      cannot be dlopen()ed on a device below it, and the app dies at load time.
#      A WEAK one is not a floor: it resolves to null and the caller falls back.
#
#   2. Which Android APIs does our own code call? Every JNI call site in cpp/
#      and backend/ is listed with its class and method, so the API level of
#      each can be looked up and recorded in the doc's §5 inventory. This half
#      is a SEARCH, not a verdict: Java-side API levels are not discoverable
#      from the binary and must come from the AOSP documentation.
#
# Nothing here changes any state; it only reads and reports. Safe to run any
# time, and worth running after a Qt upgrade, an NDK change, a new native crate,
# or a new JNI call site.
#
# Usage:
#   android-api-scan.sh                       # everything, auto-detected APK
#   android-api-scan.sh --apk path/to.apk     # a specific APK
#   android-api-scan.sh --floor 28            # test against a proposed minSdk
#   android-api-scan.sh --sections symbols    # one section only
#   android-api-scan.sh --markdown            # symbol tables in the doc's format
#
# --markdown affects the symbol scan's two tables (the ones §2 of the doc
# carries verbatim); the other sections stay plain text, being search results a
# human turns into inventory rows rather than tables to paste.
#
# Sections: declared, qt, symbols, jni, manifest  (comma-separated, or "all")
#
# See docs/android-api-levels-and-feature-dependencies.md -- this script is the
# tool its §2.2 and §9 refer to.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$script_dir")"

apk=""
floor=""
abi="arm64-v8a"
ndk_root="${ANDROID_NDK_ROOT:-}"
sections="all"
markdown=0

while [ $# -gt 0 ]; do
    case "$1" in
        --apk) apk="${2:?--apk needs a value}"; shift 2 ;;
        --apk=*) apk="${1#*=}"; shift ;;
        --floor) floor="${2:?--floor needs a value}"; shift 2 ;;
        --floor=*) floor="${1#*=}"; shift ;;
        --abi) abi="${2:?--abi needs a value}"; shift 2 ;;
        --abi=*) abi="${1#*=}"; shift ;;
        --ndk) ndk_root="${2:?--ndk needs a value}"; shift 2 ;;
        --ndk=*) ndk_root="${1#*=}"; shift ;;
        --sections) sections="${2:?--sections needs a value}"; shift 2 ;;
        --sections=*) sections="${1#*=}"; shift ;;
        --markdown) markdown=1; shift ;;
        -h|--help) sed -n '2,36p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "android-api-scan: unknown argument: $1" >&2; exit 2 ;;
    esac
done

_c_red=$'\033[31m'; _c_grn=$'\033[32m'; _c_yel=$'\033[33m'; _c_bold=$'\033[1m'; _c_off=$'\033[0m'
[ -t 1 ] || { _c_red=""; _c_grn=""; _c_yel=""; _c_bold=""; _c_off=""; }
[ "$markdown" -eq 1 ] && { _c_red=""; _c_grn=""; _c_yel=""; _c_bold=""; _c_off=""; }

section() { if [ "$markdown" -eq 1 ]; then printf '\n### %s\n\n' "$1"; else printf '\n%s%s%s\n' "$_c_bold" "$1" "$_c_off"; fi; }
item()    { printf '  %-26s %s\n' "$1" "$2"; }
note()    { printf '  %s\n' "$1"; }
ok()      { printf '  %sOK%s       %s\n' "$_c_grn" "$_c_off" "$1"; }
bad()     { printf '  %sHARD%s     %s\n' "$_c_red" "$_c_off" "$1"; }
warn()    { printf '  %sWARN%s     %s\n' "$_c_yel" "$_c_off" "$1"; }

want() {
    [ "$sections" = "all" ] && return 0
    case ",$sections," in *",$1,"*) return 0 ;; *) return 1 ;; esac
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# ---------------------------------------------------------------------------
# Section: declared levels -- what the build says, from the single sources.
# ---------------------------------------------------------------------------
declared_min=""
declared_target=""

read_declared() {
    declared_min="$(sed -n 's/^[[:space:]]*minSdkVersion[[:space:]]*\([0-9]*\).*/\1/p' \
        "$root/android/build.gradle" | head -n1)"
    declared_target="$(sed -n 's/^[[:space:]]*targetSdkVersion[[:space:]]*\([0-9]*\).*/\1/p' \
        "$root/android/build.gradle" | head -n1)"
}
read_declared

report_declared() {
    section "Declared levels"
    item "minSdkVersion" "${declared_min:-<unreadable>}  (android/build.gradle defaultConfig -- the ONLY source)"
    item "targetSdkVersion" "${declared_target:-<unreadable>}  (android/build.gradle defaultConfig)"

    if grep -q "uses-sdk" "$root/android/AndroidManifest.xml" 2>/dev/null; then
        warn "AndroidManifest.xml now has a <uses-sdk> element -- it did not before;"
        note "         androiddeployqt validates that one and rejects minSdk < Qt's floor."
    else
        item "manifest <uses-sdk>" "absent (so androiddeployqt's own minSdk guard never fires)"
    fi

    item "QT_ANDROID" "$(sed -n 's/^[[:space:]]*set(QT_ANDROID[[:space:]]*"\([^"]*\)").*/\1/p' \
        "$root/CMakeLists.txt" | head -n1)"
    item "NDK in use" "$(basename "${ndk_root:-<unset ANDROID_NDK_ROOT>}")"
    note ""
    note "  Verify a BUILT artifact with:  aapt2 dump badging <apk> | grep -i sdkversion"
    note "  Never read android-build/gradle.properties -- build.gradle does not read it."
}

# ---------------------------------------------------------------------------
# Section: Qt's own floor -- three independent statements of it in the kit.
# ---------------------------------------------------------------------------
report_qt() {
    section "Qt's declared Android floor"

    local qt_ver kit src
    qt_ver="$(sed -n 's/^[[:space:]]*set(QT_ANDROID[[:space:]]*"\([^"]*\)").*/\1/p' \
        "$root/CMakeLists.txt" | head -n1)"
    kit="$HOME/Qt/$qt_ver/android_${abi//-/_}"
    src="$HOME/Qt/$qt_ver/Src/qtbase/src/tools/androiddeployqt/main.cpp"

    item "Qt version" "$qt_ver"
    item "kit" "$kit"

    if [ -f "$kit/lib/cmake/Qt6/qt.toolchain.cmake" ]; then
        item "ANDROID_PLATFORM" "$(sed -n 's/.*set(ANDROID_PLATFORM[[:space:]]*"\([^"]*\)".*/\1/p' \
            "$kit/lib/cmake/Qt6/qt.toolchain.cmake" | head -n1)  (the level Qt's own libs are COMPILED against)"
    else
        warn "kit not found -- cannot read ANDROID_PLATFORM"
    fi

    if [ -f "$src" ]; then
        item "androiddeployqt default" "$(sed -n 's/.*minSdkVersion{"\([0-9]*\)"}.*/\1/p' "$src" | head -n1)"
        local guard
        guard="$(grep -n 'minSdkVersion must be >=' "$src" | head -n1)"
        [ -n "$guard" ] && item "androiddeployqt guard" "main.cpp:${guard%%:*} -- ${guard#*:}"
    else
        note "  Qt sources not installed; skipping androiddeployqt evidence."
        note "  (Install the Qt Sources component to re-derive it.)"
    fi

    note ""
    note "  Qt re-evaluates its minimum ANNUALLY, at the autumn release, targeting"
    note "  90% cumulative market usage -- so the floor moves on a schedule, not with"
    note "  any particular version. Check the target version's own documentation:"
    note "    https://doc.qt.io/qt-6/android.html"
    note "    https://doc.qt.io/qt-6/android-supported-versions-selection-guidelines.html"
}

# ---------------------------------------------------------------------------
# Section: native symbol scan -- the measurement that produces the real floor.
# ---------------------------------------------------------------------------
sysroot_triple_for_abi() {
    case "$1" in
        arm64-v8a)   echo "aarch64-linux-android" ;;
        armeabi-v7a) echo "arm-linux-androideabi" ;;
        x86_64)      echo "x86_64-linux-android" ;;
        x86)         echo "i686-linux-android" ;;
        *)           echo "" ;;
    esac
}

find_apk() {
    # Newest APK under build/, which is where a local android build leaves one.
    find "$root/build" -name '*.apk' -printf '%T@ %p\n' 2>/dev/null \
        | sort -rn | head -n1 | cut -d' ' -f2-
}

report_symbols() {
    section "Native symbol scan"

    [ -n "$apk" ] || apk="$(find_apk)"
    if [ -z "$apk" ] || [ ! -f "$apk" ]; then
        warn "no APK found -- build one (make android-beta-debug) or pass --apk PATH"
        return
    fi

    if [ -z "$ndk_root" ] || [ ! -d "$ndk_root" ]; then
        # Fall back to the newest NDK the SDK has, the same way a build would.
        ndk_root="$(find "${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}/ndk" -maxdepth 1 -mindepth 1 -type d 2>/dev/null \
            | sort -V | tail -n1)"
    fi
    if [ -z "$ndk_root" ] || [ ! -d "$ndk_root" ]; then
        warn "NDK not found -- pass --ndk PATH or set ANDROID_NDK_ROOT"
        return
    fi

    local triple sysroot readelf
    triple="$(sysroot_triple_for_abi "$abi")"
    if [ -z "$triple" ]; then
        warn "unknown ABI: $abi"
        return
    fi
    sysroot="$ndk_root/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/$triple"
    readelf="$ndk_root/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-readelf"
    command -v "$readelf" >/dev/null 2>&1 || readelf="$(command -v llvm-readelf || command -v readelf)"

    if [ ! -d "$sysroot" ]; then
        warn "NDK sysroot not found: $sysroot"
        return
    fi

    [ -n "$floor" ] || floor="$declared_min"

    item "APK" "$apk"
    item "ABI" "$abi"
    item "NDK" "$(basename "$ndk_root")"
    item "readelf" "$readelf"
    item "floor under test" "API $floor"

    # Per-level export sets: the union of every stub library the NDK ships for
    # that level. A symbol absent from level N does not exist on a device at N.
    local levels lvl
    levels="$(find "$sysroot" -maxdepth 1 -mindepth 1 -type d -printf '%f\n' | grep -E '^[0-9]+$' | sort -n)"
    for lvl in $levels; do
        for l in "$sysroot/$lvl"/*.so; do
            "$readelf" --dyn-syms --wide "$l" 2>/dev/null \
                | awk '$4=="FUNC" || $4=="OBJECT" {print $8}' | sed 's/@.*//'
        done | sort -u > "$tmp/plat-$lvl.txt"
    done

    # Symbols that do not exist at the floor.
    [ -f "$tmp/plat-$floor.txt" ] || { warn "NDK has no stub libraries for API $floor"; return; }

    rm -rf "$tmp/apk"; mkdir -p "$tmp/apk"
    unzip -q -o "$apk" "lib/$abi/*" -d "$tmp/apk" 2>/dev/null
    local libdir="$tmp/apk/lib/$abi"
    if [ ! -d "$libdir" ]; then
        warn "APK carries no lib/$abi/ -- wrong ABI, or a split APK"
        return
    fi
    item "libraries scanned" "$(find "$libdir" -name '*.so' | wc -l)"

    # Symbols any bundled library provides are resolved inside the APK and are
    # not a platform requirement at all.
    for f in "$libdir"/*.so; do
        "$readelf" --dyn-syms --wide "$f" 2>/dev/null \
            | awk '$7!="UND" && ($4=="FUNC" || $4=="OBJECT") {print $8}' | sed 's/@.*//'
    done | sort -u > "$tmp/apk-provided.txt"

    # Columns: Num: Value Size Type Bind Vis Ndx Name
    #                       $4   $5   $6  $7  $8
    #
    # Deliberately set operations (comm/awk), not a per-symbol grep loop: the
    # app library alone exports ~350k symbols, and the naive form takes minutes.
    : > "$tmp/hard.txt"; : > "$tmp/weak.txt"
    for f in "$libdir"/*.so; do
        local base; base="$(basename "$f")"
        "$readelf" --dyn-syms --wide "$f" 2>/dev/null \
            | awk '$7=="UND" {print $5 "\t" $8}' | sed 's/@[^\t]*$//' | sort -u > "$tmp/und.tsv"
        cut -f2 "$tmp/und.tsv" | sort -u > "$tmp/und-syms.txt"

        # Unresolved at the floor: imported, not provided inside the APK, and
        # absent from the platform at that level.
        comm -23 "$tmp/und-syms.txt" "$tmp/apk-provided.txt" \
            | comm -23 - "$tmp/plat-$floor.txt" > "$tmp/residual.txt"
        [ -s "$tmp/residual.txt" ] || continue

        # Re-attach each residual symbol's binding.
        awk -F'\t' 'NR==FNR {r[$1]; next} ($2 in r) {print $1 "\t" $2}' \
            "$tmp/residual.txt" "$tmp/und.tsv" > "$tmp/residual-bind.tsv"

        while IFS=$'\t' read -r bind sym; do
            [ -n "$sym" ] || continue
            # Lowest level that does provide it, so the report says how far above
            # the floor it is. The residual set is small, so this loop is cheap.
            local first="none"
            for lvl in $levels; do
                if grep -qx "$sym" "$tmp/plat-$lvl.txt"; then first="$lvl"; break; fi
            done
            if [ "$bind" = "WEAK" ]; then
                printf '%s\t%s\t%s\n' "$base" "$sym" "$first" >> "$tmp/weak.txt"
            else
                printf '%s\t%s\t%s\n' "$base" "$sym" "$first" >> "$tmp/hard.txt"
            fi
        done < "$tmp/residual-bind.tsv"
    done

    echo
    if [ -s "$tmp/hard.txt" ]; then
        note "HARD requirements above API $floor -- each one means the app CANNOT LOAD"
        note "on a device at API $floor (dlopen fails: cannot locate symbol):"
        echo
        if [ "$markdown" -eq 1 ]; then
            printf '| Library | Symbol | Binding | First available |\n|---|---|---|---|\n'
            awk -F'\t' '{printf "| `%s` | `%s` | **GLOBAL** | %s |\n", $1, $2, ($3=="none" ? "never" : "API " $3)}' "$tmp/hard.txt"
        else
            while IFS=$'\t' read -r lib sym first; do
                if [ "$first" = "none" ]; then
                    bad "$lib: $sym (never provided by bionic at any level)"
                else
                    bad "$lib: $sym (first available: API $first)"
                fi
            done < "$tmp/hard.txt"
        fi
    else
        ok "no hard (GLOBAL) undefined symbol requires more than API $floor"
    fi

    echo
    if [ -s "$tmp/weak.txt" ]; then
        note "WEAK references above API $floor -- NOT a floor: these resolve to null"
        note "and the caller takes a fallback path. Listed so they are not mistaken"
        note "for the hard ones above."
        echo
        if [ "$markdown" -eq 1 ]; then
            printf '| Library | Symbol | Binding | First available |\n|---|---|---|---|\n'
            awk -F'\t' '{printf "| `%s` | `%s` | WEAK | %s |\n", $1, $2, ($3=="none" ? "never in bionic" : "API " $3)}' "$tmp/weak.txt"
        else
            while IFS=$'\t' read -r lib sym first; do
                if [ "$first" = "none" ]; then
                    item "  weak" "$lib: $sym (never in bionic)"
                else
                    item "  weak" "$lib: $sym (API $first)"
                fi
            done < "$tmp/weak.txt"
        fi
    fi

    # System libraries linked as hard dependencies set their own floors:
    # libaaudio.so does not exist before API 26, for instance.
    echo
    note "System libraries the app links directly (each has its own introduction level):"
    "$readelf" -d --wide "$libdir"/libsimsapadhammareader_*.so 2>/dev/null \
        | sed -n 's/.*Shared library: \[\(lib[A-Za-z0-9_]*\.so\)\].*/\1/p' \
        | grep -v -- '-v8a\|_arm\|_x86\|c++_shared' | sort -u | sed 's/^/    /'
}

# ---------------------------------------------------------------------------
# Section: JNI call sites -- the source-side inventory for the doc's §5.
#
# This is a search, not a verdict. The API level of a Java class member is not
# recoverable from our sources; look each one up in the AOSP documentation and
# record it in the doc. The point of the script is that NOTHING IS MISSED when
# the inventory is refreshed.
# ---------------------------------------------------------------------------
report_jni() {
    section "JNI call sites (C++)"
    note "Qt's QJniObject calls. Look each method up and record its API level."
    echo
    grep -rn --include='*.cpp' --include='*.h' \
        -oE '(callStaticObjectMethod|callObjectMethod|callStaticMethod|callMethod<[^>]*>|getStaticObjectField|getField<[^>]*>)\("[^"]+"' \
        "$root/cpp" 2>/dev/null \
        | sed 's|^'"$root"'/||' | sed 's/(\"/  ->  /' | sed 's/"$//' | sort -u | sed 's/^/    /'

    section "Android/Java classes named in C++"
    grep -rn --include='*.cpp' --include='*.h' \
        -oE '"(android|java|org/qtproject)/[A-Za-z0-9/_$]+"' "$root/cpp" 2>/dev/null \
        | sed 's|^'"$root"'/||' | tr -d '"' \
        | awk -F: '{printf "%-42s %s:%s\n", $3, $1, $2}' | sort -u | sed 's/^/    /'

    section "Android intent actions and flags named anywhere"
    grep -rn --include='*.cpp' --include='*.rs' --include='*.qml' \
        -oE '"android\.(intent|settings|provider)\.[A-Za-z_.]+"' "$root/cpp" "$root/backend/src" "$root/bridges/src" "$root/bridges/assets/qml" 2>/dev/null \
        | sed 's|^'"$root"'/||' | sort -u -t: -k3 | sed 's/^/    /'

    section "JNI call sites (Rust)"
    note "backend/src/android_saf.rs and friends. Same rule: look the level up."
    note "The jni crate wraps its arguments over several lines, so the first two"
    note "quoted tokens after the call are shown: for call_static_method those are"
    note "class then method; for call_method the receiver is a variable, so the"
    note "first token is already the method."
    echo
    # Look ahead a few lines from each call and report the quoted tokens found.
    find "$root/backend/src" "$root/bridges/src" -name '*.rs' 2>/dev/null | sort | while read -r f; do
        awk -v file="${f#"$root"/}" '
            function flush_call() {
                if (start && toks != "") printf "    %s:%s  ->  %s\n", file, start, toks
                grab = 0; start = 0; toks = ""; ntok = 0
            }
            /call_(static_)?method[[:space:]]*\(/ { flush_call(); grab = 5; start = FNR }
            grab > 0 {
                line = $0
                while (match(line, /"[^"]*"/)) {
                    t = substr(line, RSTART + 1, RLENGTH - 2)
                    line = substr(line, RSTART + RLENGTH)
                    # Keep identifiers only: a JNI type signature starts with
                    # "(", and an error-message literal has spaces or braces.
                    if (t == "" || t ~ /^\(/ || t ~ /[ {}:]/) continue
                    toks = (toks == "" ? t : toks " , " t)
                    if (++ntok >= 2) { flush_call(); next }
                }
                if (--grab == 0) flush_call()
            }
            END { flush_call() }
        ' "$f"
    done
    echo
    note "Java classes named in Rust:"
    grep -rn --include='*.rs' -oE '"(android|java)/[A-Za-z0-9/_$]+"' \
        "$root/backend/src" "$root/bridges/src" 2>/dev/null \
        | sed 's|^'"$root"'/||' | tr -d '"' \
        | awk -F: '{printf "%-42s %s:%s\n", $3, $1, $2}' | sort -u | sed 's/^/    /'

    section "JNI exception hygiene"
    note "A Java method that does not exist on the device throws, and the pending"
    note "exception must be cleared or it surfaces as an unrelated crash later."
    echo
    item "C++ checkAndClearExceptions" "$(grep -rc 'checkAndClearExceptions' "$root/cpp"/*.cpp 2>/dev/null | grep -v ':0$' | tr '\n' ' ')"
    item "Rust exception_clear" "$(grep -rc 'exception_clear' "$root/backend/src"/*.rs 2>/dev/null | grep -v ':0$' | tr '\n' ' ')"
    note ""
    note "  Any cpp/ file that makes JNI calls and appears in NEITHER list is worth"
    note "  a look -- acceptable only while every call it makes is API 1."

    section "Native crates with an Android backend"
    grep -nE '^(cpal|ndk-context|jni|app_dirs|symphonia|flacenc|rubato)' "$root/backend/Cargo.toml" 2>/dev/null | sed 's/^/    /'
}

# ---------------------------------------------------------------------------
# Section: manifest -- permissions and features, which Play turns into filters.
# ---------------------------------------------------------------------------
report_manifest() {
    section "Manifest permissions"
    grep -oE '<uses-permission android:name="[^"]+"' "$root/android/AndroidManifest.xml" 2>/dev/null \
        | sed 's/.*name="//; s/"$//' | sort -u | sed 's/^/    /'

    section "Manifest features"
    note "Every <uses-feature> must be required=\"false\" unless the app truly cannot"
    note "run without the hardware -- a bare one defaults to REQUIRED and Play then"
    note "filters the app off every device lacking it (this is what removed the app"
    note "from Chromebooks once already)."
    echo
    grep -oE '<uses-feature android:name="[^"]+"[^/]*' "$root/android/AndroidManifest.xml" 2>/dev/null \
        | sed 's/<uses-feature //' | sed 's/^/    /'

    section "Shared external storage"
    note "WRITE_EXTERNAL_STORAGE is genuinely required on API 27-28 to write SHARED"
    note "external storage. It is deliberately absent because the app never does."
    note "This grep must stay empty:"
    echo
    # Code only. The manifest's own prose explains why the permission is absent,
    # and matching that comment would report the documentation as the defect.
    if { grep -rn --include='*.rs' --include='*.cpp' --include='*.h' --include='*.qml' \
            -E 'getExternalStorage|EXTERNAL_STORAGE|/sdcard' \
            "$root/backend" "$root/bridges" "$root/cpp" 2>/dev/null \
            | grep -v 'isExternalStorageEmulated\|isExternalStorageRemovable\|getExternalStorageState'
         grep -n 'uses-permission[^>]*EXTERNAL_STORAGE' "$root/android/AndroidManifest.xml" 2>/dev/null \
            | sed "s|^|$root/android/AndroidManifest.xml:|"
       } | sed 's|^'"$root"'/||' | sed 's/^/    /' | grep .; then
        warn "a direct shared-storage path appeared -- re-check the permission decision"
    else
        ok "no direct shared external storage use"
    fi
}

# ---------------------------------------------------------------------------

if [ "$markdown" -eq 1 ]; then
    printf '<!-- Generated by scripts/android-api-scan.sh on %s -->\n' "$(date '+%Y-%m-%d %H:%M:%S %Z')"
fi

want declared && report_declared
want qt       && report_qt
want symbols  && report_symbols
want jni      && report_jni
want manifest && report_manifest

echo
note "Record what changed in docs/android-api-levels-and-feature-dependencies.md."
exit 0
