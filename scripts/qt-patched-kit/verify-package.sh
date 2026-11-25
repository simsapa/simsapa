#!/usr/bin/env bash
# Prove that an APK or AAB carries the patched Qt code (or the stock code),
# not just that it built. build-android.sh runs it on every package built
# with the patched kit. See docs/qt-android-patched-kit.md.
#
#   [PATCHED_KIT_ROOT=<kit root>] scripts/qt-patched-kit/verify-package.sh <apk|aab> [--expect-patched|--expect-stock]
#
# Reads an APK (classes*.dex, lib/<abi>/) or an AAB (base/dex/, base/lib/<abi>/).
#
# Default is --expect-patched. Checks:
#   1. dexdump: org.qtproject.qt.android.QtInputConnection is defined exactly
#      once across all classes*.dex (a second copy, e.g. from an app's own
#      android/src, would make which one runs undefined).
#   2. the patch marker string ($MARKER, any version suffix) is in the dex
#      (patched) or absent (stock).
#   3. for every ABI in the package: the packaged
#      libplugins_platforms_qtforandroid_<abi>.so matches the patched sha256
#      from $PATCHED_KIT_ROOT/PATCHED-KIT-MANIFEST-<abi>.txt (patched; or the
#      stock sha256 if the patches don't touch the plugin), or that ABI's
#      stock kit file (stock). Gradle packages the already-stripped kit
#      plugin byte for byte, so the checksums are comparable. At least one
#      ABI must be packaged, and every ABI in EXPECT_ABIS (';' or ' '
#      separated) if set.
#   4. per ABI: the marker is in the packaged plugin (patched, if the
#      patches touch the plugin) or absent (stock).
#   5. fix signatures: call sites, per class and method, that only the fix
#      produces (from `dexdump -d`; a plain string search does not work,
#      since androidx also references controlWindowInsetsAnimation):
#        - f5c0296fdaad: QtInputConnection.updateFullScreenExtractedText
#          calls InputMethodManager.updateExtractedText;
#        - 5fd393cb69be: QtInputDelegate.showKeyboard calls
#          setWindowInsetsAnimationCallback and no longer calls
#          controlWindowInsetsAnimation;
#        - the Qt-local modifier-key fix: QtInputConnection.sendKeyEvent and
#          QtEditText.onKeyDown call KeyEvent.isModifierKey.
#      Patched: every signature present; stock: every one absent. The C++
#      backports 491ac58e3d24 and 83876bf5500b (longPress() iterator) have
#      no dex signature, and a one-line C++ change leaves no reliable
#      pattern in the stripped plugin; check 3 covers them (the plugin
#      matches the manifest, which lists the patches it was built from).
#      Set CHECK_FIX_SIGNATURES=0 to skip (e.g. for a marker-only kit).
# Prints PASS/FAIL per check and exits non-zero on any FAIL.
set -euo pipefail
source "$(dirname "$0")/env.sh"

APK="${1:-}"; MODE="${2:---expect-patched}"
[ -f "$APK" ] || { echo "usage: $0 <apk|aab> [--expect-patched|--expect-stock]" >&2; exit 2; }
case "$MODE" in --expect-patched|--expect-stock) ;; *) echo "bad mode: $MODE" >&2; exit 2 ;; esac

MARKER="${MARKER:-qt-thai-fix-marker}"
CLASS='Lorg/qtproject/qt/android/QtInputConnection;'

TMP="$(mktemp -d "${TMPDIR:-/tmp}/verify-package.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
# An AAB keeps the same files under base/dex/ and base/lib/.
if unzip -l "$APK" 'base/dex/classes*.dex' >/dev/null 2>&1; then PREFIX=base/; DEXDIR=base/dex/
else PREFIX=; DEXDIR=; fi
unzip -q "$APK" "${DEXDIR}classes*.dex" "${PREFIX}lib/*/libplugins_platforms_qtforandroid_*.so" -d "$TMP" \
    || { echo "error: could not extract the dex files and Qt platform plugins from $APK" >&2; exit 1; }
DEX=("$TMP/$DEXDIR"classes*.dex)

fails=0
result() {  # PASS|FAIL label detail
    printf '  %-4s  %-42s %s\n' "$1" "$2" "$3"
    [ "$1" = PASS ] || fails=$((fails + 1))
}

echo "==> $APK ($MODE)"

# 1. class defined exactly once
count=0
for dex in "${DEX[@]}"; do
    n="$("$DEXDUMP" "$dex" 2>/dev/null | grep -cF "Class descriptor  : '$CLASS'" || true)"
    count=$((count + n))
done
if [ "$count" -eq 1 ]; then result PASS "QtInputConnection defined once" "($count)"
else result FAIL "QtInputConnection defined once" "(found $count)"; fi

# 2. marker in dex
dex_marker="$(cat "${DEX[@]}" | grep -aoE "$MARKER[-a-z0-9]*" | sort -u | tr '\n' ' ' || true)"
if [ "$MODE" = --expect-patched ]; then
    if [ -n "$dex_marker" ]; then result PASS "marker in dex" "$dex_marker"
    else result FAIL "marker in dex" "(not found)"; fi
else
    if [ -z "$dex_marker" ]; then result PASS "no marker in dex" ""
    else result FAIL "no marker in dex" "found: $dex_marker"; fi
fi

# 3./4. platform plugin, per packaged ABI. Qt names the plugin after its
# ABI, so lib/<abi>/ must hold libplugins_platforms_qtforandroid_<abi>.so (a
# plugin of another ABI in that folder is build-android.sh's cross-ABI check).
abis=()
for dir in "$TMP/${PREFIX}lib"/*/; do
    [ -d "$dir" ] || continue          # unmatched glob
    abi="$(basename "$dir")"
    [ -f "$dir/libplugins_platforms_qtforandroid_$abi.so" ] && abis+=("$abi")
done
[ "${#abis[@]}" -gt 0 ] || result FAIL "plugin packaged" "(no lib/<abi>/libplugins_platforms_qtforandroid_<abi>.so)"
# EXPECT_ABIS (';' or ' ' separated): each must be packaged.
for want_abi in ${EXPECT_ABIS:+${EXPECT_ABIS//;/ }}; do
    if printf '%s\n' "${abis[@]}" | grep -qx -- "$want_abi"; then result PASS "[$want_abi] packaged" ""
    else result FAIL "[$want_abi] packaged" "(expected, not in the package)"; fi
done
for abi in "${abis[@]}"; do
    so="$TMP/${PREFIX}lib/$abi/libplugins_platforms_qtforandroid_$abi.so"
    if ! kit_dir="$(kit_dir_for_abi "$abi")"; then
        result FAIL "[$abi] known ABI" ""; continue
    fi
    plugin_rel="$(plugin_rel_for_abi "$abi")"
    manifest="${MANIFEST:-$(manifest_for_abi "$abi")}"
    got="$(sha256sum < "$so" | cut -d' ' -f1)"
    stock="$(sha256sum < "$QT_STOCK_ROOT/$kit_dir/$plugin_rel" | cut -d' ' -f1)"
    so_marker="$(grep -aoE "$MARKER[-a-z0-9]*" "$so" | sort -u | tr '\n' ' ' || true)"
    if [ "$MODE" = --expect-patched ]; then
        if [ ! -f "$manifest" ]; then
            result FAIL "[$abi] plugin = patched build" "(no manifest: $manifest)"; continue
        fi
        want="$(awk -v r="$plugin_rel" '$1 == r { print $3 }' "$manifest")"
        if [ -n "$want" ]; then
            if [ "$got" = "$want" ]; then result PASS "[$abi] plugin = patched build" "${got:0:16}…"
            else result FAIL "[$abi] plugin = patched build" "got ${got:0:16}…, want ${want:0:16}…"; fi
            if [ -n "$so_marker" ]; then result PASS "[$abi] marker in plugin" "$so_marker"
            else result FAIL "[$abi] marker in plugin" "(not found)"; fi
        else
            # The patches don't touch the plugin: it must be the stock one.
            if [ "$got" = "$stock" ]; then result PASS "[$abi] plugin = stock (not patched)" "${got:0:16}…"
            else result FAIL "[$abi] plugin = stock (not patched)" "got ${got:0:16}…, want ${stock:0:16}…"; fi
        fi
    else
        if [ "$got" = "$stock" ]; then result PASS "[$abi] plugin = stock kit" "${got:0:16}…"
        else result FAIL "[$abi] plugin = stock kit" "got ${got:0:16}…, want ${stock:0:16}…"; fi
        if [ -z "$so_marker" ]; then result PASS "[$abi] no marker in plugin" ""
        else result FAIL "[$abi] no marker in plugin" "found: $so_marker"; fi
    fi
done

# 5. fix signatures
if [ "${CHECK_FIX_SIGNATURES:-1}" = 1 ]; then
    # One line per call site in the Qt input classes: <class> <method> <callee>
    calls="$TMP/qt-input-calls.txt"
    for dex in "${DEX[@]}"; do "$DEXDUMP" -d "$dex" 2>/dev/null; done | awk '
        /^  Class descriptor  :/ { cls = $4; gsub(/\047/, "", cls); next }
        /^      name          :/ { m = $3; gsub(/\047/, "", m); next }
        cls ~ /^Lorg\/qtproject\/qt\/android\/Qt(InputConnection|EditText|InputDelegate)/ && /invoke-/ {
            t = $0; sub(/.*}, /, "", t); sub(/ \/\/.*/, "", t); print cls, m, t
        }' >| "$calls"
    # label | present-when-patched (1) or absent-when-patched (0) | ERE on a call line
    sigs=(
        "f5c0296fdaad updateExtractedText|1|^Lorg/qtproject/qt/android/QtInputConnection; updateFullScreenExtractedText .*InputMethodManager;\.updateExtractedText"
        "5fd393cb69be insets callback|1|^Lorg/qtproject/qt/android/QtInputDelegate; showKeyboard .*setWindowInsetsAnimationCallback"
        "5fd393cb69be no insets control|0|^Lorg/qtproject/qt/android/QtInputDelegate; showKeyboard .*controlWindowInsetsAnimation"
        "modifier fix: sendKeyEvent|1|^Lorg/qtproject/qt/android/QtInputConnection; sendKeyEvent .*KeyEvent;\.isModifierKey"
        "modifier fix: onKeyDown|1|^Lorg/qtproject/qt/android/QtEditText; onKeyDown .*KeyEvent;\.isModifierKey"
    )
    for s in "${sigs[@]}"; do
        label="${s%%|*}"; rest="${s#*|}"; when_patched="${rest%%|*}"; re="${rest#*|}"
        if grep -qE "$re" "$calls"; then found=1; else found=0; fi
        # the state the fix produces: found == when_patched
        if [ "$found" = "$when_patched" ]; then is_fixed=1; else is_fixed=0; fi
        if [ "$MODE" = --expect-patched ]; then
            if [ "$is_fixed" = 1 ]; then result PASS "sig $label" ""
            else result FAIL "sig $label" "(stock code)"; fi
        else
            if [ "$is_fixed" = 0 ]; then result PASS "stock: $label" ""
            else result FAIL "stock: $label" "(patched code)"; fi
        fi
    done
fi

if [ "$fails" -eq 0 ]; then echo "==> PASS"; else echo "==> FAIL ($fails check(s))"; fi
[ "$fails" -eq 0 ]
