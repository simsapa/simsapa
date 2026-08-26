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
- [ ] 1.7 Commit Part A's code change on its own (docs follow in 2.0, or fold
  them in — but nothing from Part B or C in this commit).

### 2.0 Part A — correct every "minSdk 27" claim, and record the scan, permission re-check and Play share

> **Spec.** FR-4, FR-5, FR-5b, FR-7. The rule is that after this task **no file
> in the tree claims the app's minSdk is 27** (Success Metric 2). Line numbers in
> the PRD's list will have moved — re-derive the site list by grep rather than
> trusting it, and treat the PRD list as a completeness check.
> **Depends on:** 1.0.

- [ ] 2.1 Re-run the `WRITE_EXTERNAL_STORAGE` audit (FR-4): grep
  `getExternalStorage`, `EXTERNAL_STORAGE` and `/sdcard` across `backend/`,
  `bridges/`, `cpp/`, `assets/qml/` and `android/`. Record here that it was
  re-run and returned nothing, with the date. Nothing changes — the app is still
  inside the API 27–28 band where the permission would be required, and it still
  never touches shared external storage.
- [ ] 2.2 Build the authoritative site list:
  `grep -rn "minSdk\|API 27\|api 27" AGENTS.md PROJECT_MAP.md docs/ android/ CMakeLists.txt build-android.sh`
  and reconcile it against FR-5's list. Note any site FR-5 missed and any site
  FR-5 names that no longer exists.
- [ ] 2.3 Correct the code comments: `android/build.gradle` (the NDK comment near
  `:52`) and `CMakeLists.txt` (near `:516`). Both must keep saying the r28
  exclusion holds — only the "at minSdk 27" phrasing changes.
- [ ] 2.4 Correct `AGENTS.md` (`:242`, `:386`, `:914`, `:1588`). `:386` is the
  "decision to raise minSdk to 28 **with the upgrade**" claim — it becomes "done
  on <date>, decoupled from the upgrade, because the override never worked".
  Remember `CLAUDE.md` is a symlink; edit `AGENTS.md` only.
- [ ] 2.5 Correct `PROJECT_MAP.md` (`:52`, `:71`, `:80`) and **add links** to
  `docs/android-api-levels-and-feature-dependencies.md` from both `PROJECT_MAP.md`
  and `AGENTS.md`, so it is discoverable alongside the other Android docs (FR-5).
- [ ] 2.6 Rewrite `docs/android-qt-upgrade-considerations.md` **§2.2** from
  "deferred to the Qt upgrade" to "done on <date>, and why it was decoupled" —
  the `getentropy` finding is the reason — and update §1's version table row.
- [ ] 2.7 Update `docs/android-multi-abi-and-chromeos.md`: the §2 permissions
  rationale (the "on API 27–28" sentence stays true and should say so
  explicitly), the `qtMinSdkVersion=28`-override note (there is no override any
  more), and the `aapt2` sample output.
- [ ] 2.8 Update `docs/android-beta-distribution-and-play-policy.md`'s
  `mipmap-anydpi-v26` note and its API-27 direct-call note.
- [ ] 2.9 Check `docs/pure-rust-audio-backend.md` and
  `docs/relocated-storage-recovery.md` — both are expected to need **no** change
  (the first already states the exclusion does not lapse at 28; the second's
  "above the minSdk floor" claim concerns an API 30 call). Record the
  confirmation rather than editing for the sake of it.
- [ ] 2.10 Update `docs/android-soft-keyboard.md:254`, which lists minSdk 28
  among the things that arrive with the Qt upgrade — it no longer does.
- [ ] 2.11 Update `docs/android-api-levels-and-feature-dependencies.md`: §1's
  declared-levels table (`minSdkVersion` is now 28), §4's last row (the "below
  the hard floor" defect is **resolved** — say when and by what), and §8 into the
  past tense. Paste 1.6's `--floor 28` output into §2.2 as the confirming run.
- [ ] 2.12 Add the §9 standing rule as a rule, not a note (FR-5b): any new JNI
  call site records its API level in §5, and the §2.2 symbol scan is re-run on a
  Qt or NDK change.
- [ ] 2.13 Read the Play Console device/API-level distribution and **record the
  API-27 install share here** (FR-7). It does not gate anything — the decision is
  made — but it belongs in the record and may shape the release-notes wording.
- [ ] 2.14 Final grep sweep: no file claims minSdk 27 except where it is
  explicitly historical ("was 27 until <date>"). Commit the doc changes.

### 3.0 Part A — beta APK smoke pass on device, and the release-notes line

> **Spec.** FR-8, FR-6, FR-9. A minSdk floor change should be **invisible at
> runtime**; this pass exists to confirm that, not to explore. arm64 device only
> — non-arm64 runtime validation stays out of scope (PRD §5.4) and must remain
> documented as never run.
> **Depends on:** 1.0.

- [ ] 3.1 Install the 1.4 beta APK on the arm64 phone
  (`make android-beta-debug-install`) and confirm it replaces the previous beta.
- [ ] 3.2 Smoke pass, all five: app launches; a sutta opens; search returns
  results; text entry works in the search field; audio record **and** playback
  work in Chanting Practice. Record pass/fail per item.
- [ ] 3.3 Watch `adb logcat -s simsapa Qt QtCore QtQml` during the pass for any
  new load-time or JNI error. Record that the log was read and what it showed.
- [ ] 3.4 Add the release-notes line (FR-6): **Play stops offering updates to
  devices below API 28 (Android 8.1); existing installs on those devices keep the
  last compatible version and are not uninstalled**, and sideloaded APKs will
  refuse to install on API 27. Put it wherever this project's release notes live;
  if there is no such file yet, record the exact wording here for the release.
- [ ] 3.5 Note in this file that `android/version.txt` must be bumped before the
  next Play upload (FR-9) — **this PRD does not make a release**, so do not bump
  it here.

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
