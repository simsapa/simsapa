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

- [ ] 2.0 Update the QML layer: property/API rename, the reworked Settings
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
  - [ ] 2.12 **Check the margins visually before moving on** — the whole of 2.0
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

- [ ] 3.0 Move the Android build to targetSdk 36 and stop building the debug
      variant during release builds (PRD 1, 2, 3, 20, 35, 36, 37, 38, 39)
  - [ ] 3.1 Set `targetSdkVersion 36` in `android/build.gradle` `defaultConfig`,
        with a comment recording the three enforced API 36 behaviours
        (edge-to-edge with no opt-out, predictive back by default, large-screen
        orientation attributes ignored).
  - [ ] 3.2 Add `android.suppressUnsupportedCompileSdk=36` to
        `android/gradle.properties` with a comment pointing at the AGP analysis in
        `AGENTS.md`. **It is not there today** — the file ends at
        `android.useAndroidX=true`, so every build currently prints the warning
        that `AGENTS.md:509` claims is already suppressed. Reconcile that
        sentence as part of 7.6. (Confirmed safe: androiddeployqt *appends* its
        generated keys to the copied file rather than overwriting it — the
        generated `android-build/gradle.properties` still carries our
        `org.gradle.parallel` and `android.useAndroidX` lines.)
  - [ ] 3.3 Add the `androidComponents { beforeVariants(selector().withBuildType(
        "debug")) { it.enable = !project.hasProperty("simsapaReleaseOnly") } }`
        block to `android/build.gradle`, with a comment explaining that
        androiddeployqt appends the bare `bundle` task, which otherwise drags in
        the whole debug variant.
  - [ ] 3.4 In `build-android.sh`, `export
        ORG_GRADLE_PROJECT_simsapaReleaseOnly=true` **only** for release builds.
        Gradle maps `ORG_GRADLE_PROJECT_<name>` env vars to project properties,
        so `project.hasProperty("simsapaReleaseOnly")` works with no
        androiddeployqt plumbing. **For `--debug` the variable must be left
        unset — never set to `false`**, since `hasProperty` is true for any
        value including `false` and the empty string.
  - [ ] 3.5 Build a signed AAB and confirm the log contains **no** `:*Debug*`
        packaging tasks (previously 43), the release AAB is still produced and
        signed, and `build/outputs/bundle/debug/` is not created.
  - [ ] 3.6 Confirm `make android-apk-debug` still succeeds with the debug variant
        enabled.
  - [ ] 3.7 Do **not** touch AGP, the Gradle wrapper, the NDK or the JDK pin;
        confirm the build still reports AGP 8.6.0 / Gradle 8.12 / JDK 21 (PRD 38).

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

- [ ] 4.0 Make the version values flow from `android/version.txt` and
      `bridges/Cargo.toml` through the environment into CMake (PRD 28–34)
  - [ ] 4.1 Create `android/version.txt` with the comment header and the single
        value `3` (Play currently has 2).
  - [ ] 4.2 In `build-android.sh`, add a parser that reads the first non-blank,
        non-`#` line of `android/version.txt` and validates it as a positive
        integer; **do not** `source` the file.
  - [ ] 4.3 In `build-android.sh`, parse the package `version` from
        `bridges/Cargo.toml` (the `[package]` version near the top — not a
        dependency's) with a targeted regex.
  - [ ] 4.4 Export both as `ANDROID_VERSION_CODE` / `ANDROID_VERSION_NAME`, letting
        a **non-empty** inherited environment value win (`[ -n "${VAR:-}" ]`, not
        an is-set test — see the spec above), and record which source was used.
  - [ ] 4.5 Fail **before** the CMake configure with an actionable message if
        either file is missing or unparseable, or the versionCode is not a positive
        integer (replacing the current warn-only check at build-android.sh:306).
  - [ ] 4.6 Echo `versionCode` / `versionName` and their sources at the start and
        again at the end of the build next to the artifact path (replacing the
        current lines 298–299).
  - [ ] 4.7 Drop the `-DANDROID_VERSION_*` arguments (build-android.sh:315–316) now
        that CMake reads the environment.
  - [ ] 4.8 In `CMakeLists.txt`, replace the two `CACHE STRING` version variables
        (140–141) with plain `$ENV{...}` reads, and guard the
        `QT_ANDROID_VERSION_*` properties (353–357) so they are only set when both
        are **non-empty** (`if(NOT "${X}" STREQUAL "")` — make exports undefined
        variables as empty); emit a `message(STATUS)` otherwise. Also update the
        stale example in the comment at 137 (`make android-aab
        ANDROID_VERSION_CODE=3 …`).
  - [ ] 4.9 Update the `Makefile` Android targets and comments (90–134) to drop
        `ANDROID_VERSION_CODE=<n> ANDROID_VERSION_NAME=<v>` from the documented
        command line; keep the `export` lines so an explicit override still works.
  - [ ] 4.10 Verify with `aapt2 dump badging`: a build with no arguments carries
        versionCode 3 and the Cargo version name; then edit `android/version.txt`
        to 4, rebuild **in the same build directory via `build-android.sh`**, and
        confirm the manifest changes (the old `CACHE` behaviour would not). The
        script's unconditional `cmake -S . -B` re-configure is what makes this
        work; a bare `cmake --build` is expected to keep the old value.
  - [ ] 4.11 Verify a plain `cmake` configure without the env vars succeeds and
        logs the STATUS message instead of failing.

---

### 5.0 — specs

The two deferred investigations are **decisions with evidence**, not code changes.
Record the numbers and the conclusion in the docs (task 7.0) whichever way they go.

**Depends on:** 3.0 and 4.0 (needs a real signed multi-ABI bundle).

- [ ] 5.0 Build and statically verify the signed multi-ABI bundle, and close the
      two deferred packaging investigations (PRD 4, 5, 41, 42, 43, 44)
  - [ ] 5.1 Run a full clean `make android-rebuild` (arm64-v8a; x86_64;
        armeabi-v7a) and confirm it completes with Qt 6.9.3 / NDK 27.3 / JDK 21 /
        AGP 8.6.0.
  - [ ] 5.2 Confirm the existing artifact checks in `build-android.sh` still pass:
        no cross-ABI staged libraries, correct ELF machine type per ABI,
        `zipalign -c -P 16`.
  - [ ] 5.3 Investigate the `QML import could not be resolved:
        com.profoundlabs.simsapa` warning: compare `assets/android_rcc_bundle/qml/`
        in the built package against the QML the app imports, and confirm the app's
        own module is compiled into the binary via `cxx_qt_import_qml_module`
        rather than shipped as a plugin directory.
  - [ ] 5.4 Classify the remaining import warnings (`QtWebEngine`,
        `QtWayland.Compositor`, `QtQuick.Controls.{Windows,macOS,iOS}`,
        `QtQuick3D.MaterialEditor`) as harmless-by-construction, with the reason
        for each.
  - [ ] 5.5 Measure `useLegacyPackaging` **both ways**: AAB/APK size, on-device
        install footprint, `zipalign -c -P 16`, and `readelf -lW` `p_align` of the
        app `.so` and a Qt lib. Record the numbers.
  - [ ] 5.6 Decide on `useLegacyPackaging` from those measurements — keep `true`
        unless they favour changing it — and note the decision for task 7.0.
  - [ ] 5.7 Verify the bundle's device catalogue expectations with
        `aapt2 dump badging`: `targetSdkVersion 36`, `minSdkVersion 27`, no
        unexpected `uses-permission`, every `uses-feature` `required="false"`.
        This is the **only** trustworthy check of the target level — the
        generated `gradle.properties` will still read `qtTargetSdkVersion=35`
        (androiddeployqt writes it; `build.gradle` never reads it).
  - [ ] 5.8 Record the `qtMinSdkVersion=28` finding: Qt **6.9.3** already
        declares an Android floor of 28 in the generated `gradle.properties`, and
        `build.gradle` overrides it down to 27 — so the app has shipped one level
        below Qt's declared minimum since the 6.9.3 move. Qt 6.10 does not
        introduce that constraint, it removes our ability to keep overriding it.
        Feeds 7.4 and PRD open question 4; no code change here.

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
  - [ ] 6.1 Confirm Qt still reports non-zero safe-area margins at targetSdk 36:
        the top gap must be **one** inset, not zero and not doubled (PRD 17). This
        underpins the whole design — check it first.
  - [ ] 6.2 Portrait: search bar, toolbar and every bottom control fully visible
        and tappable, in the main window and in each secondary window (Settings,
        Library, Dictionaries, Sutta Languages, Topic Index, Reference Search,
        Chanting Practice, About) — each is its own `ApplicationWindow`.
  - [ ] 6.3 Landscape: rotate in each main window; check the cutout side and
        rounded corners; confirm margins update without a restart, including while
        a dialog is open.
  - [ ] 6.4 Soft keyboard: focus a search field and a multi-line field; the focused
        input stays visible and the keyboard raises on the first tap
        (`docs/android-soft-keyboard.md`).
  - [ ] 6.5 Settings → Extra Top Margin: with `0`, the gap is a single inset; raise
        it and confirm the space appears immediately and survives a restart;
        confirm the system-inset readout updates on rotation.
  - [ ] 6.6 Upgrade path: install over an existing install **with** a custom margin
        (layout must be unchanged) and over one **without** (the doubled gap must be
        gone).
  - [ ] 6.7 Audit the cases Qt does not pad (PRD 15), working from the list task
        2.11 produced: inline `Dialog`/`Popup` items (tall or top-anchored ones
        especially), any mobile-visible `header`/`footer`/`menuBar`, and
        `Flickable`/`ListView` content scrolling under an edge. **Check the
        drawer menu explicitly** — open it in portrait and landscape and confirm
        the "Menu" label clears the status bar / cutout with the 2.10 fix in
        place.
  - [ ] 6.8 Sutta reader (WebView): open a sutta, scroll to top and bottom, use the
        find bar, switch display layouts; confirm no HTML content sits under a
        system bar (PRD 16).
  - [ ] 6.9 Back navigation: system back gesture and hardware back from a dialog,
        a secondary window and the main window. **Record the behaviour** — this
        decides 6.10.
  - [ ] 6.10 If back regressed, add `android:enableOnBackInvokedCallback="false"`
        to the `<activity>` in `android/AndroidManifest.xml` with a comment that
        the opt-out is temporary; if it did not, change nothing. Either way the
        result feeds task 7.0.
  - [ ] 6.11 Large screens: if a tablet or Chromebook is available, resize and
        rotate; confirm nothing depends on a fixed orientation. Do **not** add
        `PROPERTY_COMPAT_ALLOW_RESTRICTED_RESIZABILITY` unless a concrete problem
        appears (PRD 25).
  - [ ] 6.12 Exercise the remaining ABI-sensitive paths: first-run asset download,
        fulltext search (tantivy), dictionary lookup, chanting record/playback,
        file save via SAF.
  - [ ] 6.13 If a 32-bit ARM device is available, repeat 6.1, 6.8 and 6.12 on it.
  - [ ] 6.14 Decide whether any edge other than the top needs its own knob
        (PRD 14) — default answer is **no**; record the finding either way.

---

### 7.0 — specs

Documentation is a deliverable here, not an afterthought: several of these findings
(the deprecated Qt Java calls, the AGP pin, the doubled-gap diagnosis) exist
specifically so the next Play report or version bump does not restart the
investigation.

**Depends on:** 5.0 and 6.0 (their results are what gets written down).

- [ ] 7.0 Documentation, recorded decisions, and cleanup (PRD 23, 26, 27, 40,
      45, 46, 47, 48, and the closed decisions 49–52)
  - [ ] 7.1 Write `docs/android-edge-to-edge-and-safe-areas.md`: Qt's
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
  - [ ] 7.2 Record in that doc that the three Play-reported deprecated APIs
        (`Window.getStatusBarColor`, `setStatusBarColor`, `setNavigationBarColor`)
        live in Qt's own Java — `QtActivityDelegateBase.java:108`,
        `QtDisplayManager.java:191/192/200/204` — are no-ops at API 36, and can only
        be removed by a Qt upgrade (PRD 26, 27).
  - [ ] 7.3 Record the predictive-back result and decision from 6.9/6.10, and the
        large-screen finding from 6.11 (PRD 23, 24).
  - [ ] 7.4 Write `docs/android-qt-upgrade-considerations.md` from PRD §7.5: the
        reasons to upgrade, and the pitfalls — Qt 6.10.1's libtiff SONAME and
        WebEngine-on-FUSE AppImage crash (`docs/qt-6.10.1-appimage-issues.md`),
        Qt 6.10's minSdk 28 floor vs our 27, the NDK constraint, cxx-qt exposure,
        the multi-ABI mechanism, and that desktop/Windows/macOS ride the same Qt.
        Include the AGP/Gradle-wrapper/JDK coupling as part of that upgrade
        (PRD 40). State the minSdk position correctly per 5.8: Qt 6.9.3 already
        declares `qtMinSdkVersion=28` and we override to 27, so the upgrade
        removes an override we are already relying on rather than imposing a new
        floor.
  - [ ] 7.5 Update `docs/android-multi-abi-and-chromeos.md` for targetSdk 36, the
        release-only debug-variant switch (and that it rides on
        `ORG_GRADLE_PROJECT_simsapaReleaseOnly`, not a `-P` argument), and the
        version workflow.
  - [ ] 7.6 Update `AGENTS.md` (`CLAUDE.md` is a symlink): the new targetSdk, the
        release procedure (edit `android/version.txt`, then `make android-aab` —
        no version arguments), and links to the two new docs. **Correct line 509**,
        which states that `android.suppressUnsupportedCompileSdk=36` is already in
        `android/gradle.properties` — it was not until task 3.2. The rest of the
        AGP section stays as is.
  - [ ] 7.7 Record the `useLegacyPackaging` measurements and decision from 5.5/5.6,
        and the QML-import-warning conclusions from 5.3/5.4.
  - [ ] 7.8 Record the closed decisions (PRD 49–52) where they belong: keep
        `armeabi-v7a` (we have users on 32-bit ARM phones), no 32-bit `x86`, do not
        remove `package=` from the manifest, bundle size needs no action.
  - [ ] 7.9 Confirm `tasks/android-packaging-follow-ups.md` is deleted (it is —
        never committed) and nothing references it; update `PROJECT_MAP.md` if the
        new docs belong in its index.
  - [ ] 7.10 Final check: `make test` passes, and the release procedure works
        end-to-end from a clean tree — edit `android/version.txt`, `make
        android-aab`, verify with `aapt2 dump badging`.
