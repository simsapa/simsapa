# Tasks: Side-by-Side Translation and Pāli View

Based on PRD: `2026-07-01-192905-prd---side-by-side-translation-view.md`

## Relevant Files

- `backend/src/app_settings.rs` - Defines `AppSettings` and its enums; add the new `TranslationPaliLayout` and `TranslationPaliOrder` enums, replace the `show_translation_and_pali_line_by_line` bool field, and set defaults (`impl Default for AppSettings`, line ~212).
- `backend/src/app_data.rs` - Holds `render_sutta_content()` (line ~282), `get_pali_for_translated()` (line ~193), `sutta_to_segments_json()` (line ~218), and the existing string getter/setter pattern for enum settings (`AnkiExportFormat` at ~1318). Add getters/setters for the two new settings and rewrite the render branch.
- `backend/src/helpers.rs` - Contains `bilara_line_by_line_html()` (line ~2125) and `bilara_content_json_to_html()` (line ~2108). Add the new side-by-side HTML builder here.
- `bridges/src/sutta_bridge.rs` - CXX-Qt bridge; replace the two `*_show_translation_and_pali_line_by_line` bridge fns (declarations ~1145/1148, impls ~3846/3851) with the new layout + order fns.
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - qmllint type stub; mirror the new bridge function signatures.
- `assets/qml/AppSettingsWindow.qml` - Settings UI; replace `show_line_by_line_checkbox` + description (lines ~610-624) with the RadioButton group and order control, and update the init block (~line 1211).
- `assets/sass/_suttacentral.sass` - Segment styling (`.segment .translated` / `.pali`, lines ~5-16); add the flexbox two-column side-by-side rules.
- `bridges/build.rs` - Only if any new QML file is added (none expected here — editing existing files).
- `backend/tests/test_render_sutta_content.rs` - Existing render tests; add side-by-side and order cases.
- `docs/` (new file, e.g. `docs/side-by-side-translation-view.md`) and `PROJECT_MAP.md` - Documentation to update.

### Notes

- Rust tests: `cd backend && cargo test` (single: `cargo test test_name`). The render tests need the real appdata DB — use the `SIMSAPA_DIR` path from `AGENTS.md`.
- Build check: `make build -B` (not raw cmake).
- Sass build: `make sass` (compiles `assets/sass/` → `assets/css/`).
- Per project memory: only run tests after **all** sub-tasks of a top-level task are done; skip `make qml-test` unless asked; skip tests for docs-only changes.
- QML: use the `Logger` module, never `console`.
- The three-way choice can be QML `RadioButton`s bound by string value to a single stored layout string, matching the existing `AnkiExportFormat` bridge convention (get/set as `String`/`QString`).

---

### Specs & dependencies for Task 1.0 (backend settings model)

- **New enums** (in `app_settings.rs`, `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]`, following `AnkiExportFormat`/`ThemeName` style):
  - `TranslationPaliLayout { OnlyTranslation, LineByLine, SideBySide }`, default `LineByLine`.
  - `TranslationPaliOrder { TranslationFirst, PaliFirst }`, default `TranslationFirst`.
- **Field change:** replace `show_translation_and_pali_line_by_line: bool` with `translation_pali_layout: TranslationPaliLayout` and add `translation_pali_order: TranslationPaliOrder`. No legacy-boolean migration (PRD §Resolved Decisions).
- The struct uses `#[serde(default)]`, so add matching `#[serde(default = "...")]`/`Default` coverage so missing keys deserialize to the defaults.

- [ ] 1.0 Add the `TranslationPaliLayout` and `translation_pali_order` settings to the backend settings model
  - [ ] 1.1 In `app_settings.rs`, define `enum TranslationPaliLayout { OnlyTranslation, LineByLine, SideBySide }` and `enum TranslationPaliOrder { TranslationFirst, PaliFirst }` with the standard derives (mirror `AnkiExportFormat`); add `#[derive(Default)]` + `#[default]` on `LineByLine` and `TranslationFirst`.
  - [ ] 1.2 In the `AppSettings` struct, remove `show_translation_and_pali_line_by_line: bool` and add `translation_pali_layout: TranslationPaliLayout` and `translation_pali_order: TranslationPaliOrder`.
  - [ ] 1.3 In `impl Default for AppSettings` (line ~212), replace `show_translation_and_pali_line_by_line: true` with `translation_pali_layout: TranslationPaliLayout::LineByLine` and add `translation_pali_order: TranslationPaliOrder::TranslationFirst`.
  - [ ] 1.4 Add string-conversion helpers on `AppSettings` (or free fns) for both enums — e.g. `translation_pali_layout_as_string()` / `set_translation_pali_layout_from_str()` and the order equivalents — following the `theme_name_as_string` / `set_theme_name_from_str` precedent, so the bridge can pass plain strings.
  - [ ] 1.5 Fix all remaining compile references to the removed bool field across the backend (`grep -rn show_translation_and_pali_line_by_line backend/`), leaving only the new fields.

---

### Specs & dependencies for Task 2.0 (AppData + bridge exposure)

- Depends on 1.0 (the enums + string helpers must exist).
- **Pattern to follow:** `AnkiExportFormat` — `AppData::get_anki_export_format() -> String` / `set_anki_export_format(&str)` (`app_data.rs:1318`), bridged as `get_anki_export_format() -> QString` / `set_anki_export_format(&QString)` (`sutta_bridge.rs:1020`).
- QML reads/writes plain strings: `"OnlyTranslation" | "LineByLine" | "SideBySide"` and `"TranslationFirst" | "PaliFirst"`.

- [ ] 2.0 Expose the two new settings through `AppData` and the `SuttaBridge` (plus the qmllint stub)
  - [ ] 2.1 In `app_data.rs`, replace `get_/set_show_translation_and_pali_line_by_line` (lines ~1593-1603) with `get_translation_pali_layout() -> String` / `set_translation_pali_layout(&str)` and `get_translation_pali_order() -> String` / `set_translation_pali_order(&str)`, using the string helpers from 1.4 and persisting via the existing app-settings save path.
  - [ ] 2.2 In `sutta_bridge.rs`, replace the two bridge declarations (lines ~1145/1148) and their impls (lines ~3846/3851) with `get_/set_translation_pali_layout` and `get_/set_translation_pali_order` (`QString` in/out), delegating to the `AppData` methods.
  - [ ] 2.3 In `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`, replace the old stub functions with the four new signatures returning/accepting simple string values (qmllint only).
  - [ ] 2.4 `grep` for any other callers of the old bridge/AppData names and update them.

---

### Specs & dependencies for Task 3.0 (Rust rendering)

- Depends on 1.0 (layout/order enums) and reuses 2.0 nothing directly (render reads `app_settings` cache).
- **Render branch** in `render_sutta_content()` (`app_data.rs:282`), currently a bool. New logic keyed on `translation_pali_layout`:
  - `OnlyTranslation` → existing standard path (`bilara_content_json_to_html`, or `content_html`/`content_plain` fallbacks).
  - `LineByLine` → existing `bilara_line_by_line_html` path (segmented + Pāli counterpart), else standard.
  - `SideBySide` → new builder (below).
- **Side-by-side builder** (new fn in `helpers.rs`, sibling to `bilara_line_by_line_html`): takes translated segments, Pāli segments, the template `IndexMap`, `show_references`, and an `order` flag. For each ordered template key, apply the template to the translated segment and to the Pāli segment **separately**, then wrap the two in a flex row so headings align with headings and footers with footers:
  `<div class='side-by-side-row'><div class='sbs-col sbs-left'>{tmpl(left)}</div><div class='sbs-col sbs-right'>{tmpl(right)}</div></div>`, where left/right are chosen by `order`. Preserve the existing ordered-keys logic (template-driven, union fallback, Pāli-only-segment safeguard).
- **Non-segmented translation** (`content_html`) in side-by-side: source the Pāli via `get_pali_for_translated()`. If found and it has segmented content, render its column from its segments; if found without segments, render its `content_html`; wrap the translation HTML and the Pāli HTML in the two flex columns (unaligned), honoring `order`. If **no** Pāli counterpart, render the translation full-width single column.
- **Pāli-only sutta** (`language == "pli"`): `get_pali_for_translated` already returns `None`, so side-by-side/line-by-line naturally degrade to the standard single-text render — verify no empty column is emitted.

- [ ] 3.0 Implement side-by-side rendering in the Rust HTML builders and branch `render_sutta_content()` on the new layout + order settings
  - [ ] 3.1 Add a `TranslationPaliOrder`-aware helper (or a `bool left_is_translation`) that the render code derives from `app_settings.translation_pali_order`.
  - [ ] 3.2 In `helpers.rs`, add `bilara_side_by_side_html(translated, pali, tmpl, show_references, left_is_translation)` that builds per-template-item two-column flex rows (reusing the ordered-keys + Pāli-only-safeguard logic from `bilara_line_by_line_html`).
  - [ ] 3.3 Add a helper for the **non-segmented** case that wraps two already-rendered HTML blocks (translation + Pāli) into the two-column flex layout, honoring order — and returns single-column when no Pāli block is available.
  - [ ] 3.4 Rewrite the branch in `render_sutta_content()` to `match` on `translation_pali_layout` (OnlyTranslation / LineByLine / SideBySide) instead of the old bool, wiring the order flag into the line-by-line and side-by-side paths.
  - [ ] 3.5 For `SideBySide` with segmented `content_json` + Pāli counterpart, call `bilara_side_by_side_html`; without a Pāli counterpart fall back to the standard translation render.
  - [ ] 3.6 For `SideBySide` with a non-segmented (`content_html`) translation, fetch the Pāli via `get_pali_for_translated`, render its column from segments (or its `content_html` fallback), and combine via 3.3; single-column when no Pāli found.
  - [ ] 3.7 Apply the `order` flag to the existing `LineByLine` path so translation/Pāli interleave order matches the setting (currently translated-then-pali in `bilara_line_by_line_html`).
  - [ ] 3.8 Confirm Pāli-only suttas render as a single text in all three modes (no empty second column).

---

### Specs & dependencies for Task 4.0 (CSS)

- Depends on 3.0 for the emitted class names (`side-by-side-row`, `sbs-col`, `sbs-left`, `sbs-right`).
- Edit `assets/sass/_suttacentral.sass`; the row is `display: flex` with each `.sbs-col` at `flex: 0 0 50%` / `max-width: 50%` and horizontal padding/gutter. Headings and footers use the same row class so they render as aligned columns. Keep two columns at all widths (no responsive collapse). Rebuild with `make sass`.

- [ ] 4.0 Add the flexbox two-column CSS for the side-by-side view (paragraphs, headings, footers)
  - [ ] 4.1 In `_suttacentral.sass`, add `.suttacentral .side-by-side-row` as a flex row and `.sbs-col` as two 50%-width columns with a gutter; ensure it stays two columns on narrow screens.
  - [ ] 4.2 Ensure headings (`h1`/`header`) and footers rendered inside `.side-by-side-row` inherit sensible typography (reuse existing `.segment .translated` / `.pali` sizing where appropriate).
  - [ ] 4.3 Verify the styling works in both `light` and `dark` themes (body theme classes already applied by the renderer).
  - [ ] 4.4 Run `make sass` to compile to `assets/css/`.

---

### Specs & dependencies for Task 5.0 (Settings UI)

- Depends on 2.0 (bridge fns available).
- Replace `show_line_by_line_checkbox` (+ its description Label) in the **Display** section (`AppSettingsWindow.qml` ~610-624). Use three `RadioButton`s in a `ButtonGroup`, each followed by an explanatory Label with the PRD copy. Add a second control (RadioButtons or a labelled ComboBox) for the order, **disabled** when the layout is `OnlyTranslation`. Keep the "Close and re-open the relevant sutta tabs to see the effect." reminder. Initialize from `SuttaBridge.get_translation_pali_layout()` / `get_translation_pali_order()` in the window init block (~line 1211). Persist on change via the setters.

- [ ] 5.0 Replace the line-by-line checkbox in `AppSettingsWindow.qml` with the three-way RadioButton selector and the order control
  - [ ] 5.1 Add a "Show Translation and Pāli:" Label and three RadioButtons (Only the translation / Line-by-line (only for segmented texts) / Side-by-side) in a shared `ButtonGroup`, each with its explanatory sub-text Label using the exact PRD copy.
  - [ ] 5.2 On each RadioButton toggle, call `SuttaBridge.set_translation_pali_layout("OnlyTranslation" | "LineByLine" | "SideBySide")`.
  - [ ] 5.3 Add the order control (Translation | Pāli vs Pāli | Translation) wired to `SuttaBridge.set_translation_pali_order(...)`, and bind its `enabled` to layout != OnlyTranslation.
  - [ ] 5.4 Keep/relocate the "Close and re-open the relevant sutta tabs to see the effect." reminder Label under the group.
  - [ ] 5.5 In the settings init block (~line 1211), set the checked RadioButton and the order control from `SuttaBridge.get_translation_pali_layout()` / `get_translation_pali_order()`.
  - [ ] 5.6 Use the `Logger` module (not `console`) for any logging in the new QML.

---

### Specs & dependencies for Task 6.0 (tests + docs)

- Depends on 1.0-5.0.

- [ ] 6.0 Update tests and documentation
  - [ ] 6.1 In `backend/tests/test_render_sutta_content.rs`, add tests for `SideBySide` rendering of a segmented translation (asserts the `side-by-side-row` / `sbs-col` structure and heading/footer columns) and for the `order` flag swapping left/right.
  - [ ] 6.2 Add a test for the non-segmented (`content_html`) side-by-side case (two columns when a Pāli counterpart exists; single column when not).
  - [ ] 6.3 Add/adjust a test confirming `OnlyTranslation` and `LineByLine` still render as before, and that a Pāli-only sutta renders single-column in all modes.
  - [ ] 6.4 Run `cd backend && cargo test` and `make build -B`; confirm a clean build and passing tests.
  - [ ] 6.5 Add a `docs/` note describing the three layout modes, the order setting, and the render branch, and update `PROJECT_MAP.md` and `AGENTS.md`'s notable-feature-docs list with a pointer.
