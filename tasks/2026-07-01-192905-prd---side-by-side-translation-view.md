# PRD: Side-by-Side Translation and Pāli View

## 1. Introduction/Overview

Simsapa currently renders a translated sutta in one of two ways, controlled by a
single boolean setting (`show_translation_and_pali_line_by_line`):

- **Off** — show only the translation.
- **On** — for segmented (Bilara/`content_json`) texts, interleave the
  translation and the Pāli line by line (alternating lines).

This feature replaces that single checkbox with a **three-way choice** and adds
a new **side-by-side** rendering mode where the translation and the Pāli appear
in two parallel 50%-width columns. For segmented texts the two columns are
aligned item-by-item (heading against heading, paragraph against paragraph); for
non-segmented (`content_html`) translations the two texts appear in two columns
but cannot be aligned segment-by-segment.

The goal is to give readers a clearer parallel reading experience while keeping
the existing "translation only" and "line-by-line" behaviours intact.

## 2. Goals

1. Replace the single line-by-line checkbox in **Settings → Display** with a
   three-option selector ("Only the translation", "Line-by-line", "Side-by-side")
   including explanatory text for each.
2. Add a new **side-by-side** rendering mode with two 50%-width columns.
3. For segmented (`content_json`) texts, keep the two columns **aligned
   item-by-item** so that the progression of the text stays in step across both
   columns.
4. For non-segmented (`content_html`) translations, render the translation in one
   column and the Pāli in the other (unaligned) column.
5. Add an **order option** (which text is on the left/right) that applies to
   **both** line-by-line and side-by-side modes. Default: translation on the
   left, Pāli on the right.
6. Replace the boolean setting with a three-value enum (`TranslationPaliLayout`)
   defaulting to Line-by-line; no legacy-boolean migration is required.
7. Preserve the existing "close and re-open the sutta tab to see the effect"
   model — no live re-render of already-open tabs is required.

## 3. User Stories

- As a reader studying a translated sutta, I want to see the translation and the
  Pāli side by side in aligned columns, so I can compare a passage with its
  source without hunting through interleaved lines.
- As a reader who prefers a specific reading order, I want to choose whether the
  Pāli or the translation is on the left, so the layout matches my habit — in
  both line-by-line and side-by-side modes.
- As a reader of a translation that has no segmented format, I still want to see
  the Pāli alongside it in a second column, even if the lines are not perfectly
  aligned.
- As a reader who only wants the translation, I want to turn both parallel modes
  off entirely.

## 4. Functional Requirements

### Settings UI (`AppSettingsWindow.qml`)

1. The system must replace the `show_line_by_line_checkbox` CheckBox (and its
   single description Label) in the **Display** section with a labelled group:
   **"Show Translation and Pāli:"** followed by three mutually-exclusive
   **RadioButton** options (so all options are visible at once), each with its
   own explanatory sub-text Label.
2. The three options and their explanatory sub-text must be:
   - **Only the translation** — (no extra explanation needed, or a short one-line
     note).
   - **Line-by-line (only for segmented texts)** — "When a translation is
     available in segmented line-by-line format, show it interleaved with the
     Pāli text."
   - **Side-by-side** — "For translations available in segmented format, show
     them in two columns with the text aligned. Translations without segmented
     format will also be in two columns, but the text cannot be aligned."
3. The group must display the reminder: **"Close and re-open the relevant sutta
   tabs to see the effect."**
4. The system must add an **order** control (a two-option selector:
   "Translation | Pāli" vs "Pāli | Translation") that determines which text is
   on the left. This order applies to **both** line-by-line and side-by-side
   modes. Default: **Translation first (left), Pāli second (right)**. The order
   control must be **disabled** when "Only the translation" is selected (it has
   no effect there) and enabled for the Line-by-line and Side-by-side modes.
5. On changing any option, the QML must persist it via the `SuttaBridge`
   (see requirement 12–13), matching the existing pattern of the other Display
   settings.
6. On window open, the controls must be initialised from the persisted values
   (mirroring the existing `AppSettingsWindow.qml` init block around line 1211).

### Rendering (`backend/src/app_data.rs`, `backend/src/helpers.rs`)

7. `render_sutta_content()` must branch on the new three-way mode instead of the
   boolean:
   - **Only translation** — render as the current "standard" path
     (`bilara_content_json_to_html` for segmented, or the existing
     `content_html` / `content_plain` fallbacks).
   - **Line-by-line** — behave as today (`bilara_line_by_line_html`) for
     segmented translations with a Pāli counterpart; fall back to the standard
     path otherwise (as today).
   - **Side-by-side** — new behaviour described below.
8. In **side-by-side** mode for a **segmented** (`content_json`) translation with
   a Pāli counterpart (obtained via the existing `get_pali_for_translated`):
   - The system must build the page **item-by-item** following the template
     order (the same ordered-keys approach used by `bilara_line_by_line_html`,
     including the Pāli-only-segment safeguard).
   - Each parallel item — **including headings and footers** — must be wrapped
     in a flex container holding two 50%-width columns so the two texts'
     progression stays aligned across the page (headers align against headers,
     footers against footers).
   - Column order (which text is left) must follow the **order** setting.
9. In **side-by-side** mode for a **non-segmented** (`content_html`) translation:
   - The Pāli side must be sourced using the **same `get_pali_for_translated`**
     lookup used for line-by-line. The Pāli counterpart usually **has** segmented
     format; render that column from its segments. If the Pāli counterpart has
     **no** segmented format, fall back to rendering its `content_html`.
   - If a Pāli counterpart is found, render the translation HTML block in one
     column and the Pāli in the other (two columns, unaligned), respecting the
     order setting.
   - If **no** Pāli counterpart is found, render the translation **full-width in
     a single column** (no empty second column).
10. For a **Pāli-only** sutta (`language == "pli"`, i.e. the user is viewing the
    Pāli text itself with no translation), side-by-side and line-by-line modes
    must degrade gracefully to the standard single-text rendering (no empty
    second column).
11. Alignment for segmented texts is at the **template-item** granularity (the
    existing segment/template keys), not sub-segment word alignment.

### Settings storage (`backend/src/app_settings.rs`, `app_data.rs`, `bridges/src/sutta_bridge.rs`)

12. Replace the boolean `show_translation_and_pali_line_by_line` field with an
    **enum-valued** setting representing the three modes, named
    `TranslationPaliLayout` with variants `OnlyTranslation`, `LineByLine`,
    `SideBySide`. Serialisation format can be chosen freely — **backward
    compatibility with the old boolean is NOT a concern** (the app is new).
13. The new setting defaults to **`LineByLine`** (preserving the current default
    behaviour, which defaults to `true` at `app_settings.rs:218`).
14. Add a persisted **order** setting named `translation_pali_order` with
    variants `TranslationFirst` and `PaliFirst`, defaulting to
    `TranslationFirst`.
15. Expose getters/setters for both settings through `SuttaBridge`
    (`get_/set_…`), following the existing bridge pattern, and mirror the new
    function signatures in the qmllint stub
    `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`.

### Styling (`assets/sass/` → `assets/css/`)

16. Add CSS (**CSS flexbox**) for the side-by-side wrapper and the two
    50%-width columns (translation/Pāli), reusing the existing
    `suttacentral bilara-text` structure where possible. Headings and footers
    are laid out in the same two-column flex containers as paragraphs. On narrow
    screens the layout **remains two columns** (no responsive collapse);
    horizontal scrolling is acceptable.

## 5. Non-Goals (Out of Scope)

- **No** responsive collapse of side-by-side to line-by-line or stacked layout on
  narrow/mobile screens — it stays two columns everywhere.
- **No** sub-segment / word-level alignment; alignment is per template item only.
- **No** live re-rendering of already-open sutta tabs; the user re-opens tabs to
  see the change (consistent with the current behaviour).
- **No** three-or-more column views, and no per-sutta override of the global
  setting.
- **No** change to how the Pāli counterpart is discovered beyond reusing the
  existing `get_pali_for_translated`.

## 6. Design Considerations

- Settings UI lives in the **Display** section of `AppSettingsWindow.qml`
  (currently around lines 602–624). Follow the existing spacing, `pointSize`,
  and description-Label conventions.
- The existing line-by-line HTML wraps each segment as
  `<span class='segment'><span class='translated'>…</span><span class='pali'>…</span></span>`
  (`helpers.rs:2165`). The side-by-side builder can reuse the same
  translated/Pāli split but arrange the two into column wrappers per template
  item.
- Default visual order: **translation left, Pāli right**; the order setting swaps
  them (and the same swap applies to the line-by-line interleave order).
- Reuse the `suttacentral bilara-text` container and theme/lang body classes so
  existing typography, dark mode, and per-language CSS continue to apply.

## 7. Technical Considerations

- Rendering is entirely server-side in Rust; the QML/WebEngine view consumes the
  produced HTML string, so most work is in `app_data.rs` and `helpers.rs` plus
  the settings plumbing.
- Add a new side-by-side builder in `helpers.rs` (sibling to
  `bilara_line_by_line_html`) that takes the translated segments, Pāli segments,
  template map, `show_references`, and the order flag, and emits per-item
  two-column wrappers. Preserve the **ordered-keys** logic (template-driven, with
  the union fallback and Pāli-only-segment safeguard) so no segment is dropped.
- The `TranslationPaliLayout` enum can use whatever serialisation is convenient
  (no legacy boolean to migrate).
- Update `docs/` where the rendering/settings behaviour is described, and keep
  `PROJECT_MAP.md` current if new functions are added.
- New QML files (if any) must be added to `qml_files` in `bridges/build.rs`; new
  bridge functions need the qmllint stub entry. Use the `Logger` module (not
  `console`) in any new QML.
- Follow the Android-safe and code-style conventions in `AGENTS.md`.

## 8. Success Metrics

- A user can select each of the three modes in Settings and, after re-opening a
  sutta tab, see the corresponding rendering.
- For a segmented translation in side-by-side mode, headings/paragraphs stay
  visually aligned across the two columns.
- For a non-segmented (`content_html`) translation with a Pāli counterpart,
  side-by-side shows two columns; with no counterpart it shows a single
  full-width column.
- The order setting correctly swaps left/right in both line-by-line and
  side-by-side modes.
- The order control is disabled when "Only the translation" is selected.

## 9. Resolved Decisions

- **Enum:** `TranslationPaliLayout { OnlyTranslation, LineByLine, SideBySide }`,
  default `LineByLine`. Order: `translation_pali_order { TranslationFirst,
  PaliFirst }`, default `TranslationFirst`. No legacy-boolean migration needed.
- **Control type:** RadioButtons for the three-way choice (all options visible).
- **Order control:** disabled for "Only the translation", enabled otherwise.
- **CSS:** flexbox two-column wrappers; headings and footers also render as
  aligned side-by-side columns.
- **Non-segmented Pāli column:** render the Pāli counterpart's segments when it
  has segmented format (the usual case); fall back to its `content_html` when it
  does not.

## 10. Open Questions

_None outstanding — the previously open items were resolved above._
