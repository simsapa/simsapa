# PRD — Available dictionaries: download and install from the Dictionaries window

## 1. Introduction / Overview

The **Dictionaries** window (`bridges/assets/qml/DictionariesWindow.qml`) lists
the user's imported dictionaries. When none are imported it shows a paragraph of
text with a link to
<https://github.com/digitalpalidictionary/other-dictionaries/releases/> and
leaves the rest to the user: follow the link in a browser, work out which of the
~15 assets is the right format, download a zip, find it again in the file picker,
choose a label and a language, and import it.

Every one of those steps is a place to give up, and on Android the file-picker
half of it is the part with the longest defect history in this codebase (see
[docs/dictionary-import-pipeline.md](../docs/dictionary-import-pipeline.md) §4).
Yet the dictionaries in question are a small, known, stable set published by a
single upstream project.

This feature adds an **Available** section to the Dictionaries window listing
ten curated dictionaries with checkboxes and a **Download and Import** button.
Checking two boxes and pressing one button replaces the whole browser-and-picker
detour.

The manual **Import StarDict/GoldenDict…** path is unchanged and remains the way
to install anything not in the curated list.

## 2. Goals

1. A user with no dictionaries can install Cone, CPD and CPED without leaving the
   app and without touching a file picker.
2. The download URLs resolve against the newest *compatible* upstream release, so
   an upstream patch release reaches users without a Simsapa release.
3. An incompatible upstream release (a new minor or major version) is ignored
   rather than downloaded blind.
4. The feature degrades to a usable state offline and when GitHub is
   unreachable or rate-limiting.
5. No new file-picker code paths, and no new import pipeline — the downloaded
   archive enters the *existing* `import_zip` batch flow.

## 3. User Stories

- **As a new user** who has just installed Simsapa, I open Dictionaries, see a
  list of ten dictionaries with their names and sizes, tick three, press
  **Download and Import**, and watch them install one after another.
- **As an existing user** with two dictionaries already imported, I see my two at
  the top and the remaining eight under **Available**; the two I have are not
  offered again.
- **As a user on a slow connection**, I see per-item download progress in bytes
  and can tell which of my three selections is currently downloading.
- **As a user whose connection drops mid-batch**, the failed dictionary is
  reported by name at the end, the others still installed, and the failed one is
  still in **Available** to try again.
- **As an offline user**, the Available list still renders and I get a clear
  error when I press the button, rather than an empty screen.

## 4. The curated catalogue

Ten entries. Every field below was read from the shipped v1.0.8 assets, not from
documentation: the **label** is the asset file name with the `-gd.zip` suffix
removed, and the **name** is the `bookname` line of the archive's `.ifo` file —
with one deliberate exception, `abt`, noted below the table.

| Label | Name (from `.ifo` `bookname`) | Entries | Size | Import `lang` |
|---|---|---|---|---|
| `abt` | Ancient Buddhist Texts Glossary (CPED) — *see note* | 21,099 | 0.39 MB | `pli` |
| `apte` | Apte Practical Sanskrit-English Dictionary, 1890 (sa-en) | 34,277 | 5.49 MB | `san` |
| `bhs` | Edgerton's Buddhist Hybrid Sanskrit Dictionary 1953 (sa-en) | 17,836 | 2.26 MB | `san` |
| `cone` | Dictionary of Pāli by Margaret Cone (pi-en) | 37,391 | 55.69 MB | `pli` |
| `cpd` | Critical Pāli Dictionary (pi-en) | 29,734 | 6.57 MB | `pli` |
| `mw` | Monier-Williams Sanskrit-English Dictionary, 1899 (sa-en) | 194,084 | 19.08 MB | `san` |
| `nyanatiloka` | Buddhist Dictionary: Manual of Buddhist Terms and Doctrines (pi-en) | 1,406 | 0.20 MB | `pli` |
| `peu` | Pali English Ultimate (pa-en) | 203,865 | 7.78 MB | `pli` |
| `sin-eng-sin` | Sinhala-English English-Sinhala (si-en) | 96,050 | 1.85 MB | `si` |
| `whitney` | Whitney Sanskrit Roots (sa-en) | 1,009 | 0.16 MB | `san` |

**Deliberately not offered** (do not add them): `all-dictionaries`, `bold-def`,
`dppn`, `dpr`, `simsapa`. The last three are already shipped with Simsapa or
duplicate shipped content; `all-dictionaries` is a 172 MB bundle of everything;
`bold-def` is the shipped Bold Definitions.

Only the `-gd.zip` (StarDict / GoldenDict) assets are used. The parallel
`-mdict.zip` assets are a different format the importer does not read.

Three notes for whoever writes the table into code:

- **`abt` is the only name not taken from the `.ifo`.** Its `bookname` reads
  `Concise Pali English Dictionary (pi-en)` (`author=Ven. A. P. Buddhadatta`,
  `website=ancient-buddhist-texts.net`), but the catalogue must display
  **"Ancient Buddhist Texts Glossary (CPED)"** — the name that matches both the
  asset's own label and the CPED the user is looking for. Hard-code this one
  string; do not derive it. Earlier internal notes called this entry `cped`;
  there is no `cped` asset and there never was.
- **`peu`'s `.ifo` says `(pa-en)`.** `pa` is ISO 639-1 for *Punjabi*; upstream
  means Pali. The import `lang` is `pli`, not `pa`. Do not derive the language
  code from the bookname string.
- **`si` is not a known tokenizer language.** `KNOWN_TOKENIZER_LANGS`
  (`bridges/src/dictionary_manager.rs:255`) has `pli`, `san`, `en` and the
  European set, but no `si`. `sin-eng-sin` will therefore index with the default
  tokenizer — the same outcome a manual import with `si` gives today. This is
  accepted, not a defect to work around.

Labels are valid: `validate_label()` allows ASCII alphanumerics, `_` and `-`, so
`sin-eng-sin` passes.

## 5. Functional Requirements

### 5.1 The Available section (UI)

1. The Dictionaries window must show an **Available** section below the existing
   list of imported dictionaries. It is shown whether or not any dictionaries are
   imported.
2. Each row must show the dictionary's **name** (the `bookname` from §4), its
   **label**, and its **download size**.
3. Each row must have a **checkbox**, unchecked by default.
4. Below the list, the section must show a **Download and Import** button,
   disabled while nothing is checked.
5. When entries are checked, the section must show the **combined download size**
   of the checked set.
6. A dictionary whose label matches one already imported must be **omitted from
   the Available list entirely**. Refreshing the imported list (`refresh_list()`)
   must also refresh the Available list, so an entry disappears from Available as
   soon as its import finishes and reappears if it is deleted.
7. Beneath the Available list, a single short line must link to
   <https://github.com/digitalpalidictionary/other-dictionaries/releases/> for
   the dictionaries not in the curated set. It is always visible, not only when
   the imported list is empty. The existing "No imported dictionaries yet"
   paragraph is replaced by this line plus the Available list.
8. The section must **name its source and the resolved release tag**, so the
   user can see where these files come from and which upstream version they are
   getting — e.g. `digitalpalidictionary/other-dictionaries v1.0.8`. It updates
   when the tag resolution completes, and shows the fallback tag when the lookup
   failed (FR-14). This doubles as diagnosis: an upstream mismatch is then
   readable from a screenshot.
9. The whole window content — the imported list *and* the Available section —
   must scroll together in the existing `ScrollView`. The Available section is
   always expanded; there is no collapse control.
10. The section must render on mobile at phone width — name wrapping, checkbox
    and size legible.

### 5.2 Resolving the release tag

11. The app must carry a **pinned compatible series** constant, `1.0`, and a
    **fallback tag** constant, `v1.0.8`.
12. When the Dictionaries window opens, the app must query the upstream releases
    list (`https://api.github.com/repos/digitalpalidictionary/other-dictionaries/releases`)
    on a background thread and select the **highest tag whose major and minor
    match the pinned series**, ignoring prereleases and drafts.
13. A release outside the pinned series (e.g. `v1.1.0`, `v2.0.0`) must be
    **ignored**. A minor or major bump upstream is the case where the import
    mechanism itself is most likely to need work — renamed assets, a changed
    archive layout, a different format — so raising the pin is a deliberate
    Simsapa code change, made after someone has checked the upstream assets still
    match §4. It must not be reachable from settings (§11).
14. If the query fails for any reason — offline, DNS failure, HTTP error, rate
    limit (`403` with `X-RateLimit-Remaining: 0`), unparseable JSON, no tag in
    the pinned series — the app must fall back to the **fallback tag constant**
    and still render the Available list. The failure is logged; no error is shown
    while merely browsing the list.
15. Version parsing and comparison must reuse `to_version()` and
    `compare_versions()` from `backend/src/update_checker.rs`. The tags carry a
    leading `v` which `to_version()` must be given without, or tolerate — verify
    which and handle it in one place.
16. Download sizes and download URLs must come from the API response when it
    succeeded (`assets[].size`, `assets[].browser_download_url`). Under the
    fallback tag, the URL is constructed as
    `https://github.com/digitalpalidictionary/other-dictionaries/releases/download/<tag>/<label>-gd.zip`
    and the size is the baked-in figure from §4, shown as an approximation.
17. The resolved tag must be logged once per resolution, with its source
    (`api` or `fallback`).

**This is not the existing app-release mechanism.**
`get_latest_app_compatible_assets_release()` works over *Simsapa's* releases feed
(pythonanywhere `POST /releases` plus `assets/releases-fallback.json`) and is
keyed on Simsapa's own app and database versions — see
[docs/releases-info-and-fallback.md](../docs/releases-info-and-fallback.md). The
DPD `other-dictionaries` repository is a third-party GitHub repo that does not
appear in that feed at all. Reuse the *version comparison helpers*; do not try to
route this through `compatible_assets_release()`.

### 5.3 Download and import

18. Pressing **Download and Import** must process the checked entries
    **sequentially**, in catalogue order.
19. Each archive must be downloaded into the **dictionaries staging directory**
    (`import_staging::staging_dir("dictionaries")`), under the asset's own file
    name. This puts the downloaded file under the same ownership rules as a
    staged pick, so the existing cleanup applies unchanged (§5.5).
20. During the download phase the window must show a progress frame giving the
    current dictionary's name, bytes downloaded, total bytes, and its position in
    the batch (e.g. "2 of 3").
21. Once an archive is downloaded, its import must go through the **existing
    batch import** — the item is appended to the queue consumed by
    `DictionariesWindow.start_batch()` as
    `{ kind: "zip", path: <staged path>, member: "", label: <label>, lang: <lang> }`.
    No new import code path is written.
22. `member` is `""` (the whole archive) for every catalogue entry: each `-gd.zip`
    contains exactly one dictionary at its root. The probe/scan step
    (`scan_source`) is **not** run — the label, language and member are known from
    the catalogue, which is the point of the feature.
23. The user is asked nothing: no import dialog, no label prompt, no language
    prompt. Label and language come from the §4 table.
24. A **Cancel** control must be available during the run. Cancelling stops
    before the next item starts; an in-flight download is aborted; an in-flight
    import goes through the existing `abort_import()`.
25. The whole run must hold the keep-screen-on lock under a **distinct holder
    name** — `dictionary-download-batch` — acquired when the run starts and
    released in the single function every ending passes through (success,
    failure and cancel alike). See the holder rules in `CLAUDE.md`.
26. While a run is active, the window must refuse to close, matching the existing
    `onClosing` guard for the delete/import/rename frames.

### 5.4 Failures

27. A failure in one item must **not** stop the batch. The run records the
    failure and continues with the next checked entry.
28. Failures must be surfaced at the end, in the existing summary frame, naming
    each failed dictionary and its error message alongside the count that
    succeeded.
29. A failed entry remains in **Available** (it was never imported) and may be
    retried by checking it again.
30. Download failures must be distinguishable from import failures in the message
    — the user needs to know whether to check their connection or report a bad
    archive.
31. A download that returns HTTP 404 must report that the file is not available in
    the upstream release, naming the resolved tag. This is the symptom of an
    upstream rename, and the message must be enough to diagnose it from a user's
    `log.txt`.
32. A partially downloaded file must be deleted before the item is recorded as
    failed, so a retry does not resume onto a truncated file.

### 5.5 Temporary files

33. Every downloaded archive must be deleted once the run ends, by the same
    `cleanup_staged_file()` call the existing batch already makes in
    `finish_batch()` — which is why FR-19 requires the download to land in the
    dictionaries staging directory. `cleanup_staged_file` decides ownership **by
    location**, so a file written anywhere else will silently survive.
34. The feature name passed to `staging_dir()` must be the same
    `"dictionaries"` constant the pick path uses. A mismatch fails *silently* and
    leaves 10–200 MB behind per run.

## 6. Non-Goals (Out of Scope)

- **Updating an installed dictionary.** A dictionary already imported is hidden
  from Available; there is no "a newer version is available" check, no re-download
  and no replace. If a user wants the newer one they delete and re-install.
- **The `-mdict.zip` assets** and the `all-dictionaries` bundle.
- **`dppn`, `dpr`, `simsapa`, `bold-def`** — excluded by decision, not by
  oversight.
- **A generic "add a catalogue source" mechanism.** The catalogue is one hard-coded
  list from one repository.
- **Detecting a duplicate imported under a different label.** FR-6 hides a
  catalogue entry when its label matches an imported one, and that is the whole
  of the duplicate handling. A user who imported the same dictionary *manually*
  got the label the zip name suggested — `cone-gd`, not `cone` — so the catalogue
  still offers `cone` and they end up with both. **This is accepted:** the two
  are plainly visible in the imported list and the user deletes whichever they do
  not want. Matching on content rather than label is out of scope.

  The *same*-label case does not arise from this flow at all: FR-6 removes the
  row, so there is nothing to check and nothing to press. (`import_zip` would
  refuse it anyway — `dictionary_manager_core` returns "A dictionary with label
  'X' already exists." — but that backstop is for the manual import path, not
  this one.)
- **Checksum or signature verification** of the downloaded archives. The import
  step already rejects a malformed StarDict archive.
- **Resumable / parallel downloads.** Sequential, restart-from-zero on retry.
- **A mobile-data confirmation prompt.** Sizes are shown; no threshold dialog.
- **Changing the manual import flow** in any way.

## 7. Design Considerations

- The section lives inside the existing `ScrollView` in `DictionariesWindow.qml`,
  below the `Repeater` over `root.user_dictionaries`.
- The download progress frame is a new index in the existing `views_stack`,
  alongside the delete (1), import (2), rename (3) and summary (4) frames. The
  summary frame is reused, with a new `op_kind` for this run.
- A per-row component (e.g. `AvailableDictionaryRow.qml`) keeps the delegate
  readable; if created it must be added to `qml_files` in `bridges/build.rs` as
  `"assets/qml/AvailableDictionaryRow.qml"` — relative to `bridges/`, never with
  `..`.
- Any new `Dialog` must follow the width-clamp rule
  (`parent: Overlay.overlay`, `width: Math.min(parent.width - 40, 480)`) and, if
  it has both a title and wrapping text, use `header: DialogHeader { … }`.
- Logging in QML goes through `Logger`, one concatenated string argument.

## 8. Technical Considerations

- **Where the code goes.** The catalogue table, tag resolution and the download
  belong in `backend/` (Qt-free, unit-testable) with a thin
  `DictionaryManager` bridge surface: something like
  `available_dictionaries() -> QString` (JSON: label, name, size, url, lang,
  resolved tag, tag source) plus `download_available(labels)` driving
  `download_progress` / `download_finished` / `download_failed` signals. Follow
  the existing `import_staging.rs` shape — pure Rust, chunked reads, cancel flag
  between chunks, `f64` byte counts, progress throttled to ~100 ms.
- **HTTP client.** `backend/Cargo.toml` already has
  `reqwest = { version = "0.12", default-features = false, features = ["blocking", "json", "rustls-tls"] }`.
  Use it; do not add a dependency. Note the pin comment in `bridges/Cargo.toml`
  — reqwest must stay on 0.12.
- **`queue_or_log`, never `.unwrap()`** on `qt_thread.queue()`. The Dictionaries
  window is destroyed on close, so `ObjectDestroyed` is a live path. Log and
  continue — an early return would skip the staged-file cleanup.
- **`try_exists()`, never `.exists()`** for any file check, per the Android rule.
- **GitHub API without a token** allows 60 requests/hour per IP. One request per
  window open is fine; do not poll. Caching the resolved tag for the lifetime of
  the process is enough and avoids a second request when the window is reopened.
- The catalogue's baked-in sizes are for the fallback path only and will drift
  from a newer patch release. Display them as approximate ("~55 MB") when the
  API lookup failed.

## 9. Success Metrics

1. From a fresh install with no dictionaries, a user can install `cone` and `cpd`
   with: open Dictionaries → tick two → press one button. No file picker, no
   browser, no text entry.
2. With the network disconnected, the Available list still renders all ten
   entries, and pressing the button produces a named download error per entry
   rather than a hang or a blank frame.
3. After a run, the dictionaries staging directory contains no leftover archives.
4. Publishing an upstream `v1.0.9` changes the resolved tag with no Simsapa
   release; publishing a `v1.1.0` does not.
5. Unit tests cover tag selection (highest patch in series; a higher minor
   ignored; empty list; malformed tag) and catalogue URL construction, with no
   network access.

## 10. Testing Notes

- Tag selection, URL construction and the catalogue table are pure functions —
  test them in `backend/` with a fixture of the GitHub JSON, no network.
- The download itself needs a device/desktop run. `nyanatiloka` (0.20 MB) and
  `whitney` (0.16 MB) are the cheap end-to-end cases; `cone` (55.69 MB) is the
  one that exercises progress display and keep-screen-on.
- `sin-eng-sin` is the case that must produce the unknown-tokenizer path without
  failing the import.
- Follow the agent GUI rule: verify with `make build -B` and `cargo test`; the
  interactive run is the user's.

## 11. Decisions Taken

These three were open while the PRD was drafted and are now settled. They are
recorded because each looks like a free choice at implementation time and is not.

1. **The resolved tag is shown to the user** — FR-8. Repo name plus tag, e.g.
   `digitalpalidictionary/other-dictionaries v1.0.8`.
2. **The Available section is not collapsible** — FR-9. The window scrolls as one
   list, which is enough on a phone; a collapse control would be a second way to
   hide content the user came to find.
3. **The pinned series stays a constant, not a setting.** There are no urgent
   upstream dictionary releases, so nothing needs to be picked up between Simsapa
   releases. More to the point, a minor or major version bump upstream is exactly
   the case where the import mechanism itself is likely to need changing —
   renamed assets, a different archive layout, a format change. Letting a setting
   wave such a release through would remove the one gate that forces a human to
   look first. Raising the pin is a code change, deliberately.

## 12. Open Questions

None outstanding.
