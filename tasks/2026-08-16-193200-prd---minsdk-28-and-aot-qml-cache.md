# PRD: minSdkVersion 28 and the AOT QML cache — Qt-independent carry-overs

- **Date:** 2026-08-16
- **Status:** Draft
- **Scope:** Three items carried over from the **closed** Qt 6.10.3 PRD that do
  **not** depend on a Qt upgrade. **No Qt version changes in this work.**

## Source documents

The two predecessor documents, both closed/archived, carry the measurement trail
and must be read before starting:

- `tasks/archive/2026-08-07-175059-prd---cxx-qt-and-qt-6-10-3-android-upgrade.md`
  and its task list — **CLOSED 2026-08-08, stage 2 reverted.** `QT_ANDROID` is
  back at `6.9.3`. This PRD picks up three of its unchecked tasks.
- `tasks/archive/2026-07-27-131601-prd---android-api-36-compliance-and-packaging-follow-ups.md`
  — the original source of the minSdk deferral.
- **[docs/android-api-levels-and-feature-dependencies.md](../docs/android-api-levels-and-feature-dependencies.md)
  — written for this PRD, and the evidence base for Part A.** The measured
  per-feature API inventory, the two Qt floors (6.9.3 and 6.11), and the finding
  that `libQt6Core` hard-requires API 28. Also the crash-triage playbook that
  outlives this PRD.
- [docs/android-qt-upgrade-considerations.md](../docs/android-qt-upgrade-considerations.md)
  §2.2 (minSdk), §0 (why the upgrade was reverted).
- [docs/cxx-qt-fork.md](../docs/cxx-qt-fork.md) §"Side finding: the AOT QML cache
  has never been used in this project".
- [docs/pure-rust-audio-backend.md](../docs/pure-rust-audio-backend.md) — the NDK
  constraint, which this PRD does **not** change.

---

## 1. Introduction / Overview

The August 2026 attempt to move Android to Qt 6.10.3 was abandoned: it did not
fix the Gboard/Thai bug it was undertaken for, and it broke the Android UI. The
PRD was closed with roughly seventy tasks unchecked, most of which only made
sense *with* an upgrade.

Three of them do not depend on the Qt version at all, and are stranded behind an
upgrade that is now indefinitely deferred:

1. **`minSdkVersion` 27 → 28.** The app ships one API level *below* what its own
   Qt declares it supports (Qt 6.9.3 writes `qtMinSdkVersion=28`;
   `android/build.gradle` overrides it back down to 27). The 2026-07-29 decision
   was to raise it "with the upgrade" — which now means "never". **A binary scan
   done for this PRD shows the override does not work at all**: `libQt6Core`
   imports `getentropy`, which bionic first provides at API 28, as a non-weak
   symbol — so the app cannot load on an API 27 device. What looked like a
   product trade-off is a correctness defect.
2. **The AOT QML cache.** `qmlcachegen`'s output has **never once been consulted**
   in this project, under cxx-qt 0.7 or 0.9: the generated loader inserts keys
   containing `/../` and looks them up through `QDir::cleanPath`, which strips
   them, so no key can match. 87 compiled units — 12.4 MB of generated C++ plus a
   52 KB loader — were compiled into every binary and never used. That dead
   weight is gone; **enabling the cache for real is an untried improvement**, and
   it is blocked only by our own directory layout, not by any Qt version.
3. **Two upstream defects**, found while diagnosing (2), never reported.

**Goal:** land all three, each measured on its own terms, without touching a
single Qt version variable.

---

## 2. Goals

1. `minSdkVersion` is **28**, verified on the built artifact, with every "at
   minSdk 27" claim in the code comments and docs corrected — closing the gap
   where Play offers the app to devices on which `libQt6Core` cannot load.
2. A **measured** answer to "does AOT `qmlcachegen` help this app?" — with proof
   the cache is actually *hit* at runtime, not merely generated. **A null result
   that reverts the change is a successful outcome of this PRD**, provided it is
   recorded.
3. Two upstream defects reported to KDAB with a reproducer, so the next reader of
   `docs/cxx-qt-fork.md` finds an issue link instead of a dead end.
4. No Qt version, AGP, Gradle wrapper, JDK or NDK change anywhere in this work.

---

## 3. User Stories

- **As a user on an Android 8.1 (API 27) device**, I keep the last version that
  supported me and am not offered a broken update — I am not left running a
  configuration Qt does not claim to support.
- **As a user on any platform**, the app's window appears sooner, or it does not
  and nobody spends another year wondering whether the QML cache would have
  helped.
- **As the next developer**, when I read that the AOT cache has never worked, I
  find the measurement that decided the matter and an upstream issue number,
  rather than an open question I have to re-derive.

---

## 4. Functional Requirements

### Part A — `minSdkVersion` 27 → 28

**A safety analysis was performed for this PRD and is written up in
[docs/android-api-levels-and-feature-dependencies.md](../docs/android-api-levels-and-feature-dependencies.md).
It found something stronger than expected: the app already cannot load below
API 28.** `libQt6Core_arm64-v8a.so` carries a **GLOBAL** undefined
`getentropy@LIBC_P`, a bionic symbol that first exists at API 28, so on an
Android 8.1 device `dlopen` fails before any app code runs. Raising the floor
therefore drops **no working users** — it stops Play offering the app to devices
where it was never going to start. The requirements below encode that analysis
so it is not re-derived.

- **FR-1** Change `minSdkVersion 27` → `minSdkVersion 28` in
  `android/build.gradle`'s `defaultConfig` (currently line 153). **That is the
  only source of truth.** There is no `<uses-sdk>` element in
  `android/AndroidManifest.xml` and no `QT_ANDROID_TARGET_SDK_VERSION` in
  `CMakeLists.txt`; the implementation must confirm this rather than assume it.

- **FR-2** Verify on the **artifact**, with
  `aapt2 dump badging <apk> | grep -i sdkversion` → `minSdkVersion:'28'` and
  `targetSdkVersion:'36'`. **Do not verify by reading the generated
  `android-build/gradle.properties`** — androiddeployqt writes values there that
  `build.gradle` never reads (it already writes `qtMinSdkVersion=28` today, and a
  stale `qtTargetSdkVersion=35`).

- **FR-3** The NDK exclusion is **unchanged and must not be re-litigated**. NDK
  r28 is excluded because its libc++ references `pthread_cond_clockwait`, which
  bionic declares only at **API 30+** — 28 is still below 30, so the exclusion
  holds exactly as before. `build-android.sh` (both the pin comment and the r28
  backstop) already says "holds at minSdk 28 as well as 27" and needs no edit.
  Stay on r26b/r27.

- **FR-4** Re-affirm the `WRITE_EXTERNAL_STORAGE` reasoning rather than changing
  it. `docs/android-multi-abi-and-chromeos.md` justifies dropping the permission
  by noting it is genuinely required on **API 27–28** for shared external
  storage, and that Simsapa never touches shared external storage (everything
  goes through SAF `content://` URIs or the app-private directory). At minSdk 28
  the app is still inside that band, so **nothing changes**; re-run the
  `getExternalStorage` / `EXTERNAL_STORAGE` / `/sdcard` grep across `backend/`,
  `bridges/`, `cpp/`, `assets/qml/` and `android/` to confirm it still returns
  nothing, and record that it was re-run.

- **FR-5** Correct every "at minSdk 27" / "`minSdkVersion` is 27" statement in
  code comments and documentation. Known sites (verify the list is complete
  before editing — line numbers will have moved):
  - `android/build.gradle:52` (NDK comment), `CMakeLists.txt:516`
  - `AGENTS.md` (four sites) — `CLAUDE.md` is a symlink to it
  - `PROJECT_MAP.md:52`, `PROJECT_MAP.md:71`
  - `docs/android-qt-upgrade-considerations.md` §1 version table and **§2.2**,
    which must be rewritten from "deferred to the upgrade" to "done, on
    <date>, and why it was decoupled from the upgrade"
  - `docs/android-multi-abi-and-chromeos.md` (§2 permissions rationale, the
    `qtMinSdkVersion=28` override note, the `aapt2` sample output)
  - `docs/android-beta-distribution-and-play-policy.md` (the
    `mipmap-anydpi-v26` note and the API-27 direct-call note)
  - `docs/pure-rust-audio-backend.md` — already states the exclusion does not
    lapse at 28; confirm it needs no change
  - `docs/relocated-storage-recovery.md` — check the "above the minSdk floor"
    claim about the API 30 call still reads correctly
  - `docs/android-api-levels-and-feature-dependencies.md` — §1's declared-levels
    table and §8's "consequences of raising" become past tense; §4's last row
    (the "below the hard floor" defect) is resolved
  - `PROJECT_MAP.md` and `AGENTS.md` must **link** the new document, so it is
    discoverable from the same place as the other Android docs

- **FR-5a** Re-run `./scripts/android-api-scan.sh --floor 28` after the change
  and record its output in the task list. It must report *"no hard (GLOBAL)
  undefined symbol requires more than API 28"* — that is the confirmation that
  28 is a sufficient floor, not merely a higher one. (The same script's
  `--floor 27` run is what produced the finding above.)

- **FR-5b** Keep `docs/android-api-levels-and-feature-dependencies.md` current as
  part of this work, and treat its §9 as a standing rule afterwards: any new JNI
  call site records its API level in §5, and the §2.2 symbol scan is re-run on a
  Qt or NDK change.

- **FR-6** Add a release-notes line: **Play stops offering updates to devices
  below API 28 (Android 8.1). Existing installs on those devices keep the last
  compatible version; they are not uninstalled.** Sideloaded APKs from GitHub
  Releases will likewise refuse to install on API 27.

- **FR-7** Before the first Play upload carrying minSdk 28, read the Play Console
  device/API-level distribution and **record the API-27 install share** in the
  task list. This does not gate the change (the decision is made) but the number
  belongs in the record.

- **FR-8** Build a beta APK (`make android-beta-debug`) and run a smoke pass on
  the arm64 device: app launches, a sutta opens, search returns results, text
  entry works, audio record/playback works. A minSdk floor change should be
  invisible at runtime; this pass exists to confirm that, not to explore.

- **FR-9** Remember `android/version.txt` must be bumped for any Play upload
  (strictly increasing `versionCode`). This PRD does not itself make a release.

### Part B — Enable and measure the AOT QML cache

**The measurement is the deliverable, not the move.** A green build proves
nothing here: unmatched AOT units fail *silently* by falling back to parsing QML
from source, which is exactly today's behaviour.

- **FR-10 (baseline, before any change)** Record a baseline over several **cold**
  runs on Linux desktop:
  - time from `STARTUP-TRACE: engine.load() start` to `engine.load() end`
    (`cpp/sutta_search_window.cpp:18-20`, already instrumented — see
    `docs/startup-sequence-and-caches.md` §6);
  - stripped binary size;
  - `make build -B` wall-clock, since re-enabling AOT re-introduces ~12.4 MB of
    generated C++ to compile.
  Without this the change cannot be evaluated, and the whole task is void.

- **FR-11** `git mv assets/qml/ bridges/assets/qml/`. This is the only accepted
  mechanism. **A repo symlink and `set_current_dir("..")` were both evaluated and
  rejected in the predecessor PRD** — a symlink breaks on Windows without
  `core.symlinks` and makes every QML file visible at two paths to
  `rg`/`qmllint`/`cargo package`; `set_current_dir` breaks incremental builds,
  because cxx-qt-build emits `rerun-if-changed` with the raw path while cargo
  resolves relative rerun paths against the package root.

  The move takes the whole tree, including the subdirectories:
  `assets/qml/com/profoundlabs/simsapa/` (the `qmllint` type stubs and their
  `qmldir`), `assets/qml/data/` (`BojjhangaData.qml` + its own `qmldir`),
  `assets/qml/icons/`, and the 13 `tst_*.qml` test files.

- **FR-12** Update every consumer of the path. Known sites:
  - `bridges/build.rs` — the 96-entry list loses its `../` prefix
  - `Makefile` — `qml-lint` (`qmllint -I ./assets/qml/ ./assets/qml/*.qml`),
    `qml-test` (`qmltestrunner -import … -input …`), the commented single-test
    line, and the two `tokei` exclude lists
  - `build-macos.sh` — `macdeployqt … -qmldir=./assets/qml`
  - `build-appimage.sh` — `QML_SOURCES_PATHS="$QT6_PATH/qml:./assets/qml"`
  - `AGENTS.md` (incl. the "New QML components" rule, the `console`-API exception
    for the stub directory, and the `tst_dialog_loop_harness` instructions),
    `PROJECT_MAP.md`, and the `docs/*.md` files that name the path
  - `.claude/settings.json` if it names the path (grep says it does not — confirm)
  Then grep the whole tree for `assets/qml` and confirm nothing is left pointing
  at the old location.

- **FR-13** The move must be **alias-neutral**, and this is the property the
  whole approach rests on. From `bridges/` the path becomes `assets/qml/Foo.qml`,
  so the resource path stays
  `:/qt/qml/com/profoundlabs/simsapa/assets/qml/Foo.qml` — the ~16 `qrc:` literals
  in `cpp/` and the prefix in `assets/icons.qrc` are untouched. Verify this by
  diffing the generated `.qrc` aliases against the pre-change build's; they must
  be **byte-identical**.

  Note that `assets/icons.qrc` stays where it is: its `<file>` entries resolve
  against `assets/` (they name `icons/32x32/…`, i.e. `assets/icons/`, **not**
  `assets/qml/icons/`), while its `prefix` is the shared QML resource directory.
  Neither half is affected by moving `assets/qml/`. Confirm this rather than
  assuming it.

- **FR-14** Move the QML files from `CxxQtBuilder::qrc_resources(…)` back into
  `QmlModule::new(URI).qml_files(…)` with the now `..`-free paths, and delete the
  alias-derivation block in `bridges/build.rs`. **Its long comment explains the
  runtime bug it exists for — carry that explanation into
  `docs/cxx-qt-fork.md` rather than deleting it with the code.**

- **FR-15 (artifact check, before trusting any runtime result)**
  - The generated `qmldir` component lines must now resolve
    (`Logger 1.0 assets/qml/Logger.qml`).
  - The generated `.qrc` aliases must be unchanged (FR-13).
  - **Every** key in the generated `qmlcache_loader.cpp` must be free of `/../`,
    so it can match a `QDir::cleanPath`-ed lookup.

- **FR-16 (the check the task turns on)** Prove at **runtime** that the cache is
  hit: run with `QT_LOGGING_RULES="qt.qml.diskcache.debug=true"` and/or a
  deliberate-mismatch control (perturb a QML file so the unit *should* be
  rejected, and confirm the log reports it). "It built and started" is not
  evidence. If the cache cannot be shown to be hit, the task fails at this point
  and FR-18 applies.

- **FR-17** Re-measure FR-10's three numbers and compare. **Record the result
  either way**; a null result is a useful finding and closes a question that has
  been open since cxx-qt 0.7.

- **FR-18 (decide)** If the measured win does not justify moving app UI under
  `bridges/` — a genuine design question, since `assets/qml/` is application UI,
  not bridge code — **revert the move** and record the measurement and the
  decision in `docs/cxx-qt-fork.md`, so the next reader does not re-derive it.
  Reverting is an acceptable, complete outcome.

- **FR-19** Whichever way FR-18 goes, `AGENTS.md`'s "New QML components" rule must
  end up describing the path form that is actually in `bridges/build.rs`, and its
  build-time `panic!` message must match. The rule and the code must not
  disagree.

- **FR-20** Re-verify on the platforms available (Linux, macOS, Windows) that the
  build still configures and completes after the path change, and build one
  Android beta APK to confirm QML still loads there (resource paths are unchanged,
  so this is a confirmation, not an investigation). `make qml-lint` and
  `make qml-test` must still run against the moved tree.

### Part C — Report the upstream defects

- **FR-21** Report to KDAB (cxx-qt), with a one-file reproducer — a `qml_files`
  entry written as `"../foo/Bar.qml"`:
  1. a `qml_files` path containing `..` is folded by rcc but **not** by the
     qmldir writer or by qmlcachegen, so the three derivations disagree;
  2. `qmlcache_loader.cpp` cleans the path on **lookup** but not on **insert**,
     so keys containing `/../` are unreachable — which is why the AOT cache has
     never been consulted in this project.

- **FR-22** Report (or record a decision not to) the missing
  `cargo::rerun-if-env-changed=CXX_QT_AUTORCC_OPTIONS` in `cxx-qt-build` — a
  one-line `println!`; changing that variable currently does not trigger a
  rebuild. Optional but cheap; **record the decision either way.**

- **FR-23** Record the issue URLs in `docs/cxx-qt-fork.md` next to the findings
  they document.

---

## 5. Non-Goals (Out of Scope)

1. **Any Qt version change.** All five `QT_*` variables in `CMakeLists.txt` stay
   at 6.9.3. This PRD exists *because* the upgrade is off.
2. **AGP / Gradle wrapper / JDK.** AGP stays 8.6.0, the wrapper stays 8.10, the
   JDK stays pinned 17–21. (Deferred: predecessor tasks 7.2–7.6.)
3. **Packaging measurements** — the 16 KB `max-page-size` link flag and
   `useLegacyPackaging` both stay as they are, unmeasured. (Tasks 7.8, 8.4–8.5.)
4. **Non-arm64 runtime validation.** x86_64 and armeabi-v7a still have never been
   *run*; no such device or emulator is available for this work. (Task 9.10 —
   remains outstanding and must stay documented as such.)
5. **The cxx-qt fork branch** (recreating `simsapa` on upstream `2180c12`,
   patches B/D, deleting the `lipo` dead code). (Tasks 5.1–5.6.)
6. **First runs of the `qt-env-verify` gate on macOS/Windows.** (Tasks 2.13,
   2.14, 2.16.)
7. **The Gboard/Thai mid-word Shift bug.** Both the Qt 6.10.3 route and the
   patched-`QtEditText` route are disproven on device; the new lead
   (`updateSelection` / repeated `showSoftInput` / the `EditorInfo` Qt reports)
   is a separate investigation.
8. **The predictive-back opt-out.** `android:enableOnBackInvokedCallback="false"`
   stays — it is a functional dependency on Qt 6.9.3.
9. **New features.** No QML behaviour changes, no signals, no schema, no
   migrations.

---

## 6. Technical Considerations

### 6.1 Order of work

**Part A → Part C → Part B.** A is small, self-contained and testable in an hour.
C is independent of everything and can be done at any point. B is the large one
and the only one that can end in a revert — it must not be entangled with a
minSdk change in the same commit.

Commit each part separately. Within B, the `git mv` (FR-11/FR-12) and the
`build.rs` switch (FR-14) should be one commit — the build does not work between
them.

### 6.2 Traps, all previously measured

- **`aapt2 dump badging` is the only trustworthy minSdk/targetSdk check.** The
  generated `gradle.properties` carries values `build.gradle` never reads.
- **A green build proves nothing about the AOT cache.** Stale, mismatched or
  simply unmatched units fall back silently to parsing source. FR-16 exists
  because of this.
- **A stale `build/` directory caches `CMAKE_PREFIX_PATH`.** Reconfigure clean
  when measuring; otherwise a "measurement" may be against a cached value.
- **The `qmllint` stub directory moves too.** `assets/qml/com/profoundlabs/simsapa/`
  holds the type stubs for the Rust bridges and their `qmldir`; it is also the one
  directory where the `console` API is allowed. Both facts follow it to the new
  path, in `AGENTS.md` as well as on disk.
- **`assets/qml/data/` has its own `qmldir`.** Implicit same-directory type
  resolution — the reason `Logger.qml` needs no import — must still work after
  the move; this is exactly what broke under cxx-qt 0.8 and is what FR-15's
  `qmldir` check guards.
- **Do not re-open the NDK question.** The r28 exclusion is about API 30, not
  about 27 vs 28.

### 6.3 Why minSdk 28 is safe in code (the analysis behind Part A)

Summarised here; the full measurement, the reproducible commands and the
per-feature inventory are in
[docs/android-api-levels-and-feature-dependencies.md](../docs/android-api-levels-and-feature-dependencies.md).

| Surface | Floor it actually needs | Affected by 27 → 28? |
|---|---|---|
| **`libQt6Core` → `getentropy` (GLOBAL undef)** | **API 28 — hard, app fails to `dlopen`** | Yes — this is the defect being fixed |
| cpal → AAudio (`libaaudio.so`, hard `NEEDED`) | API 26 | No |
| Storage volume enumeration (`cpp/utils.cpp`) | API 24 | No |
| Storage Access Framework (`android_saf.rs`) | API 21 | No |
| `Build.VERSION`-gated code in `cpp/`, `bridges/`, `backend/` | none exists | No |
| Rust weak-linked `getrandom` (28), `memfd_create` (30), `copy_file_range` (34) | none — WEAK, with fallbacks | No |
| `WRITE_EXTERNAL_STORAGE` (already dropped) | required on API 27–28 | No — nothing writes shared storage |
| NDK r28 exclusion | bionic API 30 | No |
| Qt 6.9.3 declared floor / build platform | **28** | Yes — the override disappears |
| Qt 6.11 declared floor | 28 (unchanged) | Not a reason to wait |

The earlier framing — "no technical risk, the only cost is distribution, so there
is no benefit on its own" — is what coupled this change to the Qt upgrade. **That
framing was wrong**, and only because nobody had scanned the binaries: the app is
currently offered to API 27 devices on which it cannot start. The change is a
correctness fix, and Android 8.1 users lose nothing they had.

---

## 7. Success Metrics

1. `aapt2 dump badging` on a release artifact reports `minSdkVersion:'28'` and
   `targetSdkVersion:'36'`, and the arm64 smoke pass (FR-8) is clean.
2. No file in the tree claims the app's minSdk is 27.
3. The AOT question is **closed with a number**: baseline and post-change
   `engine.load()` times, binary size and build time, plus runtime proof of a
   cache hit (or proof that it still misses). Either outcome counts, provided
   `docs/cxx-qt-fork.md` records it.
4. If the move is kept: `make build -B`, `make qml-lint`, `make qml-test`, an
   AppImage build and an Android beta APK all succeed against the new layout, and
   the generated `.qrc` aliases are byte-identical to the pre-change build's.
5. Two upstream issue URLs (or a recorded decision not to file) exist in
   `docs/cxx-qt-fork.md`.
6. `git diff` shows **no change** to any `QT_*` variable in `CMakeLists.txt`,
   to `android/build.gradle`'s AGP line, or to the Gradle wrapper.

---

## 8. Open Questions

1. **What threshold makes the AOT move worth keeping?** Proposal: keep it if the
   cold `engine.load()` improves by ≥ 100 ms *and* the binary-size and
   build-time regressions from ~12.4 MB of regenerated C++ are judged acceptable;
   otherwise revert. To be agreed before FR-17 is measured, so the decision is
   not made after seeing the number.
2. **Does the Android build benefit at all?** `qmlcachegen` runs per target; the
   Android startup path is dominated by other costs. Worth one measurement on
   device if the desktop result is positive.
3. **What is the Play API-27 install share?** (FR-7.) Not a gate, but it belongs
   in the record and may change the release-notes wording.
4. **Does app UI belong under `bridges/`?** The predecessor PRD flagged this as a
   genuine design question and explicitly allowed "no" as the answer. If the
   measured win is real but the layout is objectionable, the fallback is to keep
   the current `qrc_resources` arrangement and report the upstream defects
   (Part C) as the only outcome.

---

## 9. Implementation Surface

**Reuse as-is:** the `STARTUP-TRACE` instrumentation around `engine.load()`; the
`aapt2 dump badging` verification recipe in
`docs/android-qt-upgrade-considerations.md` §5; the beta-APK install flow in
`docs/android-beta-distribution-and-play-policy.md`.

**Adapt:** `bridges/build.rs` (the `qml_files` list and the `qrc_resources` alias
block); `Makefile` `qml-lint` / `qml-test` / `tokei` targets; `build-macos.sh`
and `build-appimage.sh` QML path arguments; `android/build.gradle` `defaultConfig`.

**Genuinely new:** the baseline/after measurement record; the runtime
cache-hit proof (FR-16); the two upstream issue reports.

**Explicitly not touched:** any `QT_*` variable, AGP, the Gradle wrapper, the
JDK pin, the NDK pin, `useLegacyPackaging`, the 16 KB link flag, the
predictive-back opt-out, `assets/icons.qrc`, and the ~16 `qrc:` literals in
`cpp/` (which the alias-neutrality requirement, FR-13, exists to protect).
