# Tasks — minSdkVersion 28 and the AOT QML cache

PRD: [2026-08-16-193200-prd---minsdk-28-and-aot-qml-cache.md](./2026-08-16-193200-prd---minsdk-28-and-aot-qml-cache.md)

Order of work is fixed by PRD §6.1: **Part A → Part C → Part B**. Each parent
task ends with the tree building and the relevant tests passing; each part is a
separate commit. **No Qt, AGP, Gradle-wrapper, JDK or NDK version moves anywhere
in this work.**

## Relevant Files

### Part A — minSdkVersion

- `android/build.gradle` — `defaultConfig` at line 153 holds `minSdkVersion 27`;
  **the only source of truth**. Line 52 carries the NDK comment that names 27.
- `CMakeLists.txt` — line 516's NDK-r28 comment names minSdk 27. Confirm it sets
  no `QT_ANDROID_TARGET_SDK_VERSION` / min-sdk property.
- `android/AndroidManifest.xml` — confirm there is **no** `<uses-sdk>` element.
- `scripts/android-api-scan.sh` — the measurement tool; `--floor 28` is the
  confirmation run (FR-5a). Also has `--markdown` for the doc's §2 tables.
- `docs/android-api-levels-and-feature-dependencies.md` — the evidence base;
  §1 declared-levels table, §4 last row, §8 "consequences", §9 standing rule.
- `docs/android-qt-upgrade-considerations.md` — §1 version table, **§2.2** (must
  be rewritten from "deferred to the upgrade" to "done, and why decoupled").
- `docs/android-multi-abi-and-chromeos.md` — §2 permissions rationale (`:71`,
  `:94`, `:174`, `:530`, `:801`, `:803`), the `qtMinSdkVersion=28` override note,
  the `aapt2` sample output.
- `docs/android-beta-distribution-and-play-policy.md` — `:87` (`mipmap-anydpi-v26`)
  and `:262` (the API-27 direct-call note).
- `docs/pure-rust-audio-backend.md` — `:41-42` already say the r28 exclusion does
  not lapse at 28; confirm no edit needed.
- `docs/relocated-storage-recovery.md` — `:174` "above the minSdk floor" claim.
- `docs/android-soft-keyboard.md` — `:254` names minSdk 28 as upgrade-coupled.
- `AGENTS.md` (`CLAUDE.md` is a symlink) — `:242`, `:386`, `:914`, `:1588`.
- `PROJECT_MAP.md` — `:52`, `:71`, `:80`.
- `build-android.sh` — `:134-138`, `:345-351`: **no edit expected** (already
  says "holds at minSdk 28 as well as 27"); verify only.
- `android/version.txt` — not bumped by this PRD; FR-9 is a reminder only.

### Part B — AOT QML cache

- `bridges/build.rs` — the 96-entry `"../assets/qml/…"` list, the
  `qrc_resources` + alias-derivation block with its explanatory comment, and
  `CxxQtBuilder::new_qml_module(QmlModule::new(URI))`.
- `assets/qml/` → `bridges/assets/qml/` — 108 root `.qml` files (13 of them
  `tst_*.qml`), plus `com/profoundlabs/simsapa/` (bridge type stubs + `qmldir`),
  `data/` (`BojjhangaData.qml` + its own `qmldir`), and `icons/`.
- **`assets/qml/icons` is a tracked SYMLINK to `../icons`** (git mode `120000`),
  not a directory. `git mv` moves the link verbatim and its relative target then
  resolves to `bridges/assets/icons`, which does not exist — see 6.2.
- `assets/icons.qrc` — **stays put**; its `<file>` entries resolve against
  `assets/` (`icons/32x32/…`), its prefix is the shared QML resource dir.
  Verified: 117 files in `assets/icons/32x32/`, identical md5s through the
  symlink, and `CMakeLists.txt:398` `qt_add_resources(icons "assets/icons.qrc")`
  is the only consumer.
- `appimage.conf:8` — `qml_sources_paths = assets/qml`. **A consumer FR-12 does
  not list.**
- `bridges/src/api.rs:256` — `include_dir!("$CARGO_MANIFEST_DIR/../assets/")`
  embeds the **whole** assets tree, `qml/` included (~2.4 MB), and serves it at
  `/assets/<path..>`. The move removes that from the binary — a size delta
  unrelated to qmlcachegen. See 5.4.
- `Makefile` — `qml-lint` (`:102`), `qml-test` (`:105`), the commented
  single-test line (`:90`), and the two `tokei` exclude lists (`:70`, `:73`).
- `build-macos.sh:233` — `macdeployqt … -qmldir=./assets/qml`.
- `build-appimage.sh:179` — `QML_SOURCES_PATHS="$QT6_PATH/qml:./assets/qml"`.
- `cpp/sutta_search_window.cpp:18-20` — the `STARTUP-TRACE: engine.load()`
  instrumentation used for the baseline (FR-10) and re-measurement (FR-17).
- `cpp/` — the 12 `qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/…` literals
  that FR-13's alias-neutrality protects. **Must not change.**
- `docs/cxx-qt-fork.md` — §5 (the `..` trap) and its "Side finding: the AOT QML
  cache has never been used in this project"; where the measurement, the
  decision and the upstream issue URLs are recorded.
- `AGENTS.md` — "New QML components" rule, the `console`-API exception for the
  stub directory, the `tst_dialog_loop_harness` instructions.
- `scripts/tst_dialog_loop_harness.qml.keep`, `scripts/qml-watch.sh`,
  `scripts/qt-env.sh` — check each for a hardcoded `assets/qml` path.

### Notes

- **`aapt2 dump badging` is the only trustworthy minSdk/targetSdk check.** The
  generated `android-build/gradle.properties` carries `qtMinSdkVersion` and a
  stale `qtTargetSdkVersion=35` that `build.gradle` never reads.
- **A green build proves nothing about the AOT cache.** Unmatched units fall
  back silently to parsing QML source — today's behaviour. Task 8.0 exists
  because of this.
- **A stale `build/` directory caches `CMAKE_PREFIX_PATH`.** Reconfigure clean
  before every measurement run.
- Timing-assertion drift in `cargo test` is known and is not a regression from
  this work.
- Never hand-delete `android-build/`; use `make android-clean`.
- **FR-13's alias-neutrality is verified from the pinned cxx-qt source**, not
  assumed. `qt-build-utils/src/lib.rs`'s `resource_add_path` sets the rcc alias
  to `path.display().to_string()` — the literal string from the `qml_files`
  list — under prefix `/qt/qml/{uri dirs}`. So `"assets/qml/Foo.qml"` produces
  exactly today's `:/qt/qml/com/profoundlabs/simsapa/assets/qml/Foo.qml`. The
  same function also shows why `"../assets/qml/Foo.qml"` is broken today.
- **qmlcachegen runs automatically** for every `qml_files` entry when Qt ≥ 6
  (same file, "Run qmlcachegen" block), once per file plus one `compile_loader`
  pass. No flag or CMake change is needed to turn AOT on — the `qml_files` move
  *is* the switch.
- **Side benefit of the move:** cxx-qt-build emits `rerun-if-changed` with the
  **raw** path and cargo resolves relative rerun paths against the package root.
  Today's `../assets/qml/…` entries are therefore resolved wrongly; once the
  files live under `bridges/`, they resolve correctly and incremental rebuilds on
  a QML edit become reliable. Worth confirming during 8.x, and worth recording in
  9.5 as a non-timing argument for keeping the move.
- **No `pragma Singleton` among the 96 registered files** (only the qmllint stub
  `com/profoundlabs/simsapa/SuttaBridge.qml`, which is not registered). The
  generated `qmldir` becoming authoritative therefore cannot mis-declare a
  singleton as a plain type — a real risk, checked and clear.
- **`assets/qml/data/` is not load-bearing.** It is absent from
  `bridges/build.rs`'s list, so it is not in the resource at all; its only
  reference is a commented-out preview line at `FulltextResults.qml:64`. PRD
  §6.2 overstates it — the implicit same-directory resolution that matters is
  the **root** directory (`Logger.qml` and friends).
- **`scripts/android-api-scan.sh` scans one ABI** — `abi="arm64-v8a"` at `:45`,
  with an `--abi` flag — and `find_apk` picks the *newest* APK anywhere under
  `build/`. Pass `--apk` explicitly and run all three ABIs (see 1.6).

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, check it off in this file by changing
`- [ ]` to `- [x]`. Update the file after each **sub-task**, not only after a
parent task. Where a task says "record", write the number or the output into
this file, under the sub-task, so the record lives with the work.

---

## Tasks

### 1.0 Part A — raise `minSdkVersion` to 28 and verify it on the artifact

> **Spec.** One-line change in `android/build.gradle`'s `defaultConfig`
> (FR-1). The safety analysis is already done and written up in
> `docs/android-api-levels-and-feature-dependencies.md`: `libQt6Core_arm64-v8a.so`
> carries a **GLOBAL** undefined `getentropy@LIBC_P`, first provided by bionic at
> API 28, so the app already cannot `dlopen` below 28. This is a correctness fix,
> not a distribution trade-off — do not re-derive the analysis.
> **Depends on:** nothing. **Blocks:** 2.0, 3.0.

- [x] 1.1 Confirm `android/build.gradle:153` is the only source of the minimum
  level. **Pre-verified during the task review (2026-08-26):**
  `android/AndroidManifest.xml` has no `<uses-sdk>` and `CMakeLists.txt` has no
  `*_SDK_VERSION` variable of any kind. Re-run both greps to confirm nothing has
  moved, then check this off.

  **Re-run 2026-08-26.** `grep -n "uses-sdk\|minSdk\|SdkVersion"
  android/AndroidManifest.xml` → no match (exit 1).
  `grep -n "SDK_VERSION\|minSdk\|MIN_SDK" CMakeLists.txt` → one hit, `:516`,
  which is prose inside the NDK-r28 comment, not a variable.
  `android/build.gradle` → `:153 minSdkVersion 27`, `:171 targetSdkVersion 36`,
  `:47 compileSdkVersion androidCompileSdkVersion` (androiddeployqt-supplied).
  Confirmed: `:153` is the only source of truth.
- [x] 1.2 Change `minSdkVersion 27` → `minSdkVersion 28` in
  `android/build.gradle`'s `defaultConfig`, with a short comment stating that
  Qt 6.9.3 declares `qtMinSdkVersion=28` and that `libQt6Core` hard-requires
  API 28 (`getentropy`), naming
  `docs/android-api-levels-and-feature-dependencies.md` §2.

  Done. The comment records the `getentropy` GLOBAL-undef finding, that bionic
  first provides it at API 28, that Qt 6.9.3 declares `qtMinSdkVersion=28`, and
  that the old value was an override that never worked.
- [x] 1.3 Verify `build-android.sh`'s two NDK sites (`:134-138`, `:345-351`)
  already read correctly at minSdk 28 and need **no** edit (FR-3). Do **not**
  re-open the r28 question — its exclusion is about bionic API **30**.

  Confirmed, **no edit made**. Both sites already phrase the pin as "at this
  project's minSdk" and both carry the sentence *"The exclusion holds at
  minSdk 28 as well as 27."* The r28 exclusion is about `pthread_cond_clockwait`
  being declared by bionic only at API **30+**, which 28 is still below.
- [x] 1.4 Build a **multi-ABI** beta APK: `make android-beta-debug` (all three
  ABIs — `make android-beta-debug-arm64` is faster but leaves 1.6 unable to
  cover x86_64 and armeabi-v7a). Expect no build-system complaint about the
  floor.

  **Built 2026-08-26, exit 0**, no build-system complaint about the floor.
  Artifact:
  `build/android-multiabi/android-build/build/outputs/apk/debug/android-build-debug.apk`
  (versionCode 7 from `android/version.txt`, versionName `1.0.0-alpha.6` from
  `bridges/Cargo.toml`). The script's own artifact checks all passed: three ABIs
  present (`arm64-v8a`, `armeabi-v7a`, `x86_64`), *"Cross-ABI contamination
  check: OK — every library matches its ABI directory"*, and *"ChromeOS check:
  no required hardware features. OK."*
- [x] 1.5 Run `aapt2 dump badging <apk> | grep -i sdkversion` and record the
  output here. It must show `minSdkVersion:'28'` **and** `targetSdkVersion:'36'`
  (FR-2). Do not verify by reading `android-build/gradle.properties`.

  `~/Android/Sdk/build-tools/36.0.0/aapt2 dump badging <apk> | grep -i sdkversion`:

  ```
  package: name='io.github.simsapa.app.beta' versionCode='7' versionName='1.0.0-alpha.6-beta-debug' platformBuildVersionName='16' platformBuildVersionCode='36' compileSdkVersion='36' compileSdkVersionCodename='16'
  minSdkVersion:'28'
  targetSdkVersion:'36'
  ```

  Both required values confirmed **on the artifact** (FR-2).
- [x] 1.6 Run the confirming scan **once per ABI**, passing the APK explicitly
  (`find_apk` otherwise takes the newest APK anywhere under `build/`, which may
  be an unrelated arm64-only build):

  ```sh
  ./scripts/android-api-scan.sh --apk <apk> --floor 28 --abi arm64-v8a
  ./scripts/android-api-scan.sh --apk <apk> --floor 28 --abi x86_64
  ./scripts/android-api-scan.sh --apk <apk> --floor 28 --abi armeabi-v7a
  ```

  Paste all three verdict lines here. Each must report that **no hard (GLOBAL)
  undefined symbol requires more than API 28** (FR-5a). The script defaults to
  arm64 only, and `minSdkVersion` binds every ABI — the `getentropy` finding was
  measured on `libQt6Core_arm64-v8a.so` and the other two slices have never been
  scanned at all. If a non-arm64 slice reports a *higher* floor, stop and raise
  it: that is a new finding, not a formality.

  **Run 2026-08-26** against the 1.4 APK, NDK 27.3.13750724, 98 libraries
  scanned per ABI. All three verdict lines:

  ```
  arm64-v8a     OK  no hard (GLOBAL) undefined symbol requires more than API 28
  x86_64        OK  no hard (GLOBAL) undefined symbol requires more than API 28
  armeabi-v7a   OK  no hard (GLOBAL) undefined symbol requires more than API 28
  ```

  **28 is a sufficient floor for every ABI, not merely a higher one** (FR-5a).
  No slice reports a higher floor, so there is no new finding to raise — the two
  previously unscanned slices are now covered.

  Everything above 28 is **WEAK** and therefore not a floor (resolves to null,
  caller falls back):

  | ABI | WEAK refs above 28 |
  |---|---|
  | arm64-v8a | `copy_file_range` (34), `memfd_create` (30), 4× `ZSTD_trace_*` (never in bionic) |
  | x86_64 | `copy_file_range` (34), 4× `ZSTD_trace_*` (never in bionic) |
  | armeabi-v7a | `copy_file_range` (34) |

  Directly linked system libraries are identical across all three ABIs:
  `libaaudio.so`, `libandroid.so`, `libc.so`, `libdl.so`, `libEGL.so`,
  `libGLESv2.so`, `liblog.so`, `libm.so`, `libnativewindow.so` — `libaaudio.so`
  being the cpal/AAudio dependency at API 26, well under the floor.
- [x] 1.7 Commit Part A's code change on its own (docs follow in 2.0, or fold
  them in — but nothing from Part B or C in this commit).

  Commit `5fe14a7` *"Raise Android minSdkVersion from 27 to 28"* —
  `android/build.gradle` plus this task file. Nothing from Part B or C.

### 2.0 Part A — correct every "minSdk 27" claim, and record the scan, permission re-check and Play share

> **Spec.** FR-4, FR-5, FR-5b, FR-7. The rule is that after this task **no file
> in the tree claims the app's minSdk is 27** (Success Metric 2). Line numbers in
> the PRD's list will have moved — re-derive the site list by grep rather than
> trusting it, and treat the PRD list as a completeness check.
> **Depends on:** 1.0.

- [x] 2.1 Re-run the `WRITE_EXTERNAL_STORAGE` audit (FR-4): grep
  `getExternalStorage`, `EXTERNAL_STORAGE` and `/sdcard` across `backend/`,
  `bridges/`, `cpp/`, `assets/qml/` and `android/`. Record here that it was
  re-run and returned nothing, with the date. Nothing changes — the app is still
  inside the API 27–28 band where the permission would be required, and it still
  never touches shared external storage.

  **Re-run 2026-08-26. The conclusion holds, but "returned nothing" is not
  literally what happens** — the sweep returns three hits and each is benign.
  Recording them so the next run is not alarmed by them:

  | Hit | Why it is not shared-storage access |
  |---|---|
  | `cpp/utils.cpp:179` | A **comment** naming `Environment.getExternalStorageState(File)` |
  | `cpp/utils.cpp:196` | The call itself — a read-only **mount-state query** ("mounted", "removed", …) on a volume path, used by storage-volume enumeration. Reads no file, needs no permission. |
  | `android/AndroidManifest.xml:39` | A **comment** recording that androiddeployqt's injected permissions (incl. `WRITE_EXTERNAL_STORAGE`) are *not* used — i.e. the rationale for having dropped it |

  `/sdcard` — no match anywhere. No code path writes, reads or enumerates files
  in shared external storage; everything goes through SAF `content://` URIs or
  the app-private directory. The app remains inside the API 27–28 band where the
  permission *would* be required if it did, so the reasoning in
  `docs/android-multi-abi-and-chromeos.md` is unchanged by the floor moving to
  28.
- [x] 2.2 Build the authoritative site list:
  `grep -rn "minSdk\|API 27\|api 27" AGENTS.md PROJECT_MAP.md docs/ android/ CMakeLists.txt build-android.sh`
  and reconcile it against FR-5's list. Note any site FR-5 missed and any site
  FR-5 names that no longer exists.

  **Run 2026-08-26. Reconciliation: FR-5's list is accurate as far as it goes —
  every site it names still exists, none has vanished — but it is not complete.**

  Sites **FR-5 missed** (all real, all needing attention):

  | Site | What it says |
  |---|---|
  | `docs/android-soft-keyboard.md:254` | lists minSdk 28 among things arriving *with the Qt upgrade* — it no longer does (already covered by 2.10) |
  | `PROJECT_MAP.md:80` | "cannot load below API 28 despite `minSdk 27`" (2.5 covers `:52`/`:71`; this third hit is in the task list's Relevant Files but not in FR-5) |
  | `docs/android-qt-upgrade-considerations.md:406` | "re-check the minSdk (§2.2) rather than assuming the r28 exclusion still applies" |
  | `docs/android-qt-upgrade-considerations.md:443`, `:491` | the "Gradle, minSdk and packaging" sequencing advice — minSdk is no longer part of that bundle |
  | `docs/android-multi-abi-and-chromeos.md:801`, `:803` | the doc's own cross-links describing minSdk as a pending raise |

  Sites FR-5 names that **no longer exist**: none.

  **`build-android.sh` confirmed needing no edit** (4 hits — `:134`, `:138`,
  `:345`, `:347`, `:351`): all phrase the pin as "at this project's minSdk" and
  two state outright that the exclusion *"holds at minSdk 28 as well as 27"*.
  This matches 1.3.

  `docs/android-api-levels-and-feature-dependencies.md` carries by far the most
  hits (16), which is expected — it is the evidence base and much of it is
  deliberately historical. 2.11 handles which parts become past tense.
- [x] 2.3 Correct the code comments: `android/build.gradle` (the NDK comment near
  `:52`) and `CMakeLists.txt` (near `:516`). Both must keep saying the r28
  exclusion holds — only the "at minSdk 27" phrasing changes.

  Both rephrased to "at this project's minSdk" and both now state outright that
  **the exclusion holds at minSdk 28 as well as 27**, matching the wording
  `build-android.sh` already used. The r28 exclusion itself is untouched.
- [x] 2.4 Correct `AGENTS.md` (`:242`, `:386`, `:914`, `:1588`). `:386` is the
  "decision to raise minSdk to 28 **with the upgrade**" claim — it becomes "done
  on <date>, decoupled from the upgrade, because the override never worked".
  Remember `CLAUDE.md` is a symlink; edit `AGENTS.md` only.

  All four done, `AGENTS.md` only (`CLAUDE.md` is a symlink and was not
  touched). `:386` now reads *"the `minSdkVersion` raise to 28 — done on
  2026-08-26 and deliberately decoupled from the upgrade"* with the `getentropy`
  reason. `:914` gained the standing clarification that **the r28 exclusion is
  about API 30, not about 27 vs 28**, so the floor raise does not re-open it.
  `:1588` now says `minSdkVersion 28` and links the evidence doc.
- [x] 2.5 Correct `PROJECT_MAP.md` (`:52`, `:71`, `:80`) and **add links** to
  `docs/android-api-levels-and-feature-dependencies.md` from both `PROJECT_MAP.md`
  and `AGENTS.md`, so it is discoverable alongside the other Android docs (FR-5).

  `:52` → `minSdk 28`. `:71` — minSdk removed from the list of things deferred
  to the Qt upgrade (it is no longer deferred). `:80` — the "despite `minSdk 27`"
  clause replaced by the finding's actual consequence, that it turned the raise
  into a correctness fix, plus the 2026-08-26 date and the all-ABI sufficiency
  result.

  **Links:** `PROJECT_MAP.md:78` already carried one. `AGENTS.md` had none in
  its notable-feature-docs list, so a full entry was added there (after the
  android-qt-upgrade bullet) covering the `getentropy` finding, the WEAK-vs-GLOBAL
  distinction, the per-feature floors, and the `--abi` gotcha in
  `scripts/android-api-scan.sh`.
- [x] 2.6 Rewrite `docs/android-qt-upgrade-considerations.md` **§2.2** from
  "deferred to the Qt upgrade" to "done on <date>, and why it was decoupled" —
  the `getentropy` finding is the reason — and update §1's version table row.

  §2.2 rewritten as *"`minSdkVersion` 28 — **done 2026-08-26, and decoupled from
  the upgrade**"*. It keeps the 2026-07-29 decision as a **block quote** rather
  than deleting it, because the interesting content is *why that decision was
  wrong*: it rested on "the override demonstrably works for shipped users", and
  the scan showed the override never worked and there were no such users. Adds
  the confirmations (single source of truth; the per-ABI `--floor 28` runs; the
  NDK exclusion unchanged; `WRITE_EXTERNAL_STORAGE` unaffected) and the
  user-facing consequence, and states the general lesson — *"no technical risk,
  only a distribution cost, therefore no benefit on its own" is a conclusion
  that needs a measurement*.

  §1 table: the `minSdkVersion` row is now **28** ("raised 2026-08-26, decoupled
  from the upgrade"), and the NDK row now says the exclusion is about API **30**
  so it held at 27 and holds unchanged at 28.

  Also fixed the three sites in this doc that FR-5 did not list (found by 2.2):
  `:445` (re-verify against the new NDK, not the new minSdk — the floor is no
  longer a variable, and re-run the scan per ABI), `:482` and `:530` (minSdk
  removed from the "packaging work to do with the upgrade" sequence).
- [x] 2.7 Update `docs/android-multi-abi-and-chromeos.md`: the §2 permissions
  rationale (the "on API 27–28" sentence stays true and should say so
  explicitly), the `qtMinSdkVersion=28`-override note (there is no override any
  more), and the `aapt2` sample output.

  - §1a: `minSdkVersion` is 28, cross-linked to §2.2 of the upgrade doc.
  - The override note now says `build.gradle` **used to** override
    `qtMinSdkVersion=28` down to 27 and no longer does, so the two agree and
    there is no override left to misread.
  - Permissions rationale: says explicitly that **28 is still inside the API
    27–28 band**, so the reasoning is *re-affirmed rather than retired*, and
    would only lapse at 29. Also corrected the "returns nothing" claim to match
    2.1's actual result (three benign hits, described).
  - NDK-r28 precondition bullet: re-worded to bionic API 30+, "unaffected by the
    minSdk floor".
  - Cross-link list: minSdk removed from the *deferred* list, with a note that it
    shipped separately; added a link to
    `docs/android-api-levels-and-feature-dependencies.md`.
  - **`aapt2` sample output:** the doc's by-hand block had no sdkversion recipe
    at all (its samples covered features and alignment), so rather than
    correcting a sample, a `grep -i sdkversion` invocation was **added** with the
    expected `minSdkVersion:'28'` / `targetSdkVersion:'36'` output and the
    warning not to read the generated `gradle.properties` instead.
- [x] 2.8 Update `docs/android-beta-distribution-and-play-policy.md`'s
  `mipmap-anydpi-v26` note and its API-27 direct-call note.

  `:87` → `minSdkVersion 28`, noting the adaptive icon already won at 27 (both
  being above 26) so the raise only widened the margin. `:262` → minSdk 28, with
  the point that **28 is still below 30**, so `getInstallerPackageName()` is
  still not universally replaceable by `getInstallSourceInfo()` and the direct
  call stays correct.
- [x] 2.9 Check `docs/pure-rust-audio-backend.md` and
  `docs/relocated-storage-recovery.md` — both are expected to need **no** change
  (the first already states the exclusion does not lapse at 28; the second's
  "above the minSdk floor" claim concerns an API 30 call). Record the
  confirmation rather than editing for the sake of it.

  `docs/relocated-storage-recovery.md:174` — **confirmed, no edit made.**
  "`StorageVolume.getDirectory()` (API 30) is above the minSdk floor and is not
  used" still reads correctly: 30 > 28, so the sentence is as true at the new
  floor as at the old.

  `docs/pure-rust-audio-backend.md` — **the expectation was half right and the
  task's premise needs correcting.** Its headline claim is indeed fine (the
  exclusion is about API 30 and does not lapse at 28), but the same sentence
  carried a **stale parenthetical**: it said the raise to 28 was *"done as part
  of the Qt 6.10.3 upgrade"* — an upgrade that was abandoned and reverted, so
  the raise had in fact not happened at all when that was written. Corrected to
  "done on 2026-08-26, on its own — *not* as part of the Qt 6.10.3 upgrade,
  which was abandoned and reverted", and the tense moved to the past.
- [x] 2.10 Update `docs/android-soft-keyboard.md:254`, which lists minSdk 28
  among the things that arrive with the Qt upgrade — it no longer does.

  The checklist reference now names AGP/Gradle coupling and the predictive-back
  opt-out, and states explicitly that **minSdk 28 is no longer part of that
  checklist** — raised on its own on 2026-08-26.
- [x] 2.11 Update `docs/android-api-levels-and-feature-dependencies.md`: §1's
  declared-levels table (`minSdkVersion` is now 28), §4's last row (the "below
  the hard floor" defect is **resolved** — say when and by what), and §8 into the
  past tense. Paste 1.6's `--floor 28` output into §2.2 as the confirming run.

  - §1 table: **28** (Android 9), noting it was raised from 27 on 2026-08-26
    because of §2.
  - §2: the consequence paragraph moved to past tense, with a **Resolved
    2026-08-26** block quote recording the `aapt2` verification and that this
    finding is what decoupled the raise from the Qt upgrade.
  - §2.2: 1.6's three verdict lines pasted in as *"The confirming run for
    `minSdkVersion 28`"*, with the point that this proves 28 is a **sufficient**
    floor and that the two non-arm64 slices had never been scanned before. Also
    documented the two flags that turned out to be effectively mandatory
    (`--abi`, `--apk`) and the per-ABI loop, since the script scans one ABI and
    defaults to the newest APK under `build/`.
  - §4 last row: now **28**, "Matches the hard floor", with the defect recorded
    as historical.
  - §8: retitled *"Consequences of the raise … — done 2026-08-26"*, framed as
    predicted-vs-actual (every prediction held), and extended with the
    `WRITE_EXTERNAL_STORAGE` re-audit and the build-system result.
- [x] 2.12 Add the §9 standing rule as a rule, not a note (FR-5b): any new JNI
  call site records its API level in §5, and the §2.2 symbol scan is re-run on a
  Qt or NDK change.

  §9 retitled *"Standing rules: keeping this document true"* and the two items
  promoted into a block quote as **Rule 1** and **Rule 2**, each stating its
  failure mode — an unrecorded JNI call site degrades §7's triage into guesswork
  and is invisible until a user reports it; an unscanned floor change fails
  nothing in the build and first shows up as a device that will not start.
  Prefaced with why they are rules: the `getentropy` defect survived a year
  because nobody scanned the binaries.
- [ ] 2.13 Read the Play Console device/API-level distribution and **record the
  API-27 install share here** (FR-7). It does not gate anything — the decision is
  made — but it belongs in the record and may shape the release-notes wording.

  **BLOCKED — needs the maintainer.** The Play Console is behind an
  authenticated web session and cannot be read from this environment. Nothing
  else in Part A depends on it (FR-7 explicitly does not gate the change), so
  the rest of 2.0 was completed around it.

  To fill in: **Play Console → Statistics → filter by Android version / API
  level**, or **Release → App bundle explorer → Device catalog**. Record the
  API-27 (Android 8.1) share of active installs here. Expected to be very small,
  and note it counts installs that **cannot actually run the app** (§2 of
  `docs/android-api-levels-and-feature-dependencies.md`) — so a non-zero number
  is a count of broken installs, not of lost users.
- [x] 2.14 Final grep sweep: no file claims minSdk 27 except where it is
  explicitly historical ("was 27 until <date>"). Commit the doc changes.

  **Swept 2026-08-26** over `*.md`, `*.rs`, `*.qml`, `*.cpp`, `*.h`, `*.gradle`,
  `*.sh`, `*.ps1`, `*.txt`, `*.json`, `*.xml`, `*.conf`, excluding `build/`,
  `target/`, `node_modules/` and `tasks/archive/`.

  **The sweep found four live sites that neither FR-5 nor 2.2's grep had caught**
  — 2.2's grep covered only `AGENTS.md PROJECT_MAP.md docs/ android/
  CMakeLists.txt build-android.sh`, so it could not see `cpp/`, `scripts/` or
  the non-archived PRDs:

  | Site | Fix |
  |---|---|
  | `cpp/utils.cpp:262` | "must NOT be used at minSdk 27" → "at this project's minSdk (28, and still well below 30)" — the point is that `StorageVolume.getDirectory()` is API 30 |
  | `cpp/utils.cpp:750` | `getInstallerPackageName()` "works on every level the app supports (minSdk 27)" → 28, "still below 30" |
  | `scripts/generate_beta_app_icons.sh:42` | `minSdkVersion 27` → 28 (the `mipmap-anydpi-v26` note, twin of the one in the beta doc) |
  | `tasks/2026-07-31-…-prd---picker-url-handling…md:1281` | a **live, non-archived** PRD stating `minSdkVersion 27`; updated to 28 with the note that scoped-storage enforcement is `targetSdkVersion`-driven so its SAF-only conclusion is unaffected |

  Everything still matching after that is correct in context and was
  deliberately left: **historical statements** (`AGENTS.md:406`,
  `docs/pure-rust-audio-backend.md:41`,
  `docs/android-qt-upgrade-considerations.md:156`/`:237`,
  `docs/android-api-levels-and-feature-dependencies.md:52`/`:68`/`:248`), the
  **API 27–28 permission band**, which is still true at floor 28 and is the
  reason `WRITE_EXTERNAL_STORAGE` stays dropped, the `getentropy` explanation
  itself, and `tasks/archive/` (out of scope).

  One staleness fixed while sweeping: §2.1's heading was *"Symbols above API 27
  that are safe"*, and its table lists `getrandom` at API **28** — no longer
  above the floor. Retitled to "above the floor" with a note that the table was
  measured against the old floor.

  `scripts/qt-env-verify.sh` and `scripts/android-api-scan.sh` **read** the value
  out of `android/build.gradle` rather than hardcoding it, so both follow the
  change automatically — no edit needed.

  Rebuilt after the `cpp/utils.cpp` comment edits: `cmake --build` recompiles and
  links clean.

### 3.0 Part A — beta APK smoke pass on device, and the release-notes line

> **Spec.** FR-8, FR-6, FR-9. A minSdk floor change should be **invisible at
> runtime**; this pass exists to confirm that, not to explore. arm64 device only
> — non-arm64 runtime validation stays out of scope (PRD §5.4) and must remain
> documented as never run.
> **Depends on:** 1.0.

- [x] 3.1 Install the 1.4 beta APK on the arm64 phone
  (`make android-beta-debug-install`) and confirm it replaces the previous beta.

  Installed by the maintainer 2026-08-26 on device `RFCW112DLPR`. It replaced
  the previous beta (`io.github.simsapa.app.beta` is a single package; the
  released `io.github.simsapa.app` sits alongside it, as designed).

  **`adb shell dumpsys package` gives an independent confirmation of FR-2,
  from the installed package rather than the APK file:**

  ```
  versionCode=7 minSdk=28 targetSdk=36
  versionName=1.0.0-alpha.6-beta-debug
  ```

  Worth noting as a second verification route alongside `aapt2 dump badging` —
  it reads what the *platform* parsed, not what the build wrote.
- [x] 3.2 Smoke pass, all five: app launches; a sutta opens; search returns
  results; text entry works in the search field; audio record **and** playback
  work in Chanting Practice. Record pass/fail per item.

  **All five PASS.** Evidence from the logcat capture, item by item:

  | # | Item | Result | Evidence |
  |---|---|---|---|
  | 1 | App launches | **PASS** | Full startup chain: `gui::start()` → `DbManager::new()` → all three DBs opened → both migration runners "no pending migrations" → `start_webserver()`. **No `dlopen` failure** — the exact symptom a wrong floor would produce |
  | 2 | A sutta opens | **PASS** | `get_sutta_html_by_uid(): window_id: window_0, uid: mn22/pli/ms`, after `mn22/en/bodhi` — both translations rendered |
  | 3 | Search returns results | **PASS** | `results_page() start - query='nn22', search_area='Suttas'`, then `query='uid:mn22'`; also `dpd_lookup_grouped(): query_text_orig: samayena` (63 ms) from a word lookup in the open sutta |
  | 4 | Text entry in the search field | **PASS** | The `'nn22'` → `'uid:mn22'` sequence *is* the proof: a mistyped `n` for `m` then a corrected query is a human typing into the field, not a programmatic query |
  | 5 | Audio record **and** playback | **PASS** | `recording started` → `flacenc` 158 frames / 7 workers → `recording finished` → `check_file: …_user_….flac exists: true` → `symphonia` found the FLAC marker. Playback confirmed **visually by the maintainer** ("playback worked and looked fine") |

  Item 5 is the one that mattered most: the audio stack is where the real native
  floors live (cpal → AAudio, `libaaudio.so`, API 26), and it works unchanged.
- [x] 3.3 Watch `adb logcat -s simsapa Qt QtCore QtQml` during the pass for any
  new load-time or JNI error. Record that the log was read and what it showed.

  Log captured live during the whole pass (2,656 lines, `simsapa` + `Qt` +
  `QtCore` + `QtQml` + `AndroidRuntime:E` + `DEBUG:E`) and watched by a monitor
  filtering for crashes, JNI failures, QML type errors and Rust panics.

  **Zero load-time errors and zero JNI errors. Exactly one `ERROR` line in the
  entire capture:**

  ```
  E/simsapa: qt_thread.queue() failed in audio_manager::load (window closed?):
             Cannot queue function pointer as object has been destroyed
  ```

  **This is a by-design, pre-existing, handled path — not a regression and not
  related to minSdk.** It is precisely the `ThreadingQueueError::ObjectDestroyed`
  case that `crate::queue_or_log()` exists for (AGENTS.md §"`qt_thread.queue()`",
  `docs/window-lifecycle-and-reuse.md` §5c): bridge objects are per-engine and
  secondary windows are destroyed on close, so a completion signal queued against
  a departed window is logged and skipped rather than panicking the worker
  thread. It fired 300 ms *after* the recording was written and decoded, and the
  maintainer confirmed playback was visually correct — i.e. the dropped signal
  had no user-visible effect.

  The only other non-INFO line in the capture is Rocket's own launch banner,
  which Rocket logs at warn level (`Rocket has launched from
  http://127.0.0.1:4848`).

  **Conclusion: the floor change is invisible at runtime, which is exactly what
  FR-8 set out to confirm.**
- [x] 3.4 Add the release-notes line (FR-6): **Play stops offering updates to
  devices below API 28 (Android 8.1); existing installs on those devices keep the
  last compatible version and are not uninstalled**, and sideloaded APKs will
  refuse to install on API 27. Put it wherever this project's release notes live;
  if there is no such file yet, record the exact wording here for the release.

  **There is no release-notes file in this repo** — checked: no `CHANGELOG*`, no
  `RELEASES*`, no `*release*note*` anywhere outside `node_modules`
  (`assets/releases-fallback.json` and `docs/releases-info-and-fallback.md` are
  the *update-check* mechanism, not release notes). Releases are described on the
  GitHub Releases page. Per this task's fallback, the exact wording to use is
  recorded here:

  > **Android: minimum version is now Android 9.0 (API 28).**
  >
  > Google Play will no longer offer updates to devices running Android 8.1 or
  > older. **If you already have Simsapa installed on such a device it will keep
  > working and will not be uninstalled** — it simply stays on the last
  > compatible version. APKs downloaded from GitHub Releases will likewise
  > refuse to install on Android 8.1.
  >
  > This corrects a long-standing packaging error rather than dropping working
  > devices: the app declared support for Android 8.1 but could never actually
  > start on it, because a core Qt library requires a system function
  > (`getentropy`) that Android only provides from 9.0 onwards.

  The last paragraph is deliberate — without it the note reads as *"we dropped
  your device"*, when in fact no device that could run the app lost anything.
- [x] 3.5 Note in this file that `android/version.txt` must be bumped before the
  next Play upload (FR-9) — **this PRD does not make a release**, so do not bump
  it here.

  **Noted, and deliberately NOT done.** `android/version.txt` currently holds
  **7**, which is what the 1.4 beta APK carries. Google Play requires a strictly
  increasing `versionCode` on every upload, so **the next Play upload must bump
  it first** (edit the integer, then `make android-aab` — no version arguments).
  The versionName comes separately from `bridges/Cargo.toml` (`1.0.0-alpha.6`).

  This PRD makes no release, so the file is left untouched. Note the beta APK
  built here consumed no versionCode as far as Play is concerned — it was never
  uploaded.

### 4.0 Part C — report the two upstream cxx-qt defects

> **Spec.** FR-21, FR-22, FR-23. Independent of everything else and cheap. The
> deliverable is that the next reader of `docs/cxx-qt-fork.md` finds an issue
> link instead of a dead end. **A recorded decision not to file is an acceptable
> outcome for FR-22 only** — FR-21 is to be reported.
> **Depends on:** nothing. **Blocks:** nothing.

- [ ] 4.1 Build the one-file reproducer: a minimal cxx-qt project whose
  `QmlModule` carries a single `qml_files` entry written as `"../foo/Bar.qml"`.
  Keep it outside this repo (or under the scratchpad) — it is not a project
  artifact.
- [ ] 4.2 Capture the three disagreeing derivations from that reproducer's build
  output as evidence: the rcc alias (`..` folded), the generated `qmldir`
  component line (`..` **not** folded), and qmlcachegen's `--resource-path`
  (`..` not folded). These are the concrete outputs the issue needs.
- [ ] 4.3 Capture the second defect's evidence: `qmlcache_loader.cpp` inserts the
  key raw but looks it up through `QDir::cleanPath`, so a key containing `/../`
  is unreachable. Quote both the insert and the lookup site.
- [ ] 4.4 File the issue(s) with KDAB/cxx-qt covering both defects (FR-21), with
  the reproducer and the note that this is why the AOT cache has never been
  consulted in a real project.
- [ ] 4.5 Decide on FR-22 — the missing
  `cargo::rerun-if-env-changed=CXX_QT_AUTORCC_OPTIONS` in `cxx-qt-build`, a
  one-line `println!`. File it or record the decision not to, **explicitly**,
  either way.
- [ ] 4.6 Record the issue URLs (or the recorded non-filing decision) in
  `docs/cxx-qt-fork.md` **next to the findings they document** — §5's table and
  the "Side finding" section (FR-23). Commit Part C on its own.

### 5.0 Part B — baseline and threshold, before any change

> **Spec.** FR-10 and Open Question 1. **The measurement is the deliverable of
> Part B, not the move.** Without a baseline the change cannot be evaluated and
> "the whole task is void". Agree the keep/revert threshold **now**, in writing,
> so the decision is not made after seeing the number.
> **Depends on:** nothing (but do it after A and C are committed, per §6.1).
> **Blocks:** 6.0, 8.0, 9.0.

- [ ] 5.1 Agree and write down the threshold here, before measuring. PRD
  proposal: **keep if cold `engine.load()` improves by ≥ 100 ms**, and the binary
  size and build-time regressions from ~12.4 MB of regenerated C++ are judged
  acceptable; otherwise revert. Record the agreed form, including how "cold" is
  defined and how many runs count.
- [ ] 5.2 Reconfigure clean (`make build -B` from a fresh `build/` directory) so
  no stale `CMAKE_PREFIX_PATH` is in play, and confirm the configure log names
  the Qt kit you expect (`~/Qt/6.9.3/gcc_64`).
- [ ] 5.3 Baseline metric 1 — cold `engine.load()`: run the app several times
  (record N, ≥ 5) and take the delta between the
  `STARTUP-TRACE: engine.load() start` and `engine.load() end` lines from
  `log.txt`. Record every run and the median, not just a summary.
- [ ] 5.4 Baseline metric 2 — stripped binary size of
  `build/simsapadhammareader/simsapadhammareader`. Record the exact command and
  the byte count. **Control for a confound the PRD does not mention:**
  `bridges/src/api.rs:256` embeds the whole `../assets/` tree with `include_dir!`
  and serves it at `/assets/<path..>`, so ~2.4 MB of QML source is in the binary
  today and **leaves it when the tree moves**, independently of qmlcachegen.
  Record `du -sb assets/qml` alongside the binary size so 8.4's delta can be
  read as "AOT cost minus embed saving" rather than a single opaque number.
  (Checked: nothing requests `/assets/qml/…` over HTTP, so dropping it from the
  embedding is behaviourally safe — only the measurement is affected.)
- [ ] 5.5 Baseline metric 3 — `make build -B` wall-clock from clean. Record it.
- [ ] 5.6 Save the pre-change **generated `.qrc`** and the generated `qmldir`
  from `bridges/`'s build output to the scratchpad. 7.2 diffs against these, and
  they cannot be reconstructed after the move.

### 6.0 Part B — move `assets/qml/` under `bridges/` and switch `build.rs`

> **Spec.** FR-11, FR-12, FR-13, FR-14. `git mv` is the **only accepted
> mechanism** — a repo symlink and `set_current_dir("..")` were both evaluated
> and rejected in the predecessor PRD (Windows `core.symlinks`; broken
> incremental builds via raw-path `rerun-if-changed`). The move must be
> **alias-neutral**: from `bridges/` the path becomes `assets/qml/Foo.qml`, so
> the resource path stays `:/qt/qml/com/profoundlabs/simsapa/assets/qml/Foo.qml`
> and the 12 `qrc:` literals in `cpp/` are untouched.
> **The `git mv` and the `build.rs` switch are ONE commit** — the build does not
> work between them.
> **Depends on:** 5.0 (baseline must exist first). **Blocks:** 7.0.

- [ ] 6.1 `git mv assets/qml bridges/assets/qml`, taking the whole tree: the 108
  root `.qml` files including the 13 `tst_*.qml`, plus
  `com/profoundlabs/simsapa/` (bridge stubs + `qmldir`), `data/`
  (`BojjhangaData.qml` + its own `qmldir`) and `icons/`.
- [ ] 6.2 **Re-point the `icons` symlink.** `assets/qml/icons` is a tracked
  symlink (git mode `120000`) whose target is `../icons`; after the move that
  resolves to the non-existent `bridges/assets/icons`, and it breaks **silently**
  — the Qt resource still supplies the icons (6.3), so only filesystem consumers
  notice: `qmllint`, `qmltestrunner`, `scripts/qml-watch.sh`, macdeployqt's
  `-qmldir` scan and QML previews. In the same commit, re-point it to
  `../../../assets/icons` (from `bridges/assets/qml/` that is the repo root) and
  verify with `readlink` plus an `md5sum` through the link. Do **not** replace it
  with a copy — `assets/icons/32x32/` holds 117 files that `assets/icons.qrc`
  also references.
- [ ] 6.3 Confirm `assets/icons.qrc` stays where it is and needs no edit: its
  `<file>` entries name `icons/32x32/…`, which resolve against the `.qrc`'s own
  directory, i.e. `assets/icons/` (**not** through the `assets/qml/icons`
  symlink), and its `prefix` is the shared QML resource directory. Its only
  consumer is `CMakeLists.txt:398`. Verify by reading the file (FR-13).
- [ ] 6.4 `bridges/build.rs`: strip the `../` from all 96 entries in the list.
- [ ] 6.5 `bridges/build.rs`: move the QML files from
  `CxxQtBuilder::qrc_resources(…)` back into
  `QmlModule::new(URI).qml_files(…)` with the now `..`-free paths, and delete the
  alias-derivation block **and** its `panic!` (FR-14). Drop the now-unused
  `QResource`/`QResourceFile`/`QResources` imports if nothing else uses them.
  Note that `qml_files` takes anything `Into<QmlFile>`, so plain `&str` paths
  work unchanged; `.singleton(true)` / `.version(…)` are not needed here (no
  registered file is a singleton).
- [ ] 6.6 Carry the deleted block's long comment — the runtime bug it existed for
  — **into `docs/cxx-qt-fork.md` §5** rather than deleting it with the code
  (FR-14). §5 already tells most of this story; make sure nothing is lost.
- [ ] 6.7 Update the `Makefile`: `qml-lint` (`:102`), `qml-test` (`:105`), the
  commented single-test line (`:90`), and both `tokei` exclude lists (`:70`,
  `:73`, which name `assets/qml/data/` and
  `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`).
- [ ] 6.8 Update `build-macos.sh:233` (`-qmldir=./assets/qml`),
  `build-appimage.sh:179` (`QML_SOURCES_PATHS="$QT6_PATH/qml:./assets/qml"`) and
  **`appimage.conf:8`** (`qml_sources_paths = assets/qml`) — the third is a
  consumer FR-12 does not list. Check whether `build-appimage.sh` reads
  `appimage.conf` or whether the two are independent copies of the same value;
  if independent, say so in a comment so the next mover finds both.
- [ ] 6.9 Update `AGENTS.md`: the "New QML components" rule and its path form,
  the `console`-API exception for the stub directory, and the
  `tst_dialog_loop_harness` instructions ("copy it into `assets/qml/` as a
  `tst_*.qml`"). `PROJECT_MAP.md` (~30 hits, incl. `:128`, `:196`, `:197`) and
  every `docs/*.md` that names the path.
- [ ] 6.10 Check `scripts/qml-watch.sh`, `scripts/tst_dialog_loop_harness.qml.keep`
  and `scripts/qt-env.sh` for hardcoded `assets/qml` paths. Confirmed during the
  review: **`.claude/settings.json` does not name it**. Note that `.qmlls.ini` is
  gitignored, so each developer's qmlls config may need re-pointing by hand —
  mention it in the commit message rather than trying to fix it in-tree.
- [ ] 6.11 Grep the whole tree for `assets/qml` and confirm every remaining hit
  is either the new location or an intentionally historical mention. Record the
  count before and after.
- [ ] 6.12 `make build -B` from clean. The `git mv`, the symlink re-point and the
  `build.rs` switch land as **one commit**.

### 7.0 Part B — artifact checks, before trusting any runtime result

> **Spec.** FR-15 and FR-13. These are the checks that catch the failure modes
> that are silent at runtime. Do them before measuring anything.
> **Depends on:** 6.0. **Blocks:** 8.0.

- [ ] 7.1 The generated `qmldir` component lines must now **resolve** — e.g.
  `Logger 1.0 assets/qml/Logger.qml`, with no `../`. This is the thing that broke
  under cxx-qt 0.8. Paste the first few lines here.
- [ ] 7.2 Diff the generated `.qrc` aliases against 5.6's saved pre-change copy.
  They must be **byte-identical** (FR-13) — this is the property the whole
  approach rests on. If they are not, stop: the `qrc:` literals in `cpp/` and
  `assets/icons.qrc`'s prefix are about to break.
- [ ] 7.3 Confirm the prefix is still `/qt/qml/com/profoundlabs/simsapa` and that
  **zero** `..` remain anywhere in the generated `.qrc`.
- [ ] 7.4 Inspect the generated `qmlcache_loader.cpp`: **every** key must be free
  of `/../`, so it can match a `QDir::cleanPath`-ed lookup (FR-15). Record the
  key count and the grep that proves none contains `/../`.
- [ ] 7.5 Confirm implicit same-directory type resolution still works in the
  **root** directory — the reason `Logger.qml` needs no import. (`assets/qml/data/`
  is *not* the case to worry about: it is absent from the resource entirely and
  its only reference is commented out.) `make qml-lint` and `make qml-test` are
  the cheap checks; a launch that opens several windows is the real one.
- [ ] 7.6 Check the icons still resolve **from the filesystem** as well as from
  the resource: run `make qml-lint` and open a QML preview, which is what 6.2's
  symlink serves. A broken symlink does not fail the build.

### 8.0 Part B — prove the cache is hit at runtime, then re-measure

> **Spec.** FR-16 and FR-17. **"It built and started" is not evidence.** Stale,
> mismatched or unmatched AOT units fall back silently to parsing QML source,
> which is exactly today's behaviour, so a green run is indistinguishable from
> total failure without this proof. If the cache cannot be shown to be hit, the
> task fails here and 9.0's revert branch applies.
> **Depends on:** 7.0. **Blocks:** 9.0.

- [ ] 8.1 Run with `QT_LOGGING_RULES="qt.qml.diskcache.debug=true"` and capture
  the output. Record concrete lines showing units being **loaded from the
  cache**, not compiled.
- [ ] 8.2 Run the deliberate-mismatch control: perturb one QML file so its unit
  *should* be rejected, and confirm the log reports the rejection for that unit
  and only that unit. Without this control, 8.1's log could be reporting
  something else. Revert the perturbation afterwards.
- [ ] 8.3 Re-measure metric 1 (cold `engine.load()`) with the same N and the same
  method as 5.3. Record every run and the median.
- [ ] 8.4 Re-measure metric 2 (stripped binary size) and metric 3
  (`make build -B` wall-clock from clean), same commands as 5.4/5.5. Report the
  size as two components per 5.4: the AOT units added, and the ~2.4 MB of QML
  source no longer embedded by `include_dir!`. A single net number would
  understate the AOT cost.
- [ ] 8.4a Confirm the incremental-rebuild side benefit: touch one QML file and
  check that `cargo`/`make build` actually rebuilds the resource, which the
  current `../`-prefixed `rerun-if-changed` paths do not reliably do. Record the
  result — it is an argument for 9.1 that does not depend on the timing number.
- [ ] 8.5 Write the before/after comparison table here — three metrics, both
  columns, plus the cache-hit evidence. **Record the result either way**; a null
  result closes a question open since cxx-qt 0.7 (FR-17).

### 9.0 Part B — decide keep or revert, and record it

> **Spec.** FR-18, and Open Questions 1, 2 and 4. **Reverting is an acceptable,
> complete outcome of this PRD**, provided the measurement and the decision are
> recorded. The design question — whether app UI belongs under `bridges/` — is
> legitimate and "no" is an allowed answer even if the win is real.
> **Depends on:** 8.0.

- [ ] 9.1 Compare 8.5's numbers against 5.1's agreed threshold and state the
  decision plainly here: **keep** or **revert**, with the reason.
- [ ] 9.2 If the desktop result is positive, take one Android on-device
  measurement of `engine.load()` to answer Open Question 2 — does the Android
  build benefit at all, given its startup is dominated by other costs? If the
  desktop result is null, record that this was **not** measured and why.
- [ ] 9.3 If **revert**: `git revert` (or reverse) the 6.0 commit in full,
  confirm `assets/qml/` is back at the top level, the `icons` symlink points at
  `../icons` again (`readlink` it — a reverted symlink is the easiest thing to
  get silently wrong), `bridges/build.rs` is back on `qrc_resources` with its
  alias block and comment, and `make build -B` + `make qml-test` pass. The
  `docs/cxx-qt-fork.md` record of the measurement **stays**.
- [ ] 9.4 If **keep**: confirm nothing in 7.0 regressed after any follow-up edits,
  and that the 12 `qrc:` literals in `cpp/` are untouched in `git diff`.
- [ ] 9.5 Record the measurement **and** the decision in `docs/cxx-qt-fork.md`'s
  "Side finding" section, replacing "enabling AOT for real is an untried
  improvement" with what was actually measured, so the next reader does not
  re-derive it (FR-18).
- [ ] 9.6 Commit the decision (and the revert, if that is the outcome) separately
  from 6.0's commit.

### 10.0 Final sweep — cross-platform verification and consistency

> **Spec.** FR-19, FR-20, Success Metrics 4 and 6, and PRD §5's non-goals. Runs
> against whichever layout 9.0 settled on.
> **Depends on:** 9.0.

- [ ] 10.1 `AGENTS.md`'s "New QML components" rule must describe the path form
  **actually** in `bridges/build.rs`, and — if the `qrc_resources` block survived
  — its build-time `panic!` message must match the rule (FR-19). The rule and the
  code must not disagree, whichever way 9.0 went.
- [ ] 10.2 Linux: `make build -B`, `make qml-lint`, `make qml-test`, and an
  AppImage build (`make appimage -B`) all succeed.
- [ ] 10.3 macOS and Windows: re-verify the build still **configures and
  completes** after the path change, on whichever of the two is available. If one
  is not available, say so explicitly rather than implying it was tested.
- [ ] 10.4 Android: build one beta APK and confirm QML still loads on device.
  Resource paths are unchanged, so this is a confirmation, not an investigation
  (FR-20). Note that x86_64 and armeabi-v7a remain **never functionally run** —
  out of scope (PRD §5.4) and must stay documented as outstanding.
- [ ] 10.5 `git diff` against the pre-PRD tree shows **no** change to any `QT_*`
  variable in `CMakeLists.txt`, to `android/build.gradle`'s AGP line, to the
  Gradle wrapper, to the JDK pin or to the NDK pin (Success Metric 6, PRD §5.1–5.2).
- [ ] 10.6 Confirm the untouched list is untouched: `useLegacyPackaging`, the
  16 KB `max-page-size` link flag, the predictive-back opt-out,
  `assets/icons.qrc`, and the `qrc:` literals in `cpp/`.
- [ ] 10.7 Walk PRD §7's six success metrics and check each off with the evidence
  recorded in this file. Set the PRD's Status header to reflect the outcome
  (including "Part B reverted" if that is what happened).
