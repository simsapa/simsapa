# Tasks — Android API 36 compliance and packaging follow-ups

PRD: [2026-07-27-131601-prd---android-api-36-compliance-and-packaging-follow-ups.md](./2026-07-27-131601-prd---android-api-36-compliance-and-packaging-follow-ups.md)

## Relevant Files

**Backend / bridge (task 1.0)**

- `backend/src/app_settings.rs` — `AppSettings.mobile_top_bar_margin` (line 183), its
  default (675), the `is_/get_/set_` helpers (753–769) and the
  `MobileTopBarMargin` enum (784–788). All replaced by one `u32` field.
- **`AppSettings` is deserialized in three places, not one** (PRD §6.7) — which is
  why the migration belongs in `Deserialize`, not at a call site:
  - `backend/src/db/appdata.rs:443` — `AppdataDbHandle::get_app_settings()`, the
    **in-app path** that fills `app_settings_cache`. Already does a
    post-deserialize fixup (`merge_default_system_prompts()`).
  - `backend/src/db/mod.rs:389` — the free `get_app_settings()` (fn at 367), a
    standalone pre-`QApplication` read used by `gui.cpp`.
  - `backend/src/app_data.rs:3237` — the `import-me/app_settings.json` import.
- `backend/src/app_data.rs` — `set_mobile_top_bar_margin_system()` (2377) and
  `set_mobile_top_bar_margin_custom()` (2398): cache write + persist to the
  `app_settings` row. Collapse to one setter.
- `bridges/src/sutta_bridge.rs` — `#[qinvokable]` declarations (1352–1364),
  implementations (4469–4510), and `get_status_bar_height` (1059, 3066).
- `bridges/src/api.rs` — `ffi` block declaring `get_status_bar_height()` (224).
- `cpp/utils.cpp` / `cpp/utils.h` — `get_status_bar_height()` (utils.cpp:30).
- `backend/src/app_settings.rs` `mod tests` — the serde migration/round-trip tests
  (1.4), next to the code they cover.
- `backend/src/db/appdata.rs` `mod app_settings_tests` — **new**; exercises the
  in-app `get_app_settings()` read path against a throwaway temp appdata DB (1.3).

**QML (task 2.0)**

- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — qmllint stubs (434–452).
- `assets/qml/AppSettingsWindow.qml` — the "Mobile Top Margin" section (578–640)
  replaced by "Extra Top Margin" (one SpinBox + the live `SafeArea` readout);
  `use_system_margin` / `custom_margin_value` state deleted in favour of the
  `window_safe_area_margins` / `system_safe_area_top` readout properties;
  `marginChanged()` kept, and its own margin use (189, 356, 930).
- `assets/qml/SuttaSearchWindow.qml` — property (77), `update_extra_top_margin()`
  (294–296), call sites (1332, 2404), pass-downs (2259–2294, 2397), anchor (2482).
- Windows owning the property: `LibraryWindow.qml` (27, 39–40, 116),
  `DictionariesWindow.qml` (28, 83, 315), `SuttaLanguagesWindow.qml` (28, 109–110, 236),
  `TopicIndexWindow.qml` (25 — note desktop default `5`, 58, 100, 251),
  `ReferenceSearchWindow.qml` (24, 45, 74, 89),
  `ChantingPracticeWindow.qml` (26, 41, 587),
  `ChantingPracticeReviewWindow.qml` (26, 76, 308),
  `DictionaryImportDialog.qml` (37, 70, 205),
  `DownloadAppdataWindow.qml` (26–27, 375 — deliberately fixed, runs before app data).
- Windows receiving `required property int extra_top_margin`: `AboutDialog.qml`,
  `SystemPromptsDialog.qml`, `ModelsDialog.qml`, `AnkiExportDialog.qml`,
  `DatabaseValidationDialog.qml`, `DhammaTextSourcesDialog.qml`,
  `UpdateNotificationDialog.qml`, `SearchHelpWindow.qml`,
  `KeybindingCaptureDialog.qml`, `TopicIndexInfoDialog.qml`,
  `ReferenceSearchInfoDialog.qml`.
- `assets/qml/GlobalHotkeysSection.qml` — a `ColumnLayout` (not a window) that
  merely forwards the value (16, 79).
- `assets/qml/DrawerMenu.qml` — a `Drawer` (`Popup` subclass), `height:
  window_height`, lives in the window overlay so Qt does **not** pad it; its first
  child is a top-anchored `Label { text: "Menu" }`. Instantiated at
  `SuttaSearchWindow.qml:2249`. Needs its own `topPadding` (PRD 15a).
- `assets/qml/MobileTopMarginDialog.qml` — **deleted** (dead code); it was
  already absent from `bridges/build.rs`'s `qml_files`, so that list is unchanged.

**Android build config (tasks 3.0, 4.0)**

- `android/build.gradle` — `targetSdkVersion` (defaultConfig), the new
  `androidComponents { beforeVariants … }` block.
- `android/gradle.properties` — `android.suppressUnsupportedCompileSdk=36`.
- `android/AndroidManifest.xml` — only if the predictive-back opt-out is needed.
- `android/version.txt` — **new**; holds the versionCode (seed `3`).
- `build-android.sh` — version parsing/export/echo, the release-only Gradle
  property, existing artifact checks.
- `CMakeLists.txt` — `ANDROID_VERSION_CODE` / `ANDROID_VERSION_NAME` (140–141)
  and the `QT_ANDROID_VERSION_*` properties (353–357).
- `Makefile` — Android targets and their documented command lines (90–134).

**Documentation (task 7.0)**

- `docs/android-edge-to-edge-and-safe-areas.md` — **new**.
- `docs/android-qt-upgrade-considerations.md` — **new**.
- `docs/android-multi-abi-and-chromeos.md`, `AGENTS.md` (`CLAUDE.md` is a symlink).
- `tasks/android-packaging-follow-ups.md` — already deleted (never committed);
  confirm nothing references it.

### Notes

- Rust tests: `cd backend && cargo test`. QML tests: `make qml-test`. All: `make test`.
- New/renamed `SuttaBridge` methods **must** get matching stubs in
  `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` (project rule), and any new
  QML file must be added to `qml_files` in `bridges/build.rs`.
- Agents must not drive the GUI (`AGENTS.md` → "GUI Testing for Agents"); task 6.0
  is a human-run pass on a physical **Android 16** phone.
- **Margins need eyes, not tests.** This whole change is spacing, and neither
  `make qml-test` nor `qmllint` can see a wrong gap. Task **2.12** is the quick
  visual pass right after the rename (desktop should be pixel-identical; mobile
  should lose the doubled gap and the first-paint jump), and task **6.0** is the
  full on-device edge-to-edge audit. A green test suite proves nothing here.
- Each top-level task should leave the tree compiling with tests passing.
- **1.0 and 2.0 are one commit, not two.** They fix a bug in the **shipped**
  build (the doubled top gap on every Android 15+ device) and depend on nothing
  in 3.0/4.0, so *the pair* can ship ahead of the rest — but 1.0 alone renames
  the bridge API out from under the QML, so the tree does not build between
  them (1.9 acknowledges this). Do not split them across commits.
- **Never assign `topPadding` / `padding` on an `ApplicationWindow` root.** Qt's
  safe-area binding is installed with `binding.installOn(targetProperty)`
  (`qquickapplicationwindow.cpp:793`), so an explicit assignment replaces it
  silently and the inset vanishes for that window. Verified: no window does this
  today — every `padding:` in `assets/qml/` is on an inner control. This is the
  first thing to check if an inset ever goes missing.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`.
Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

---

### 1.0 — specs

**State model.** `AppSettings.mobile_top_bar_margin: MobileTopBarMargin`
(`SystemValue | CustomValue(u32)`) becomes:

```rust
/// Extra space (dp) added below the system-provided safe area at the top of
/// mobile windows. Qt's ApplicationWindow already pads the window by the real
/// safe-area inset; this is only what the user wants *in addition*.
/// 0 = rely on the system inset alone.
pub mobile_extra_top_margin: u32,
```

`AppSettings` already carries a **container-level** `#[serde(default)]`
(`app_settings.rs:161`), so a per-field `#[serde(default)]` is redundant.

**Persisted-value migration.** `AppSettings` is stored as one JSON blob in the
`app_settings` row. Old JSON holds `"mobile_top_bar_margin": "SystemValue"` or
`{"CustomValue": 24}`. Serde ignores unknown fields by default, so without a hook
every user silently gets `0` — correct for `SystemValue`, **wrong** for anyone who
customised it. Carry the number over (PRD 9): `SystemValue → 0`,
`CustomValue(v) → v`.

**It is deserialized in three places, not one** (see Relevant Files / PRD §6.7),
and the in-app one is `db/appdata.rs:443` — *not* `db/mod.rs:389`. A hook at
`db/mod.rs` alone would migrate only the pre-`QApplication` copy and the running
app would still show the default, i.e. the migration would look like a no-op.
So put it in `AppSettings`' own deserialization — `#[serde(from = "…")]` over a
private wire struct, or a manual `impl Deserialize` — where no reader can bypass
it. Three constraints that fall out of that:

- the legacy capture field must be `#[serde(skip_serializing)]`, or the old key
  is written straight back out on the next save;
- the `MobileTopBarMargin` enum cannot simply be deleted while something is
  typed as it — keep it `pub(crate)` on the wire struct, or capture the legacy
  key as `Option<serde_json::Value>`;
- "migrate only when the new key is absent" is **not expressible** against a
  plain `u32` under the container-level `#[serde(default)]` (absent and explicit
  `0` are indistinguishable). Either capture the new key as `Option<u32>` in the
  wire struct, or use the simpler equivalent rule — **migrate when the new value
  is `0`** — which is safe because `SystemValue → 0` is the intended result.

**Bridge API.** `get_mobile_top_bar_margin` / `is_mobile_top_bar_margin_system` /
`get_mobile_top_bar_margin_custom_value` / `set_mobile_top_bar_margin_system` /
`set_mobile_top_bar_margin_custom` → `get_mobile_extra_top_margin() -> i32` and
`set_mobile_extra_top_margin(value: u32)`. `get_status_bar_height()` is kept only
for the informational Settings display (PRD 10, 12).

**Depends on:** nothing.

- [x] 1.0 Replace the top-bar-margin setting with a single "extra top margin"
      in the backend and bridge (PRD 8, 9, 10, 19)
  - [x] 1.1 In `backend/src/app_settings.rs`, replace the `mobile_top_bar_margin`
        field with `mobile_extra_top_margin: u32` (no per-field
        `#[serde(default)]` — the container already has one at line 161), update
        `Default` (line 675) to `0`, and delete the `is_/get_/set_` helpers
        (753–769). Keep `MobileTopBarMargin` (784–788) only as long as the wire
        struct in 1.2 needs it, demoted to `pub(crate)`.
  - [x] 1.2 Add the legacy-value migration **inside `Deserialize`**, not as a
        call-site hook: `#[serde(from = "AppSettingsWire")]` on `AppSettings`
        (or a manual `impl Deserialize`), with the wire struct carrying a
        `#[serde(rename = "mobile_top_bar_margin", skip_serializing)]` capture
        field. Map `SystemValue → 0` and `CustomValue(v) → v`, applying the
        carry-over when the new value is `0` (see the spec above for why "when
        the new key is absent" is not expressible). The old key must not be
        re-serialized.
  - [x] 1.3 Confirm all three deserialization sites pick the migration up with no
        change of their own — `db/appdata.rs:443`, `db/mod.rs:389`,
        `app_data.rs:3237` — and add a test that exercises the `appdata.rs` path
        specifically, since that is the one the running GUI uses.
  - [x] 1.4 Add Rust tests in `backend/`: (a) old JSON with `"SystemValue"` → `0`;
        (b) old JSON with `{"CustomValue": 24}` → `24`; (c) new JSON with
        `mobile_extra_top_margin` → unchanged; (d) a settings blob with neither key
        → `0`; (e) round-trip: serialize after migration and confirm the old key is
        gone; (f) a blob carrying **both** keys → the new one wins and the legacy
        one is dropped.
  - [x] 1.5 In `backend/src/app_data.rs`, collapse
        `set_mobile_top_bar_margin_system()` (2377) and
        `set_mobile_top_bar_margin_custom()` (2398) into one
        `set_mobile_extra_top_margin(value: u32)` that writes the cache and
        persists the row, keeping the existing error handling.
  - [x] 1.6 In `bridges/src/sutta_bridge.rs`, replace the five `#[qinvokable]`
        declarations (1352–1364) and their implementations (4469–4510) with
        `get_mobile_extra_top_margin() -> i32` and
        `set_mobile_extra_top_margin(value: u32)`; keep the
        "APP_DATA not yet initialized" guard but return **0** as the fallback
        (not 24 — Qt supplies the inset now).
  - [x] 1.7 Keep `get_status_bar_height()` (sutta_bridge.rs:1059/3066,
        api.rs:224, cpp/utils.cpp:30) **only** as the informational value for
        Settings (task 2.6). Add a comment at `cpp/utils.cpp:30` stating it is no
        longer used for layout, and why (it reports the status bar, not the safe
        area — PRD §6.2).
  - [x] 1.8 Update the qmllint stubs in
        `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` (434–452): remove the
        five old functions, add the two new ones with correct signatures.
  - [x] 1.9 `cd backend && cargo test` must pass. `make build -B` will **not**
        succeed until 2.0 lands — the QML still calls the old bridge names — so
        1.0 and 2.0 go in the **same commit** (see Notes).

---

### 2.0 — specs

**Naming.** `top_bar_margin` → `extra_top_margin` everywhere, so the property name
states that it is *additional* to Qt's padding. Desktop keeps `0`.

**Decision on `TopicIndexWindow.qml:25`** (currently `is_mobile ? 24 : 5`): set it
to plain `0`, and **do not re-home the `5`**. The desktop `5` is already dead —
line 100's `Component.onCompleted` overwrites the property with
`root.is_mobile ? … : 0`, so desktop users see `0` today and moving the 5px into
the window's layout would *add* spacing that does not currently exist.

**All owner defaults are transient initializers.** Every `is_mobile ? 24 : 0` is
overwritten on `Component.onCompleted` by the bridge read, so 2.3's change to `0`
only affects the frames before completion — which is exactly where the old
default produced a visible jump on mobile. Expect no desktop change at all from
2.3; if one appears, something else is wrong.

**`AppSettingsWindow.qml` owns the property but never reads the bridge** — its
value is assigned by whoever creates the window. It is correctly absent from
2.3's list of bridge-read sites, but 2.2 must still rename its three uses
(lines 189, 356, 930) plus the declaration at 28.

**`DrawerMenu.qml` needs a real fix, not a rename** (PRD 15a) — see 2.10.

**Application mechanism is unchanged** (PRD 13): an anchor margin on the window's
root layout, inside Qt's padding, and forwarded to child windows as a global user
preference. Only the default and the label change.

**Settings section** replaces the checkbox + SpinBox pair with one SpinBox and a
read-only live inset readout:

```
Extra Top Margin
The system status bar and camera cutout are accounted for automatically.
Increase this only if the app's top elements are still covered on your device.

  Extra space (dp): [ 0 ]        System safe area: 34 dp
```

**Depends on:** 1.0 (bridge API + stubs).

- [x] 2.0 Update the QML layer: property/API rename, the reworked Settings
      section, the unpadded `Drawer`, and removal of dead code
      (PRD 11, 12, 13, 15a, 18)
  - [x] 2.1 Delete `assets/qml/MobileTopMarginDialog.qml` and confirm it is
        referenced nowhere and absent from `bridges/build.rs`'s `qml_files`.
  - [x] 2.2 Rename the property `top_bar_margin` → `extra_top_margin` across the
        ~26 QML files (owners, `required property` receivers, and
        `GlobalHotkeysSection.qml`'s forwarder), keeping the pass-down structure.
  - [x] 2.3 Change every owner's default from `is_mobile ? 24 : 0` to `0`, and
        replace the `SuttaBridge.get_mobile_top_bar_margin()` reads with
        `SuttaBridge.get_mobile_extra_top_margin()` (`SuttaSearchWindow.qml:295`,
        `LibraryWindow.qml:40`, `DictionariesWindow.qml:83`,
        `SuttaLanguagesWindow.qml:110`, `TopicIndexWindow.qml:100`,
        `ReferenceSearchWindow.qml:74`, `ChantingPracticeWindow.qml:41`,
        `ChantingPracticeReviewWindow.qml:76`, `DictionaryImportDialog.qml:70`).
        `TopicIndexWindow.qml:25` goes to plain `0` as well — its desktop `5` is
        already overwritten at line 100 and must not be re-homed (see the spec
        above).
  - [x] 2.4 `DownloadAppdataWindow.qml` (26–27) keeps a fixed value but it must
        become `0`: it runs during first-time setup before app data exists, and
        Qt's padding already covers the inset. Update its NOTE comment to say so.
  - [x] 2.5 Rework the Settings section (`AppSettingsWindow.qml:578–640`): drop the
        "use system value" checkbox and the `use_system_margin` /
        `custom_margin_value` state (28, 1235–1240), relabel to "Extra Top Margin"
        with the explanatory text, and leave a single SpinBox (`from: 0`, default
        `0`) that calls `SuttaBridge.set_mobile_extra_top_margin(value)` on
        `onValueModified` and emits `marginChanged()`.
  - [x] 2.6 Add the read-only live system-inset readout next to the SpinBox
        (PRD 12). Prefer the window's own `SafeArea.margins.top`; fall back to
        `SuttaBridge.get_status_bar_height()` only if the attached property is
        unavailable, and label it accurately in each case.
  - [x] 2.7 Verify the `marginChanged()` → `update_*()` chain still refreshes every
        open window live (`AppSettingsWindow.qml:1269` fires it on
        `onAppSettingsReset` too), and that
        `SuttaSearchWindow.qml:294` `update_top_bar_margin()` is renamed
        consistently at both call sites (1332, 2404).
  - [x] 2.8 Grep for stragglers: `grep -rn "top_bar_margin\|mobile_top_bar_margin"
        assets/ backend/ bridges/ cpp/` must return nothing.
  - [x] 2.9 `make build -B` and `make qml-test` must pass; run `qmllint` over the
        changed files to confirm the stubs match.
  - [x] 2.10 Fix `assets/qml/DrawerMenu.qml` (PRD 15a): its `Drawer` root is in
        the window overlay and gets **no** Qt padding, yet it is full-height
        (`height: control.window_height`) and its first child is a top-anchored
        `Label { text: "Menu" }` — so on enforced edge-to-edge it lands under the
        status bar / cutout, and it is the mobile main menu. Add
        `topPadding: SafeArea.margins.top` on the `Drawer` itself (the attached
        property is relative to the item it is attached to, so this does not
        double-count) rather than plumbing `extra_top_margin` into it.
  - [x] 2.11 Sweep the rest of the `Popup` family for the same exposure: any
        inline `Dialog`/`Popup`/`Menu` that is top-anchored or tall enough to
        reach a bar. Centered dialogs need nothing. Record which were checked so
        6.7's on-device audit has a list rather than starting cold.

        **Result of the sweep** (all 55 `Dialog`/`Popup`/`Drawer`/`Menu` roots in
        `assets/qml/`): `DrawerMenu.qml` was the **only** top-anchored one, and it
        is fixed by 2.10. Every other `Dialog` is centered (`anchors.centerIn:
        parent`, or `x/y: (parent.w|h - w|h) / 2` in `TabListDialog.qml`), and the
        `Menu` popups in `SuttaSearchWindow.qml` open from toolbar buttons that sit
        below the inset. Confirmed in Qt's source that a `Drawer` can take the
        attachment at all: `QQuickPopup` implements `QQuickSafeAreaAttachable`
        (`qquickpopup.cpp:3443`, returning `popupItem()`), so `SafeArea` on a
        `Popup` resolves to the popup item rather than warning.

        Centered is not automatically safe, though — a centered dialog sized to
        nearly the full window height leaves only a few px of clearance, which is
        less than a ~34 dp inset. **Watch-list for 6.7**, in descending order of
        exposure:
        - `DocumentImportDialog.qml:10` — `height: Math.min(500, parent.height - 40)`
          → 20 px top clearance whenever the window is under ~540 px tall.
        - `DocumentMetadataEditDialog.qml:9` — same `parent.height - 40` form.
        - `TabListDialog.qml:38–41` — up to `parent.height * 0.9` when `!is_tall`.
        - `GlossTab.qml:3422` — fixed `height: 500`.
        - `DatabaseValidationDialog.qml:339`, `ChantingPracticeWindow.qml`'s
          `anchors.fill: parent` content dialogs — tall on a phone.

        None were changed: per PRD 14/15 the fix belongs on the edge that a device
        actually shows a problem on, and a phone screenshot decides it.
  - [x] 2.12 **Check the margins visually before moving on** — the whole of 2.0
        is a spacing change, and neither `make qml-test` nor `qmllint` can see a
        wrong gap. This is the one place a human has to look:
        - **Desktop** (agent-safe to ask for, human-run per the GUI rule): open
          `TopicIndexWindow` plus a couple of the renamed windows and confirm
          nothing moved. The expected result is **no visible change at all**,
          since every default was already being overwritten on
          `Component.onCompleted` — a shifted layout means the rename dropped a
          binding or the `TopicIndexWindow` `5` was re-homed by mistake.
        - **Mobile**: the top gap becomes a single inset, and there is no
          longer a jump between first paint and `Component.onCompleted` (the old
          `24` default differed from the resolved value; `0` matches it). Full
          coverage is task 6.0 — this is the quick "did the rename break
          spacing" pass, not the edge-to-edge audit.

        **Result:** desktop checked and unchanged, as predicted. The mobile
        half was **deferred to task 6.0** and will be checked against the
        uploaded build rather than a local deploy.

---

### 3.0 — specs

**One-line change with three enforced behaviours behind it** — the comment is part
of the deliverable (PRD 2). `minSdkVersion` stays 27; `compileSdk` keeps coming
from androiddeployqt (`android-36`), with AGP 8.6.0 and the JDK 17–21 pin
untouched (PRD 38).

Confirmed that 3.1 is the whole target-level change: `android/AndroidManifest.xml`
has no `<uses-sdk>` element and `CMakeLists.txt` sets no
`QT_ANDROID_TARGET_SDK_VERSION`, so `build.gradle`'s `defaultConfig` is the only
source. Note androiddeployqt also writes `qtTargetSdkVersion=35` into the
*generated* `gradle.properties` and `build.gradle` never reads it — that line
will still say 35 afterwards and means nothing. Verify with `aapt2 dump badging`
(task 5.7), not by reading generated properties.

**Debug-variant disable must be conditional** — `make android-apk-debug` depends on
the debug variant existing, so key it off a Gradle property that
`build-android.sh` sets only for release builds (PRD 36). **There is no `-P`
pass-through**: the script runs `cmake -S . -B` then `cmake --build --target
aab`, and Gradle is invoked by androiddeployqt. Deliver the property through
Gradle's environment mapping instead — `ORG_GRADLE_PROJECT_simsapaReleaseOnly`
(PRD 36a).

**Depends on:** nothing (independent of 1.0/2.0).

- [x] 3.0 Move the Android build to targetSdk 36 and stop building the debug
      variant during release builds (PRD 1, 2, 3, 20, 35, 36, 37, 38, 39)
  - [x] 3.1 Set `targetSdkVersion 36` in `android/build.gradle` `defaultConfig`,
        with a comment recording the three enforced API 36 behaviours
        (edge-to-edge with no opt-out, predictive back by default, large-screen
        orientation attributes ignored).
  - [x] 3.2 Add `android.suppressUnsupportedCompileSdk=36` to
        `android/gradle.properties` with a comment pointing at the AGP analysis in
        `AGENTS.md`. **It is not there today** — the file ends at
        `android.useAndroidX=true`, so every build currently prints the warning
        that `AGENTS.md:509` claims is already suppressed. Reconcile that
        sentence as part of 7.6. (Confirmed safe: androiddeployqt *appends* its
        generated keys to the copied file rather than overwriting it — the
        generated `android-build/gradle.properties` still carries our
        `org.gradle.parallel` and `android.useAndroidX` lines.)
  - [x] 3.3 Add the `androidComponents { beforeVariants(selector().withBuildType(
        "debug")) { it.enable = !project.hasProperty("simsapaReleaseOnly") } }`
        block to `android/build.gradle`, with a comment explaining that
        androiddeployqt appends the bare `bundle` task, which otherwise drags in
        the whole debug variant.
  - [x] 3.4 In `build-android.sh`, `export
        ORG_GRADLE_PROJECT_simsapaReleaseOnly=true` **only** for release builds.
        Gradle maps `ORG_GRADLE_PROJECT_<name>` env vars to project properties,
        so `project.hasProperty("simsapaReleaseOnly")` works with no
        androiddeployqt plumbing. **For `--debug` the variable must be left
        unset — never set to `false`**, since `hasProperty` is true for any
        value including `false` and the empty string.

  **3.5–3.7 are deferred to the single Android build run scheduled before task
  6.0** — they are all verification of the edits above and each needs a full
  multi-ABI build, so they are batched with 4.10/4.11 and 5.0 rather than
  building three separate times. The "do not touch AGP / the Gradle wrapper /
  the NDK / the JDK pin" half of 3.7 is already satisfied: none of those files
  were modified.
  - [x] 3.5 Build a signed AAB and confirm the log contains **no** `:*Debug*`
        packaging tasks (previously 43), the release AAB is still produced and
        signed, and `build/outputs/bundle/debug/` is not created.
  - [x] 3.6 Confirm `make android-apk-debug` still succeeds with the debug variant
        enabled.

        **Confirmed 2026-07-30.** The debug variant builds, packages and (with
        the new `--sign`) signs. Note the debug variant is now also the local
        **beta** build — see the 8.0 section below — so its package id is
        `io.github.simsapa.app.beta`. `make android-apk-debug` itself is
        unchanged and still produces the unsigned artifact.
  - [x] 3.7 Do **not** touch AGP, the Gradle wrapper, the NDK or the JDK pin;
        confirm the build still reports AGP 8.6.0 / Gradle 8.12 / JDK 21 (PRD 38).

  - [x] 3.8 **(added during 5.7, not in the original plan)** Fix the launcher
        name. `aapt2 dump badging` showed `application-label:'simsapadhammareader'`
        — the name under the launcher icon and on the Play install screen.
        `android/AndroidManifest.xml:87` is `android:label="-- %%INSERT_APP_NAME%%
        --"`, and with no `QT_ANDROID_APP_NAME` set androiddeployqt substitutes
        the **CMake target name**. Fixed by setting the target property in
        `CMakeLists.txt`'s `if (ANDROID)` block (`QT_ANDROID_APP_NAME "Simsapa"`),
        which feeds the placeholder through the same deployment-settings path as
        `QT_ANDROID_VERSION_*` — deliberately *not* by hardcoding
        `android:label`, so the placeholder mechanism keeps working. Unlike the
        permission/feature markers, this one is worth keeping. Verified:
        `application-label:'Simsapa'`, everything else byte-for-byte equivalent.

  **Results of the 2026-07-28 clean `make android-rebuild`** (log
  `/tmp/aab-build.log`, versionCode 3 / versionName 1.0.0-alpha.3):

  - **3.5 confirmed.** Zero debug-variant Gradle tasks (was 43). The only two
    log lines matching `Debug` are `:stripReleaseDebugSymbols` and
    `:mergeReleaseNativeDebugMetadata`, both *release*-variant tasks that merely
    contain the word — grep for `^> Task .*[Dd]ebug` and read the results, do not
    just count them. `outputs/bundle/` and `outputs/apk/` contain `release` only.
    `BUILD SUCCESSFUL`, 55 actionable tasks, AAB signed (v2 + v3 schemes).
    The script echoed `==> Debug variant: disabled (release build)`.
  - **3.7 confirmed** with one correction below: Qt 6.9.3, JDK 21.0.11, NDK
    27.3.13750724, AGP 8.6.0. The AGP "tested up to compileSdk 35" warning is
    **gone**, which is the positive confirmation that 3.2 took effect.
  - **The Gradle wrapper is OURS, not Qt's** — `android/gradle/wrapper/` is
    checked in (tracked since commit f8eaafd) at **8.10**, and androiddeployqt
    copies it into `android-build/` with the rest of `android/`. All three Qt
    6.9.3 kits ship 8.12, but that copy is never used. This contradicts
    `AGENTS.md:521-522` ("The Gradle wrapper is Qt's, not ours … Qt 6.9.3 ships
    the wrapper at 8.12") and weakens one of the three stated reasons for the AGP
    pin: bumping the wrapper for AGP 8.11+ (which needs Gradle 8.13) would **not**
    mean diverging from a Qt-provided file. The other two reasons stand (the
    JDK/lint coupling, and `build.gradle` being a Qt template using APIs removed
    in AGP 9). **The pin should still stay** — this changes the rationale, not the
    decision. Fix `AGENTS.md` in 7.6 along with the line-509 correction.

---

### 4.0 — specs

**Flow:** `android/version.txt` (versionCode) + `bridges/Cargo.toml` (`version`) →
parsed by `build-android.sh` → exported as `ANDROID_VERSION_CODE` /
`ANDROID_VERSION_NAME` → read by `CMakeLists.txt` from the environment →
`QT_ANDROID_VERSION_*` → manifest placeholders.

**Removing `CACHE` is load-bearing** (PRD 31): a cache variable is written once per
build directory, so editing a value would otherwise have no effect on an existing
`build/android-multiabi/` tree.

**Developer builds must not fail** (PRD 32): with the env vars absent, skip the
`QT_ANDROID_VERSION_*` properties entirely and let Qt default to versionCode 1 /
versionName "1.0", with a `message(STATUS)`.

**"Absent" means empty, not unset.** The `Makefile` (109–110) exports both names
unconditionally, and GNU make exports an *undefined* variable as an **empty
string** (verified) — so every plain `make android-aab` arrives with both set and
empty. Test with `[ -n "${VAR:-}" ]` in the shell (the script's existing idiom is
already right) and `if(NOT "${X}" STREQUAL "")` in CMake; an is-set test would
read make's empty export as an explicit override and defeat the file parsing.

**CMake reads `$ENV{}` at configure time only.** `build-android.sh` re-runs
`cmake -S . -B` on every invocation, so the release path always picks up an
edited file — that is what makes 4.10 pass. A bare `cmake --build` or a Qt
Creator build reuses the last configure; out of scope, but say so.

**Depends on:** nothing.

- [x] 4.0 Make the version values flow from `android/version.txt` and
      `bridges/Cargo.toml` through the environment into CMake (PRD 28–34)
  - [x] 4.1 Create `android/version.txt` with the comment header and the single
        value `3` (Play currently has 2).
  - [x] 4.2 In `build-android.sh`, add a parser that reads the first non-blank,
        non-`#` line of `android/version.txt` and validates it as a positive
        integer; **do not** `source` the file.
  - [x] 4.3 In `build-android.sh`, parse the package `version` from
        `bridges/Cargo.toml` (the `[package]` version near the top — not a
        dependency's) with a targeted regex.
  - [x] 4.4 Export both as `ANDROID_VERSION_CODE` / `ANDROID_VERSION_NAME`, letting
        a **non-empty** inherited environment value win (`[ -n "${VAR:-}" ]`, not
        an is-set test — see the spec above), and record which source was used.
  - [x] 4.5 Fail **before** the CMake configure with an actionable message if
        either file is missing or unparseable, or the versionCode is not a positive
        integer (replacing the current warn-only check at build-android.sh:306).
  - [x] 4.6 Echo `versionCode` / `versionName` and their sources at the start and
        again at the end of the build next to the artifact path (replacing the
        current lines 298–299).
  - [x] 4.7 Drop the `-DANDROID_VERSION_*` arguments (build-android.sh:315–316) now
        that CMake reads the environment.
  - [x] 4.8 In `CMakeLists.txt`, replace the two `CACHE STRING` version variables
        (140–141) with plain `$ENV{...}` reads, and guard the
        `QT_ANDROID_VERSION_*` properties (353–357) so they are only set when both
        are **non-empty** (`if(NOT "${X}" STREQUAL "")` — make exports undefined
        variables as empty); emit a `message(STATUS)` otherwise. Also update the
        stale example in the comment at 137 (`make android-aab
        ANDROID_VERSION_CODE=3 …`).
  - [x] 4.9 Update the `Makefile` Android targets and comments (90–134) to drop
        `ANDROID_VERSION_CODE=<n> ANDROID_VERSION_NAME=<v>` from the documented
        command line; keep the `export` lines so an explicit override still works.
  - [x] 4.10 Verify with `aapt2 dump badging`: a build with no arguments carries
        versionCode 3 and the Cargo version name; then edit `android/version.txt`
        to 4, rebuild **in the same build directory via `build-android.sh`**, and
        confirm the manifest changes (the old `CACHE` behaviour would not). The
        script's unconditional `cmake -S . -B` re-configure is what makes this
        work; a bare `cmake --build` is expected to keep the old value.
  - [x] 4.11 Verify a plain `cmake` configure without the env vars succeeds and
        logs the STATUS message instead of failing.

  **4.10 and 4.11 verified 2026-07-29.**

  - **4.10 PASS.** The alpha.3 release itself was the test: `android/version.txt`
    was edited from `3` to `4` and `make android-aab` re-run **in the same build
    directory**, and `aapt2 dump badging` on the resulting APK reports
    `versionCode='4' versionName='1.0.0-alpha.3'`. Under the old `CACHE`
    behaviour it would still have read 3. The build script's unconditional
    `cmake -S . -B` re-configure is what makes this work.
  - **4.11 PASS.** A `qt-cmake` Android configure with `ANDROID_VERSION_CODE` /
    `ANDROID_VERSION_NAME` explicitly removed from the environment (`env -u`)
    configures cleanly and logs:
    `-- ANDROID_VERSION_CODE / ANDROID_VERSION_NAME not set in the environment;
    Qt will default to versionCode 1 / versionName "1.0". …`

  Also verified without a build:
  - Both parsers, against the real files — `android/version.txt` → `3`,
    `bridges/Cargo.toml` `[package]` version → `1.0.0-alpha.2`.
  - The versionCode rejection cases: comments-only file, non-numeric, `0`, and
    whitespace/blank-line padding (`  4  ` → `4`).
  - `bash -n build-android.sh`, and a full `cmake -S . -B` configure of the
    edited `CMakeLists.txt` (desktop, throwaway build dir) — parses and
    configures clean. This does **not** cover the `if (ANDROID)` branch, which
    is what 4.11 is for.

---

### 5.0 — specs

The two deferred investigations are **decisions with evidence**, not code changes.
Record the numbers and the conclusion in the docs (task 7.0) whichever way they go.

**Depends on:** 3.0 and 4.0 (needs a real signed multi-ABI bundle).

- [ ] 5.0 Build and statically verify the signed multi-ABI bundle, and close the
      two deferred packaging investigations (PRD 4, 5, 41, 42, 43, 44)
  - [x] 5.1 Run a full clean `make android-rebuild` (arm64-v8a; x86_64;
        armeabi-v7a) and confirm it completes with Qt 6.9.3 / NDK 27.3 / JDK 21 /
        AGP 8.6.0.
  - [x] 5.2 Confirm the existing artifact checks in `build-android.sh` still pass:
        no cross-ABI staged libraries, correct ELF machine type per ABI,
        `zipalign -c -P 16`.
  - [x] 5.3 Investigate the `QML import could not be resolved:
        com.profoundlabs.simsapa` warning: compare `assets/android_rcc_bundle/qml/`
        in the built package against the QML the app imports, and confirm the app's
        own module is compiled into the binary via `cxx_qt_import_qml_module`
        rather than shipped as a plugin directory.
  - [x] 5.4 Classify the remaining import warnings (`QtWebEngine`,
        `QtWayland.Compositor`, `QtQuick.Controls.{Windows,macOS,iOS}`,
        `QtQuick3D.MaterialEditor`) as harmless-by-construction, with the reason
        for each.
  - [ ] 5.5 Measure `useLegacyPackaging` **both ways**: AAB/APK size, on-device
        install footprint, `zipalign -c -P 16`, and `readelf -lW` `p_align` of the
        app `.so` and a Qt lib. Record the numbers.
  - [ ] 5.6 Decide on `useLegacyPackaging` from those measurements — keep `true`
        unless they favour changing it — and note the decision for task 7.0.
  - [x] 5.7 Verify the bundle's device catalogue expectations with
        `aapt2 dump badging`: `targetSdkVersion 36`, `minSdkVersion 27`, no
        unexpected `uses-permission`, every `uses-feature` `required="false"`.
        This is the **only** trustworthy check of the target level — the
        generated `gradle.properties` will still read `qtTargetSdkVersion=35`
        (androiddeployqt writes it; `build.gradle` never reads it).
  - [x] 5.8 Record the `qtMinSdkVersion=28` finding: Qt **6.9.3** already
        declares an Android floor of 28 in the generated `gradle.properties`, and
        `build.gradle` overrides it down to 27 — so the app has shipped one level
        below Qt's declared minimum since the 6.9.3 move. Qt 6.10 does not
        introduce that constraint, it removes our ability to keep overriding it.
        Feeds 7.4 and PRD open question 4; no code change here.

  **Results of the 2026-07-28 clean `make android-rebuild`:**

  - **5.1 done.** Clean rebuild (`rm -rf build/android-multiabi` first),
    arm64-v8a + x86_64 + armeabi-v7a, Qt 6.9.3 / NDK 27.3.13750724 / JDK 21.0.11
    / AGP 8.6.0. `BUILD SUCCESSFUL in 51s` for the Gradle phase.
  - **5.2 done, but the sub-task's premise was wrong.** `build-android.sh`
    contains only **two** artifact checks — the cross-ABI contamination check
    (filename-suffix based: a lib whose `_<abi>.so` suffix disagrees with its
    directory) and the ChromeOS merged-manifest audit. There is **no** ELF
    machine-type check and **no** `zipalign` check in the script. Both were run
    by hand instead:
    - ELF machine type is correct per ABI: arm64-v8a → `AArch64`, armeabi-v7a →
      `ARM`, x86_64 → `Advanced Micro Devices X86-64`, checked on both the app
      lib and `libQt6Core`.
    - 16 KB alignment, swept over **every** lib rather than sampled: arm64-v8a
      139/139 and x86_64 139/139 at `p_align=0x4000`. armeabi-v7a is 4 KB
      (`0x1000`) for all but the app's own `.so` — **correct and expected**, the
      16 KB page requirement applies only to 64-bit ABIs.
    - `zipalign -c -P 16 4 <apk>` → PASS.
    - Cross-ABI check: OK, every library matches its ABI directory.
    > If these checks are wanted on every build, they have to be **added** to
    > `build-android.sh` — they are not there today.
  - **5.7 done.** `aapt2 dump badging` on the release APK:
    `package name='io.github.simsapa.app' versionCode='3' versionName='1.0.0-alpha.3'`,
    `minSdkVersion:'27'`, `targetSdkVersion:'36'`, `native-code: 'arm64-v8a'
    'armeabi-v7a' 'x86_64'`. Permissions are exactly INTERNET,
    ACCESS_NETWORK_STATE, RECORD_AUDIO, MODIFY_AUDIO_SETTINGS and the app's own
    DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION — no CAMERA, no location, no
    Bluetooth. Every `uses-feature` is reported as `uses-feature-not-required`.
    This also closes the first half of **4.10** (the version flow reaches the
    manifest).
  - **5.8 confirmed** from the generated `android-build/gradle.properties`:
    `qtMinSdkVersion=28` while `build.gradle` declares `minSdkVersion 27`, and
    `qtTargetSdkVersion=35` sits there unread next to a manifest that really is
    targetSdk 36 — exactly the trap 5.7 warns about.
  - **5.3 answered.** The app's own QML module is **not** shipped as a plugin
    directory: `assets/android_rcc_bundle/` does not exist in the package at all,
    and the module lives inside the app binary as Qt resources — `strings` on
    `libsimsapadhammareader_arm64-v8a.so` shows
    `:/qt/qml/com/profoundlabs/simsapa/…` paths and the `<qresource
    prefix="/qt/qml/com/profoundlabs/simsapa/assets/qml">` header, put there by
    `cxx_qt_import_qml_module` (`bridges/build.rs:109`). androiddeployqt's import
    scanner walks *disk* import paths, so a compiled-in module is unresolvable by
    construction. **Harmless; expected to persist.**
  - **5.4 classified.** None of the five are imported by app QML except
    `QtWebEngine`:
    - `QtWebEngine` — imported only by `SuttaHtmlView_Desktop.qml` and
      `DictionaryHtmlView_Desktop.qml`. Qt WebEngine has no Android port; the
      desktop-only views are never instantiated there (Android uses QtWebView).
      The scanner reads every file regardless of platform. Harmless.
    - `QtQuick.Controls.Windows` / `.macOS` / `.iOS` — other platforms' styles,
      pulled in by QtQuick.Controls' own module metadata and absent from the
      Android kit. Harmless.
    - `QtWayland.Compositor`, `QtQuick3D.MaterialEditor` — not imported by any
      app QML (`grep -rl "^import …" assets/qml/` returns nothing); transitive
      references from Qt's own modules. Harmless.

---

### 6.0 — specs

**Human-run**, on a physical **Android 16** phone (edge-to-edge enforcement does not
apply below Android 16 at targetSdk 36, so a 15 device cannot validate it). Build
with `make android-apk` and **sideload** — a Qt Creator deploy triggers the
spurious "isn't 16 KB compatible" dialog and its kits are single-ABI.

This task produces two decisions: predictive back (PRD 22) and whether any edge
other than the top needs its own knob (PRD 14).

**Depends on:** 1.0, 2.0, 3.0, 4.0.

- [ ] 6.0 Human on-device verification pass on the Android 16 phone, and the
      decisions that depend on it (PRD 6, 14, 15, 16, 17, 21, 22, 24, 25)
  - [x] 6.1 Confirm Qt still reports non-zero safe-area margins at targetSdk 36:
        the top gap must be **one** inset, not zero and not doubled (PRD 17). This
        underpins the whole design — check it first.
  - [ ] 6.2 Portrait: search bar, toolbar and every bottom control fully visible
        and tappable, in the main window and in each secondary window (Settings,
        Library, Dictionaries, Sutta Languages, Topic Index, Reference Search,
        Chanting Practice, About) — each is its own `ApplicationWindow`.
  - [ ] 6.3 Landscape: rotate in each main window; check the cutout side and
        rounded corners; confirm margins update without a restart, including while
        a dialog is open.
  - [x] 6.4 Soft keyboard: focus a search field and a multi-line field; the focused
        input stays visible and the keyboard raises on the first tap
        (`docs/android-soft-keyboard.md`).
  - [x] 6.5 Settings → Extra Top Margin: with `0`, the gap is a single inset; raise
        it and confirm the space appears immediately and survives a restart;
        confirm the system-inset readout updates on rotation.
  - [x] 6.6 Upgrade path: install over an existing install **with** a custom margin
        (layout must be unchanged) and over one **without** (the doubled gap must be
        gone).
  - [x] 6.7 Audit the cases Qt does not pad (PRD 15), working from the list task
        2.11 produced: inline `Dialog`/`Popup` items (tall or top-anchored ones
        especially), any mobile-visible `header`/`footer`/`menuBar`, and
        `Flickable`/`ListView` content scrolling under an edge. **Check the
        drawer menu explicitly** — open it in portrait and landscape and confirm
        the "Menu" label clears the status bar / cutout with the 2.10 fix in
        place.
  - [x] 6.8 Sutta reader (WebView): open a sutta, scroll to top and bottom, use the
        find bar, switch display layouts; confirm no HTML content sits under a
        system bar (PRD 16).
  - [x] 6.9 Back navigation: system back gesture and hardware back from a dialog,
        a secondary window and the main window. **Record the behaviour** — this
        decides 6.10.
  - [x] 6.10 If back regressed, add `android:enableOnBackInvokedCallback="false"`
        to the `<activity>` in `android/AndroidManifest.xml` with a comment that
        the opt-out is temporary; if it did not, change nothing. Either way the
        result feeds task 7.0.
  - [ ] 6.11 Large screens: if a tablet or Chromebook is available, resize and
        rotate; confirm nothing depends on a fixed orientation. Do **not** add
        `PROPERTY_COMPAT_ALLOW_RESTRICTED_RESIZABILITY` unless a concrete problem
        appears (PRD 25).
  - [x] 6.12 Exercise the remaining ABI-sensitive paths: first-run asset download,
        fulltext search (tantivy), dictionary lookup, chanting record/playback,
        file save via SAF.
  - [ ] 6.13 If a 32-bit ARM device is available, repeat 6.1, 6.8 and 6.12 on it.
  **Results of the 2026-07-28 on-device pass (Android 16 phone, versionCode 3):**

  - **6.1 PASS** — top margin correct: one inset, not zero, not doubled.
  - **6.4 PASS** — keyboard raises on first tap for the search input and the
    gloss text input.
  - **6.5 PASS with a layout defect, now fixed.** Value takes effect immediately,
    survives restart, and the safe-area readout updates on rotation. But the
    "System safe area: N dp" label sat *beside* the SpinBox, where portrait
    leaves too little width. Moved onto its own row **below** the SpinBox
    (`AppSettingsWindow.qml`), with a spacer `Item` absorbing the leftover width
    on the SpinBox row.
  - **6.7 PASS** — the `DrawerMenu` "Menu" label clears the status bar / cutout
    in portrait *and* landscape, confirming the 2.10 `topPadding:
    SafeArea.margins.top` fix.
  - **6.8 PASS** — sutta reader WebView and find bar both fine.
  - **6.9 FAILED — predictive back is a hard regression.** Back closed the whole
    app in every case tested: from the sutta reader (should open the tab list
    dialog), from the tab list dialog, from the search help dialog, and from the
    Chanting Practice window (each should have dismissed itself).
    **Root cause:** targetSdk 36 enables predictive back, which stops the system
    dispatching legacy `KEYCODE_BACK` key events and instead expects an
    `OnBackInvokedCallback`. Qt 6.9.3 registers none — grepping Qt's
    `android/java` sources finds **neither** `onBackPressed` **nor**
    `OnBackInvokedCallback` — and the app registers none either, because Qt Quick
    Controls dismiss a `Dialog`/`Popup`/`Window` off the `Qt::Key_Back` event the
    legacy path delivers. Nothing handles back, so the system default finishes
    the activity.
  - **6.10 APPLIED** — `android:enableOnBackInvokedCallback="false"` on the
    `<activity>` in `android/AndroidManifest.xml`, with a comment recording that
    it is temporary and must be removed once Qt implements the callback (at which
    point the dialogs need re-testing, since predictive back also changes the
    gesture animation). **Needs a rebuild + re-test to confirm the fix.**
  - **6.14 Decision: NO.** Bottom margin is correct as shipped; no edge other
    than the top needs its own knob.
  - **6.11 partial** — a Chromebook user confirmed the app **installs**, which
    closes the original "not compatible on Chromebook" report and validates the
    x86_64 ABI + the required-feature fixes. The resize/rotate half is untested.
  - **6.12 partial** — audio record/playback works (the pure-Rust `cpal` stack on
    a real device). First-run asset download, fulltext search, dictionary lookup
    and SAF file save are still untested.
  - **6.2 / 6.3 partial** — Chanting Practice was opened and rotation was
    exercised via the safe-area readout, but the per-window sweep (Settings,
    Library, Dictionaries, Sutta Languages, Topic Index, Reference Search, About)
    was not done. Each is its own `ApplicationWindow` with its own padding.
  - **6.6 NOT DONE** — needs alpha.2 installed with a custom margin, then alpha.3
    over it. This is the only check that exercises the `CustomValue(v) → v` serde
    migration on a real device.
  - **6.13 NOT DONE** — no 32-bit ARM device to hand.

  - [x] 6.14 Decide whether any edge other than the top needs its own knob
        (PRD 14) — default answer is **no**; record the finding either way.

---

### 7.0 — specs

Documentation is a deliverable here, not an afterthought: several of these findings
(the deprecated Qt Java calls, the AGP pin, the doubled-gap diagnosis) exist
specifically so the next Play report or version bump does not restart the
investigation.

**Depends on:** 5.0 and 6.0 (their results are what gets written down).

- [x] 7.0 Documentation, recorded decisions, and cleanup (PRD 23, 26, 27, 40,
      45, 46, 47, 48, and the closed decisions 49–52)
  - [x] 7.1 Write `docs/android-edge-to-edge-and-safe-areas.md`: Qt's
        `ApplicationWindow` padding supplies the safe area
        (`qquickapplicationwindow.cpp:801-805`); the app's setting is only *extra*
        clearance; why the default had to become 0 (the doubled-gap diagnosis and
        the Qt 6.8.3 → 6.9.3 history); why `status_bar_height` is not the safe area;
        the cases Qt does not pad; and how to test it. Two rules must be stated
        as rules, not asides: **never assign `topPadding`/`padding` on an
        `ApplicationWindow` root** (it silently replaces Qt's binding —
        `binding.installOn()` at `qquickapplicationwindow.cpp:793`), and
        **`Popup`-family items get no padding**, with `DrawerMenu.qml` as the
        worked example.
  - [x] 7.2 Record in that doc that the three Play-reported deprecated APIs
        (`Window.getStatusBarColor`, `setStatusBarColor`, `setNavigationBarColor`)
        live in Qt's own Java — `QtActivityDelegateBase.java:108`,
        `QtDisplayManager.java:191/192/200/204` — are no-ops at API 36, and can only
        be removed by a Qt upgrade (PRD 26, 27).
  - [x] 7.3 Record the predictive-back result and decision from 6.9/6.10, and the
        large-screen finding from 6.11 (PRD 23, 24).
  - [x] 7.4 Write `docs/android-qt-upgrade-considerations.md` from PRD §7.5: the
        reasons to upgrade, and the pitfalls — Qt 6.10.1's libtiff SONAME and
        WebEngine-on-FUSE AppImage crash (`docs/qt-6.10.1-appimage-issues.md`),
        Qt 6.10's minSdk 28 floor vs our 27, the NDK constraint, cxx-qt exposure,
        the multi-ABI mechanism, and that desktop/Windows/macOS ride the same Qt.
        Include the AGP/Gradle-wrapper/JDK coupling as part of that upgrade
        (PRD 40). State the minSdk position correctly per 5.8: Qt 6.9.3 already
        declares `qtMinSdkVersion=28` and we override to 27, so the upgrade
        removes an override we are already relying on rather than imposing a new
        floor.
  - [x] 7.5 Update `docs/android-multi-abi-and-chromeos.md` for targetSdk 36, the
        release-only debug-variant switch (and that it rides on
        `ORG_GRADLE_PROJECT_simsapaReleaseOnly`, not a `-P` argument), and the
        version workflow.
  - [x] 7.6 Update `AGENTS.md` (`CLAUDE.md` is a symlink): the new targetSdk, the
        release procedure (edit `android/version.txt`, then `make android-aab` —
        no version arguments), and links to the two new docs. **Correct line 509**,
        which states that `android.suppressUnsupportedCompileSdk=36` is already in
        `android/gradle.properties` — it was not until task 3.2. The rest of the
        AGP section stays as is.
  - [x] 7.7 Record the `useLegacyPackaging` measurements and decision from 5.5/5.6,
        and the QML-import-warning conclusions from 5.3/5.4.
  - [x] 7.8 Record the closed decisions (PRD 49–52) where they belong: keep
        `armeabi-v7a` (we have users on 32-bit ARM phones), no 32-bit `x86`, do not
        remove `package=` from the manifest, bundle size needs no action.
  - [x] 7.9 Confirm `tasks/android-packaging-follow-ups.md` is deleted (it is —
        never committed) and nothing references it; update `PROJECT_MAP.md` if the
        new docs belong in its index.
  - [x] 7.11 **(added 2026-07-30)** Document the beta package, the
        `make android-beta-*` targets and the Play update-policy gating —
        `docs/android-beta-distribution-and-play-policy.md`, plus index entries
        in `AGENTS.md`, `PROJECT_MAP.md` and
        `docs/android-multi-abi-and-chromeos.md`. See section 8.0.
  - [x] 7.10 Final check: `make test` passes, and the release procedure works
        end-to-end from a clean tree — edit `android/version.txt`, `make
        android-aab`, verify with `aapt2 dump badging`.

---

### 8.0 — beta package, on-device debugging, Play update policy

**Added 2026-07-30, not in the original PRD.** It came out of trying to run task
3.6's debug APK on the test phone: the phone carries the closed-testing build
from Play, and nothing locally built can be installed over it.

Full write-up:
[docs/android-beta-distribution-and-play-policy.md](../docs/android-beta-distribution-and-play-policy.md).

**The finding that forced the design.** A Play install is signed by **Play App
Signing** — Google's key (`CN=Android, O=Google Inc.`, `fdf35925…`), not our
upload key (`CN=Simsapa, O=Profound Labs`, `fef4991a…`). Android has no key-swap
path, so no local build can replace it, whatever key it carries. Two non-causes,
checked so they are not re-checked: Play does **not** rename the package for a
testing track (the id is plain `io.github.simsapa.app`; it is a split install
with `installerPackageName=com.android.vending`), and the mismatch has nothing to
do with debuggable-vs-release. Uninstalling would have wiped the downloaded
`appdata.sqlite3` and the state task 6.6 needs.

- [x] 8.1 Beta package `io.github.simsapa.app.beta`, label "Simsapa (beta)",
      installing **alongside** the released app —
      `applicationIdSuffix` / `versionNameSuffix` in `android/build.gradle` plus
      the new `android/AndroidManifest.beta.xml` label overlay. The FileProvider
      authority (`${applicationId}.qtprovider`) and `androidx-startup` follow the
      suffix automatically; verified no collision. Trade-off accepted: the beta
      gets its own data dir and runs first-time asset setup.
- [x] 8.2 `build-android.sh --sign` — signs a `--debug` build with the release
      keystore, order-independent with `--debug`/`--no-sign`. Signing is done
      **after** the build with `apksigner` (androiddeployqt's `--sign` path is
      release-only; Gradle's debug-keystore signature is replaced), and the
      resulting signer is printed as proof. Verified: `CN=Android Debug` →
      `CN=Simsapa`, `debuggable` retained, `zipalign -c -P 16` still PASS.
- [x] 8.3 `build-android.sh --beta` — the **not-debuggable** dist beta, via
      `ORG_GRADLE_PROJECT_simsapaBeta` on the *release* build type. A third
      Gradle build type is impossible: androiddeployqt only ever invokes
      `assembleDebug`/`assembleRelease`. `--aab --beta` and `--aab --debug
      --sign` are rejected.
- [x] 8.4 **Package-identity guard.** ninja's `apk` target does not depend on a
      Gradle property, so `--beta` followed by plain `--apk` in one build
      directory skipped androiddeployqt and reported the previous artifact —
      observed as a plain release build printing
      `package: name='io.github.simsapa.app.beta'`. The script now records
      `<BuildType>-beta<0|1>` in `$ANDROID_BUILD_DIR/.simsapa-package-identity`
      after each successful package and force-deletes the packaging **outputs**
      when it changes or is **unknown** (an unknown marker must not count as a
      match). Never `android-build/` itself — that wedges the ExternalProject
      stamps.
- [x] 8.5 Makefile targets: `android-beta-debug`, `android-beta-debug-install`,
      `android-beta-debug-run` (launch + `adb logcat`, filtered by **tag** not
      pid so startup messages are not lost), and `android-beta-dist` (copies to
      `dist/Simsapa-<version>-beta.apk`).
- [x] 8.6 **Play update-policy gating.** An app distributed through Play must
      update only through Play (Device and Network Abuse), but
      `UpdateNotificationDialog` offered an off-Play download link on Android
      with no gate — a pre-existing exposure, independent of the beta.
      Now `SuttaBridge.is_installed_from_play_store()` /
      `get_play_store_url()` (backed by `get_installer_package_name()` /
      `get_android_package_name()` in `cpp/utils.cpp`) switch the dialog between
      a `market://` **Open Google Play** button and the usual release-page link.
      Keyed on the **install**, not the build, so a sideloaded release APK keeps
      the link; false off Android, so desktop is unchanged. `open_visit_url()`
      routes through the same gate.
- [x] 8.7 Verification: `aapt2 dump badging` on real artifacts — dist beta
      (`.beta`, `Simsapa (beta)`, **no** `application-debuggable`, upload-key
      signed), debug beta (same id, `-beta-debug`, debuggable), plain release
      (`io.github.simsapa.app`, `Simsapa`, not debuggable). Identities
      round-tripped release → beta → release → debug-beta in one build directory,
      correct each time. Debug beta installed on the phone next to the untouched
      Play copy, log streaming confirmed live. Desktop `make build -B` passes;
      `qmllint` clean on the changed dialog; `make qml-test` 103 passed / 1
      pre-existing failure.
- [x] 8.10 **Beta launcher icon** (2026-07-30). Two installs sharing an icon are
      as confusing as two sharing a name. `android/res-beta/` overrides the
      launcher mipmaps for the beta variant only (`res.srcDirs += ['res-beta']`
      — *added to* the main dirs, since a variant source set already takes
      priority for same-named resources, so `values/`/`xml/` still come from
      `res/`). Generated by `scripts/generate_beta_app_icons.sh` from
      `assets/icons/appicons/simsapa-beta_w512.png`; the script derives its
      geometry from the shipped release icons rather than inventing it — both
      sources trim to the same 440x440 box at +36+36, so the release transform
      applies unchanged and the art lands inside the adaptive-icon safe zone.
      Verified by extracting `res/*.png` from both finished APKs and
      pixel-comparing: the beta package's 432 px layers are a 0-pixel match for
      the beta foreground and differ from the release art by exactly the badge;
      the plain release package is the inverse (0 vs release, badge-sized diff
      vs beta). Label and icon both confirmed via `aapt2 dump badging`.
- [x] 8.9 **Review pass (2026-07-30), two gaps found and closed:**
      1. **The release-notes pane defeated the button gating.** The app-update
         view renders the server-supplied GitHub release *description* as
         RichText with a live `onLinkActivated` handler, so a link in that
         description would still take a Play user to the download page even with
         the "Open Link" button hidden. All three link handlers in the dialog now
         go through `open_release_link()`, inert on a Play install. Trade-off
         recorded: on Play installs release-notes links are not clickable.
      2. **The identity marker ignored the signing state.** `make
         android-apk-debug` (unsigned) straight after `make android-beta-debug`
         (release-signed) yields the same package id, so ninja stayed up to date
         and the script would report the still-release-signed APK as unsigned —
         invisible, since the re-sign is applied in place. The marker is now
         `<BuildType>-beta<0|1>-sign<0|1>`.

      Also checked and found sound: the `obsolete` and `db` views offer no app
      download (their "Download Now" fetches **data**, outside the policy); no
      other QML file offers an app download link; `visit_url` is confirmed to be
      a GitHub release-tag URL (`update_checker.rs:647`); the Gradle beta
      conditionals fire in both directions; the `dist/` version parse yields
      `1.0.0-alpha.3`. One footgun noted in the Makefile rather than changed:
      `make macos-clean` does `rm -rf ./dist` and will delete the beta APK.
- [ ] 8.8 **Not runtime-verified: the Play branch of the update dialog.** It only
      appears when an update is actually available, so the QML branch has not
      been exercised on device. The JNI follows the proven
      `get_status_bar_height()` pattern and `dumpsys` independently confirms both
      installer values (`com.android.vending` for the Play copy, `null` for the
      sideloaded beta), but the branch itself is untested. Check it at the next
      release that offers an update.

---

### 9.0 — visual-verification fixes from the 6.2 per-window sweep

**Added 2026-07-30**, from screenshots of the beta build on the Android 16 phone
while working through 6.2.

- [x] 9.1 **Rich-text link colour was unreadable in light mode** — Dictionaries
      window, AI Models' "API Keys" / "Pricing", Search Help, and in fact every
      `Text { textFormat: Text.RichText }` with an `<a href>` in the app.

      **Two wrong diagnoses were tried first; both are recorded so they are not
      tried again.** (a) "`palette.link` comes from the platform under a
      *system* theme" — there **is no** system theme: `ThemeName` is
      `Light | Dark` only, and `theme_colors_light.json` sets `link` to
      `#0000FF`. (b) "set `Text.linkColor` instead" — that had no effect at all,
      which is what forced reading Qt's source.

      **Actual cause**, from Qt 6.9.3:
      - `QTextHtmlParser` gives every `<a href>` an injected CSS declaration
        `color: palette(link)` (`qtexthtmlparser.cpp:2062-2065`), so the anchor
        ends up with an **explicit** `charFormat` foreground.
      - That declaration is resolved by `QCss::ValueExtractor`, constructed at
        `qtexthtmlparser.cpp:1182` **without a palette argument**, so it falls
        back to a default-constructed `QPalette` — i.e. **`QGuiApplication`'s**.
        `ThemeHelper.apply()` only ever assigns each *window's* QML palette, so
        the theme's `link` value was never consulted.
      - `Text.linkColor` is applied by `QQuickTextNodeEngine` only when the char
        format has **no** foreground of its own
        (`qquicktextnodeengine.cpp:1098-1101`) — and the injected declaration
        gives it one. So `linkColor` is dead for `<a href>` in `RichText`.

      **Fix:** `set_app_palette_link_colors()` in `cpp/system_palette.cpp` sets
      the `Link` / `LinkVisited` roles (all three colour groups) on the
      application palette. One place, every window, including HTML that the Rust
      side generates (the AI-provider descriptions). Only those two roles are
      written, to keep the blast radius small.

      **The timing is load-bearing, and that took a second pass to get right.**
      Wiring it only into `ThemeHelper.apply()` fixed `DictionariesWindow` (a
      separate `ApplicationWindow` created later by
      `WindowManager::create_dictionaries_window()`) but **not** `SearchHelpWindow`
      or `DhammaTextSourcesDialog` — those are **inline children of
      `SuttaSearchWindow`** (`SuttaSearchWindow.qml:2282`, `:2287`), so their HTML
      is parsed during the engine load, and the anchor colour is baked into the
      char format at that moment. A palette fixed moments later does not
      retroactively recolour them. So the colours are now applied in `gui.cpp`
      immediately after `QApplication` is constructed and **before** the engine
      loads, from a standalone settings read (`theme_link_colors_c()` in
      `backend/src/lib.rs`, reusing the `render_settings()` cache that
      `render_loop_basic_c()` already uses). The `ThemeHelper.apply()` call is
      kept for windows created after a runtime theme change.

      **Known limitation:** changing the theme at runtime does not recolour links
      in already-parsed rich text — those windows need an app restart. Same root
      cause (the colour is baked at parse time), not worth extra machinery.
- [x] 9.2 **"Links:" was black in dark mode** (AI Models). Unrelated to 9.1: the
      provider `description` `Text` set no `color`, and `Text` defaults to black
      rather than `palette.text` — invisible against the dark window. Set
      `color: palette.text`.
- [x] 9.3 **Bottom button area too tall on mobile** (Sutta Languages, Library,
      Chanting Practice, About; Settings was correct because it never had the
      extra). Every one of these was `Layout.bottomMargin: root.is_mobile ? 60 :
      N` (or a bare `60`) with the comment "Extra space on mobile to avoid the
      bottom bar covering the button" — the **bottom** twin of the doubled top
      gap fixed in 1.0/2.0. All 15 files with the idiom root an
      `ApplicationWindow`, which Qt already pads by the bottom safe-area inset
      (`qquickapplicationwindow.cpp:802-805`), so the 60 was additive. Dropped
      to the desktop value throughout, and the stale comments removed. Files:
      `AboutDialog`, `ChantingPracticeWindow`, `DatabaseValidationDialog`,
      `DhammaTextSourcesDialog`, `DictionariesWindow`, `DownloadAppdataWindow`,
      `DownloadProgressFrame` (a `Frame`, but only ever inside such a window),
      `KeybindingCaptureDialog`, `LibraryWindow`, `ReferenceSearchInfoDialog`,
      `SearchHelpWindow`, `SuttaLanguagesWindow`, `TopicIndexInfoDialog`,
      `TopicIndexWindow`, `UpdateNotificationDialog`.
- [x] 9.4 **System Prompts window did not collapse to one column in portrait** —
      its `SplitView` was hardcoded `orientation: Qt.Horizontal`, so a portrait
      phone showed a squeezed list beside a ~10-character-wide editor. Adopted
      `ModelsDialog`'s rule verbatim (`is_wide` / `is_tall`, orientation
      switched on `is_wide`, `SplitView.preferredHeight`/`minimumHeight` on the
      list pane, `SplitView.fillHeight` on the editor pane) so the list moves
      **above** the editor when narrow.
- [x] 9.6 **Four windows bypassed the safe area entirely** — found when 9.3's
      smaller bottom margin exposed it: the Database Validation window's lowest
      buttons ended up *under* the navigation bar. `AnkiExportDialog`,
      `DatabaseValidationDialog`, `SystemPromptsDialog` and `ModelsDialog` sized
      their root content item from the **window**:

      ```qml
      Item {
          x: 10
          y: 10 + root.extra_top_margin
          implicitWidth: root.width - 20
          implicitHeight: root.height - 20 - root.extra_top_margin
      ```

      A direct child of an `ApplicationWindow` is reparented to `contentItem`,
      which Qt has **already** inset by the safe-area margins — its height is
      `root.height - topPadding - bottomPadding`. Sizing from `root.height`
      therefore overflows the bottom by `topPadding + bottomPadding - 10`
      (≈ 70 px on the test phone), regardless of `extra_top_margin`. The old
      `is_mobile ? 60` bottom margin had been masking almost exactly that
      overflow, which is why removing it made the defect visible rather than
      causing it. All four now use `anchors.fill: parent` +
      `anchors.margins: 10` + `anchors.topMargin: 10 + root.extra_top_margin`.

      Every other window was checked and is sound: they root their content in a
      `Frame`/`StackLayout` with `anchors.fill: parent`, so they were always
      inside the padding. **Rule for the doc:** a window's root content item
      must be anchored to its parent, never sized from `root.width`/
      `root.height` — that is the third way to defeat Qt's safe-area handling,
      alongside assigning `topPadding` on an `ApplicationWindow` root and the
      `Popup` family getting no padding at all.
- [x] 9.5 Re-check all four on device against a new beta build.
