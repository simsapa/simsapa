"""Add U+1E41 (m dot above) and U+1E40 (M dot above) to Crimson Pro faces.

Composites mirror the font's own ṅ/Ṅ construction (n + uni0307 / N + uni0307.case).
The dot x-offset for m/M is the ṅ/Ṅ offset shifted by the bbox-center delta
between the base letters, which reproduces the font's own dotbelow-on-m
placement exactly (Regular: 113.5 ≈ shipped 114).

Usage (from the repo root, needs fontTools):

    python3 scripts/patch_crimson_pro_pali_glyphs.py

Idempotent — already-patched faces are skipped. The fonts are embedded in the
binary via include_dir (bridges/src/api.rs), so run `make build -B` afterwards.
See docs/crimson-pro-pali-glyph-patch.md for the background.
"""
import copy
import glob

from fontTools.ttLib import TTFont

def bbox_center(glyf, name):
    g = glyf[name]
    g.recalcBounds(glyf)
    return (g.xMin + g.xMax) / 2

def add_composite(font, new_name, model_name, base_from, base_to, codepoint):
    glyf = font["glyf"]
    hmtx = font["hmtx"]
    if new_name in glyf.keys():
        print(f"  {new_name}: already present, skipping")
        return
    model = glyf[model_name]
    assert model.isComposite()
    new_glyph = copy.deepcopy(model)
    delta = bbox_center(glyf, base_to) - bbox_center(glyf, base_from)
    for comp in new_glyph.components:
        if comp.glyphName == base_from:
            comp.glyphName = base_to
        else:
            comp.x = round(comp.x + delta)
    font.glyphOrder.append(new_name)
    glyf.glyphs[new_name] = new_glyph
    adv, lsb = hmtx[base_to]
    hmtx[new_name] = (adv, lsb)
    for table in font["cmap"].tables:
        if table.isUnicode():
            table.cmap[codepoint] = new_name
    comps = [(c.glyphName, c.x, c.y) for c in new_glyph.components]
    print(f"  {new_name}: U+{codepoint:04X} adv={adv} components={comps}")

for path in sorted(glob.glob("assets/fonts/crimson-pro/*.ttf")):
    print(path)
    font = TTFont(path)
    add_composite(font, "uni1E41", "uni1E45", "n", "m", 0x1E41)
    add_composite(font, "uni1E40", "uni1E44", "N", "M", 0x1E40)
    font.save(path)
    # verify reload
    check = TTFont(path)
    cmap = check.getBestCmap() or {}
    assert cmap.get(0x1E41) == "uni1E41" and cmap.get(0x1E40) == "uni1E40"
    print(f"  saved, numGlyphs={check['maxp'].numGlyphs}")
