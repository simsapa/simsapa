# Crimson Pro Pāli glyph patch (ṁ / Ṁ)

The four `assets/fonts/crimson-pro/*.ttf` faces shipped in this repo are **not
stock Crimson Pro**: they have been patched with
`scripts/patch_crimson_pro_pali_glyphs.py` to add two glyphs the upstream font
lacks:

- **U+1E41 ṁ** (m with dot above — the Pāli niggahita in the MS edition)
- **U+1E40 Ṁ** (its uppercase)

If the font files are ever replaced with fresh upstream copies, re-run the
patch script (it is idempotent) and rebuild.

## The symptom

On a Pāli sutta page served to a **regular browser**
(e.g. `http://localhost:4848/get_sutta_html_by_uid/...`), every `ṁ` in the
body text (font `Crimson Pro SSP`) rendered with an oversized, mismatched dot,
while the same page looked correct in the app's own Qt WebEngine view, and the
title (Abhaya Libre) was fine everywhere.

## Root cause

Stock Crimson Pro has `ṃ` (U+1E43, dot below) but **no glyph for ṁ (U+1E41)**.

- A plain browser does per-character font fallback to a system font for the
  missing glyph → the visibly mismatched dot.
- Qt WebEngine happened to fall back differently (the font has base `m` and
  combining dot above U+0307, which it composed acceptably), masking the
  problem in-app.

## Why the first fix attempt broke the small caps

A first attempt added only the lowercase U+1E41. That fixed the body text but
broke the small-caps opening line ("Evaṁ me sutaṁ", class `.evam` with
`font-variant: small-caps` in `assets/sass/_suttacentral.sass`).

Crimson Pro has **no `smcp` OpenType feature**, so the browser *synthesizes*
small caps by scaling down **uppercase** glyphs. Rendering `ṁ` in small caps
therefore needs the capital **Ṁ (U+1E40)**. Before any patch, the whole
character fell back to a system font (ugly but small-caps-shaped); once the
font claimed U+1E41 the browser stopped falling back — and had no Ṁ to
synthesize from. **Both** codepoints must be added together.

## How the patch builds the glyphs

The script mirrors the font's own construction of ṅ/Ṅ:

| New glyph | Composite | Modeled on |
|---|---|---|
| `uni1E41` (ṁ) | `m` + `uni0307` | `uni1E45` (ṅ) = `n` + `uni0307` |
| `uni1E40` (Ṁ) | `M` + `uni0307.case` | `uni1E44` (Ṅ) = `N` + `uni0307.case` |

Using the `.case` mark variant (with the font's own −73 y-offset) keeps the
capital's dot at the correct height. The dot x-offset per face is the ṅ/Ṅ
offset shifted by the bbox-center delta between the base letters (n→m, N→M),
which handles the italic slant by construction. Sanity check: the method
derives 114 for Regular's ṁ — identical to where the font places its own dot
*below* `m` in the shipped ṃ composite.

The script updates `glyf`, `cmap`, `hmtx` and the glyph order via fontTools;
all four faces are static TTFs (no `fvar`/`gvar`), so plain composites are
sufficient.

## Rebuild required

The fonts are embedded in the binary by
`include_dir!("$CARGO_MANIFEST_DIR/../assets/")` in `bridges/src/api.rs`, so
font file changes only take effect after `make build -B` and an app restart.
Browsers also cache font files — hard-reload (Ctrl+Shift+R) when verifying.
