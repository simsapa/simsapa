#!/usr/bin/env bash
#
# Generates the BETA Android launcher icon set into android/res-beta/ from
# assets/icons/appicons/simsapa-beta_w512.png.
#
# The beta package (io.github.simsapa.app.beta) installs alongside the released
# app, so it needs its own icon or the two are indistinguishable on the home
# screen. android/build.gradle adds res-beta/ to the beta variant's res source
# dirs, where it overrides the same-named mipmap resources from android/res/.
# See docs/android-beta-distribution-and-play-policy.md.
#
# Idempotent: re-run after changing the source art. Requires ImageMagick 7.
#
# GEOMETRY — derived from the shipped release icons, not invented, so the two
# marks sit identically on the launcher:
#
#   Both source PNGs (simsapa_w512.png and simsapa-beta_w512.png) trim to the
#   same 440x440 box at +36+36 in a 512 canvas, i.e. the beta art was drawn on
#   the release grid and the "B" badge fits inside the same box. So the release
#   transform applies unchanged.
#
#   Adaptive foreground: the release foreground has 234x234 of content centred
#   in a 432 canvas (54.2%). Scaling the whole 512 source by 234/440 gives
#   272/432 = 0.6296 of the canvas edge. Content then lands well inside the
#   66.7% safe zone, so no launcher mask can clip the badge.
#
#   Legacy icon: what Android itself derives from an adaptive icon — crop the
#   central 72/108 of the canvas and mask it round. Content is therefore
#   0.5417 / 0.6667 = 0.8125 of the legacy edge, which for the full 512 source
#   is 0.8125 * 512/440 = 0.9455.
#
# Two deliberate conventions copied from the release set rather than "fixed":
#
#   - ic_launcher_monochrome.png is a byte-identical copy of the foreground
#     (that is how the release set ships). The themed-icon renderer uses only
#     the alpha channel, so this yields a filled silhouette. Diverging here
#     would make the beta themed icon inconsistent with the release one; if the
#     release monochrome is ever redrawn as a proper silhouette, redo both.
#   - The background layer is a solid #FAE6B2 and is copied verbatim from
#     android/res/, so beta and release share a background.
#
# The legacy ic_launcher.png is effectively unused at minSdkVersion 28 — the
# mipmap-anydpi-v26 adaptive icon wins on every supported device — but it is
# generated anyway to keep the resource set complete.

set -euo pipefail

cd "$(dirname "$0")/.."

SRC="assets/icons/appicons/simsapa-beta_w512.png"
RELEASE_RES="android/res"
BETA_RES="android/res-beta"

command -v magick >/dev/null 2>&1 \
    || { echo "ERROR: ImageMagick 7 (magick) not found." >&2; exit 1; }
[ -f "$SRC" ] || { echo "ERROR: source art not found: $SRC" >&2; exit 1; }

# density:adaptive_canvas:legacy_size
DENSITIES="mdpi:108:48 hdpi:162:72 xhdpi:216:96 xxhdpi:324:144 xxxhdpi:432:192"

FOREGROUND_RATIO=0.6296   # source edge / adaptive canvas edge
LEGACY_RATIO=0.9455       # source edge / legacy canvas edge

for entry in $DENSITIES; do
    density="${entry%%:*}"
    rest="${entry#*:}"
    canvas="${rest%%:*}"
    legacy="${rest##*:}"

    out_dir="$BETA_RES/mipmap-$density"
    mkdir -p "$out_dir"

    fg_size=$(awk "BEGIN { printf \"%d\", ($canvas * $FOREGROUND_RATIO) + 0.5 }")
    lg_size=$(awk "BEGIN { printf \"%d\", ($legacy * $LEGACY_RATIO) + 0.5 }")

    # --- adaptive foreground: art centred on a transparent 108dp canvas ------
    magick "$SRC" \
        -resize "${fg_size}x${fg_size}" \
        -background none -gravity center -extent "${canvas}x${canvas}" \
        "$out_dir/ic_launcher_foreground.png"

    # --- monochrome: identical to the foreground (release convention) --------
    cp "$out_dir/ic_launcher_foreground.png" "$out_dir/ic_launcher_monochrome.png"

    # --- background: the solid colour, taken from the release set ------------
    cp "$RELEASE_RES/mipmap-$density/ic_launcher_background.png" \
       "$out_dir/ic_launcher_background.png"

    # --- legacy icon: round background + art, as Android would compose it ----
    background_colour=$(magick "$RELEASE_RES/mipmap-$density/ic_launcher_background.png" \
                            -format "#%[hex:p{0,0}]" info: | cut -c1-7)
    radius=$(awk "BEGIN { printf \"%.1f\", ($legacy / 2.0) - 0.5 }")
    centre=$(awk "BEGIN { printf \"%.1f\", ($legacy / 2.0) - 0.5 }")

    magick -size "${legacy}x${legacy}" xc:none \
        -fill "$background_colour" -draw "circle $centre,$centre $centre,0" \
        \( "$SRC" -resize "${lg_size}x${lg_size}" \) \
        -gravity center -composite \
        "$out_dir/ic_launcher.png"

    echo "  $out_dir  (adaptive ${canvas}px, art ${fg_size}px; legacy ${legacy}px, art ${lg_size}px)"
done

# The adaptive-icon descriptor is identical to the release one — same layer
# names, which is exactly why res-beta/ can override the layers alone. Copied
# rather than referenced so the beta set is self-contained.
mkdir -p "$BETA_RES/mipmap-anydpi-v26"
cp "$RELEASE_RES/mipmap-anydpi-v26/ic_launcher.xml" \
   "$BETA_RES/mipmap-anydpi-v26/ic_launcher.xml"
echo "  $BETA_RES/mipmap-anydpi-v26/ic_launcher.xml"

echo
echo "Done. Rebuild with 'make android-beta-debug' or 'make android-beta-dist'."
