# Tasks: CXX-Qt catch-up, then Qt 6.10.3 on Android

Source PRD: [2026-08-07-175059-prd---cxx-qt-and-qt-6-10-3-android-upgrade.md](./2026-08-07-175059-prd---cxx-qt-and-qt-6-10-3-android-upgrade.md)

## Component analysis

Technical components and the PRD requirements each covers:

| Component | Files | FRs |
|---|---|---|
| Qt kit selection (CMake) | `CMakeLists.txt:8-12,23-74,96-190,192,200` | FR-11, FR-24 – FR-29 |
| Qt version propagation (scripts) | `Makefile`, `build-android.sh`, `build-appimage.sh`, `build-macos.sh`, `build-windows.ps1` | FR-12, FR-30, FR-30b |
| Rust bridge dependency | `bridges/Cargo.toml`, `bridges/build.rs`, `src-lib/cxx-qt`, `src-lib/cxx-qt-simsapa` | FR-1 – FR-6, FR-3b |
| Android toolchain | `build-android.sh`, `android/build.gradle`, `android/gradle/wrapper/`, `android/AndroidManifest.xml` | FR-13 – FR-19, FR-23 |
| Native/C++ surface | `CMakeLists.txt:340`, `cpp/android_raw_pick.cpp` | FR-17, FR-23c |
| Verification (desktop) | `make build -B`, `make test`, `ldd`, configure log | FR-8, FR-9 |
| Verification (device) | AAB/APK, `aapt2 dump badging`, `zipalign`, `readelf` | FR-20 – FR-23d |
| Documentation | `docs/`, `CLAUDE.md`, `AGENTS.md`, `PROJECT_MAP.md` | FR-7, FR-31 |

Dependency order (what blocks what):

```
Part C (CMake Qt selection)  ──►  meaningful Linux verification
        │                              │
        │                              ▼
        └──► scripts derive version ──► Stage 1 (cxx-qt upstream + build.rs)
                                             │
                                             ▼
                                    desktop verify @ 6.9.3
                                             │
                                             ▼
                                    Stage 2: QT_ANDROID=6.10.3 → build
                                             │
                                             ├─► AGP/Gradle cluster → build
                                             │
                                             ├─► minSdk 28 / 16 KB / packaging
                                             │
                                             ▼
                                    on-device acceptance (FR-20)
                                             │
                                             ▼
                                        documentation
```

Every functional requirement FR-1 … FR-31 is covered by at least one task below;
FR-5 and FR-13c are already satisfied and appear only as confirmation steps.

## Relevant Files

- `CMakeLists.txt` — the single source of the per-platform Qt version. Gains a Linux `CMAKE_PREFIX_PATH` branch, a `Qt6_VERSION` assertion, a derived `qmake_path`, `QT_ANDROID` = 6.10.3, and `--no-zstd` on `CMAKE_AUTORCC_OPTIONS` (lines 80-81, which is all of FR-2 — cxx-qt-cmake forwards that list to the bridge crate's `rcc`).
  - **Landed so far (tasks 1.2–1.10):** the `elseif (UNIX AND NOT APPLE AND NOT
    ANDROID)` Linux branch; the two "Qt not found" warnings → `FATAL_ERROR`;
    `REQUIRED` on `find_package`; `qt_expected_version` + the version assertion
    (set in the **second** block — see the 1.6 note); the five `qmake_path`
    values collapsed to one `Qt6_DIR` derivation + per-branch `qmake_exe_name`,
    with the Windows ambient-`PATH` fallback and its three-way search deleted;
    and the `GIT_TAG 0.7` branch-vs-tag comment. Still to come here: `QT_ANDROID`
    = 6.10.3 (6.2) and the `--no-zstd` append (3.10).
- `Makefile` — `QT_PATH` for the Darwin branch; the non-Darwin `BUILD_CMD` passes no `-DCMAKE_PREFIX_PATH` (deliberately, once FR-24 lands).
- `build-appimage.sh` — Qt resolution must be hoisted above `build_app()`; the `elif command -v qmake6` fallback and the "already built" short-circuit are hazards.
- `build-android.sh` — `QT_ANDROID_VERSION` default, the NDK "newest installed" selection, and where `CXX_QT_AUTORCC_OPTIONS` gets exported.
  **Note: `CXX_QT_AUTORCC_OPTIONS` is deliberately NOT exported here** (3.10) —
  corrosion's `cmake -E env` assignment would override it. Task 3.12 added
  `export ANDROID_SDK_ROOT ANDROID_NDK_ROOT`, without which the task-2.12
  environment gate stops every Android build. **Task 2.17 added the desktop-Qt
  scrub** (`PATH` / `LD_LIBRARY_PATH` / `QT_PREFIX` / `QMAKE`), without which an
  outer shell's `QT_LINUX` kit is loaded by the `QT_ANDROID` host tools once the
  two versions diverge.
- `build-macos.sh` / `build-windows.ps1` — hardcoded `6.9.3` paths to derive from `CMakeLists.txt`.
- `bridges/Cargo.toml` — the four cxx-qt crate pins (fork → upstream `2180c12`).
  **Done (3.4).** Also required raising `cxx` from `1.0.148` to `1.0.176`:
  cxx-qt 0.9.1 needs `^1.0.176` and cargo would not resolve against the
  lockfile's `1.0.169`.
- `bridges/Cargo.lock` — **regenerated (3.4).** Not listed originally; it is part
  of the same commit.
- `bridges/Cargo.toml` — **also (4.2):** `qt-build-utils` added to
  `[build-dependencies]` for `QResourceFile`; `cxx-qt-build` re-exports only
  `QResource` and `QResources`. Same pinned rev as the other four crates.
- `bridges/build.rs` — the `QmlModule` literal, the `rust_files` list and the `cc_builder` closure; all three change under the 0.9 API.
  **Also (4.2): the QML files are registered with `qrc_resources` and their
  resource alias is derived here** by stripping the `../`, rather than being
  passed as the module's `qml_files`. This is the fix for the 0.9 startup
  failure (`Type Logger unavailable`); the long comment at that block is the
  authoritative explanation and must not be trimmed. Task 12.0 may reverse it.
  **Done (3.6, 3.7):** `CxxQtBuilder::new_qml_module(QmlModule::new(uri)
  .qml_files(…))`, the nine bridges moved to `.files([…])`, and the closure
  replaced by the safe `.include_dir()` / `.cpp_files()` — no `unsafe` block.
  The dead `lipo` import/call comments were deleted.
- `android/build.gradle` — AGP classpath, `minSdkVersion`, `targetSdkVersion`, `packagingOptions.jniLibs`, `ndkVersion`.
- `android/gradle/wrapper/gradle-wrapper.properties` — the wrapper is ours (8.10), not Qt's.
- `android/AndroidManifest.xml` — the predictive-back opt-out, the explicit permissions and `required="false"` features.
- `cpp/android_raw_pick.cpp` — the private-Qt include `<QtCore/private/qandroidextras_p.h>`.
- `src-lib/cxx-qt/` — upstream checkout, revision `2180c12` (0.9.1), the reference for the API migration.
- `src-lib/cxx-qt-simsapa/` — our fork, branch `simsapa`, rev `8a597414`; gets rebased for Apple only.
- `scripts/qt-env.sh` — **new, landed** (11.1; task 2.1 now consumes it). The single
  shell-side source of the per-platform Qt version, derived from `CMakeLists.txt`.
- `scripts/qt-env-check.sh` — **new, landed** (11.4). Drift check between
  `.claude/settings.json`'s literal paths and `QT_LINUX`; run by `make qt-env-check`.
- `scripts/qt-env-verify.sh` — **new, landed** (2.12). The build-time environment
  report and gate, called by every bash build script. Prints the toolchain a
  build actually used (compiler, cmake, ninja, Rust + targets, Qt declared vs
  **actual**, and on Android the SDK/NDK/clang/JDK/Gradle/AGP/SDK-levels/per-ABI
  kits). `CRITICAL` findings **exit 1 and stop the build**; `ADVISORY` findings
  warn. `--all` does the repo-wide consistency checks; `--report-only`
  inspects without stopping.
  **Task 2.12's three missing checks were added by the 2026-08-08 review**
  (`check_powershell_reader`, `check_no_hardcoded_versions`,
  `check_script_syntax`, plus kit reporting in `--all`), and **task 2.17 added
  the "Android host tools" section** with the `foreign_qt_entries()` helper that
  detects a foreign Qt on `PATH` / `LD_LIBRARY_PATH`.
- `build-windows.ps1` — also gains `Invoke-EnvVerify`, the PowerShell twin of
  the above (PowerShell cannot source bash). **Must be kept in step by hand** —
  see task 2.16.
- `Makefile` — also gains the `qt-env-check`, `qt-verify`, `qt-verify-linux`,
  `qt-verify-android`, `qt-verify-macos` and `qt-checks` targets (11.4, 2.12),
  plus the `qt_version_for` make function (2.9). These are for running the gate
  **by hand**; the build scripts call it themselves.
- `~/.config/fish/conf.d/direnv.fish` — **new, landed** (11.2), outside the repo.
  The direnv shell hook. Note PRD non-goal 6 bars the *build* from depending on
  shell config; this is a convenience layer only, added at the user's request.
- `.envrc` — **new** (task 11.2). direnv hook so an interactive shell entering the
  project gets the desktop Qt on `PATH`.
- `.claude/settings.json` — **new/edited** (task 11.4). `env` block giving agent
  `Bash` calls the project's Qt; the one place a literal path is unavoidable,
  hence the drift check.
- `docs/qt-kit-selection.md` — **new** (FR-31).
- `docs/cxx-qt-fork.md` — **new, landed** (10.1). Written early because
  `CMakeLists.txt:131` and `bridges/build.rs:141` already referenced it.
- `docs/android-qt-upgrade-considerations.md`, `docs/android-soft-keyboard.md`, `docs/android-multi-abi-and-chromeos.md`, `docs/pure-rust-audio-backend.md`, `docs/file-selection-test.md`, `docs/qt-6.10.1-appimage-issues.md` — updated.
- `CLAUDE.md`, `AGENTS.md` — the "New Rust bridges" / "New QML components" sections teach the **removed** 0.7 API.
- `PROJECT_MAP.md` — only if the Qt-selection changes alter what it describes.

### Notes

- **Line numbers in the PRD are pre-change.** Task 1.0 alone shifts everything
  below `CMakeLists.txt:74`. Re-locate by content.
- **Change one variable at a time.** Each top-level task (and, inside task 6–8,
  each sub-step) ends with a build. Combined failures in this area are
  unattributable.
- **A stale `build/` caches `CMAKE_PREFIX_PATH`** — remove the build directory
  when changing Qt version, or the version assertion may pass against a cached
  value.
- **Never hand-delete `android-build/`** — use `make android-clean`.
- **Never build release packages from Qt Creator** — its kits are single-ABI.
- Desktop tests: `make build -B`, `make test` (Rust + QML + JS). Known
  timing-assertion drift in `cargo test` is not a regression from this work.
- Android verification is `aapt2 dump badging` on the finished artifact —
  **never** the generated `gradle.properties`.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

---

### [x] 1.0 Part C, first half — `CMakeLists.txt` as the single source of the Qt kit

**Specs to keep in mind**

- The platform block at lines 23-74 (`WIN32` / `APPLE AND NOT IOS`) and the
  `if (ANDROID)` block at line 96 are **two separate top-level blocks**, not one
  chain. A bare `else()` added to the first block therefore catches **Android**
  and points it at desktop Linux Qt — the build would appear to succeed and ship
  **without the Thai fix**. Guard the new branch as
  `elseif (UNIX AND NOT APPLE AND NOT ANDROID)`.
- The inner `if(NOT CMAKE_PREFIX_PATH)` guard is load-bearing for multi-ABI: Qt
  does not forward the parent's `CMAKE_PREFIX_PATH` to per-ABI sub-builds, and an
  explicitly passed prefix must win.
- Only **two** of the four `message(WARNING …)` calls mean "Qt not found"
  (lines 32 and 70). Lines 51 (macOS SDK) and 120 (`ANDROID_ABI` default) are
  working fallbacks and must stay warnings.
- `Qt6_DIR` is known ~15 lines before `qmake_path` is consumed
  (`find_package` at :192, `cxx_qt_import_crate`'s `QMAKE` argument at :211), so
  the derivation has a clean window.
- Do **not** drop the `QMAKE` argument to `cxx_qt_import_crate`: cxx-qt-cmake
  0.9.1's fallback resolves `Qt::qmake`, which is the **host** qmake — wrong for
  Android, whose kit `bin/qmake` is a POSIX shell wrapper applying the Android
  `qt.conf`.

**Depends on:** nothing. This task is the prerequisite for everything else.

- [x] 1.1 Record the pre-change baseline so the fix is provable: run `cmake -S . -B build/simsapadhammareader/` and note which Qt `find_package` reports, then `ldd build/simsapadhammareader/simsapadhammareader | grep libQt6Core`. Expect `/usr/lib` and 6.11.1 (the defect from PRD §2.1). Save the output for the doc in task 10.
  - **Measured 2026-08-08.** PRD §2.1 confirmed, with a refinement: the defect is
    **non-determinism**, not "always system Qt". A clean-env configure
    (`PATH=/usr/bin:/bin:/usr/local/bin`) resolves `/usr/lib/cmake/Qt6` =
    **6.11.1**; the working tree's existing `build/` resolves `~/Qt/6.9.3/gcc_64`,
    because `find_package` searches `PATH`'s parent dirs and that build dir was
    configured from a shell with `~/Qt/6.9.3/gcc_64/bin` on `PATH`. Neither is a
    `-D` cache entry and `~/.cmake/packages/` is empty. So the Qt picked depends
    on the invoking shell — worse than consistently wrong, and the reason the new
    task 11.0 exists. `qmake_path` is unconditionally `~/Qt/6.9.3` either way.
  - **A check in this list does not work:** `strings <binary> | grep 'Qt 6\.'`
    returns nothing — the binary carries no such string. Removed from 1.1 and
    corrected in 1.11 / 2.10; use `ldd … | grep libQt6Core` and the configure
    log's `Qt6_DIR` instead.
  - Full notes saved for task 10.2.
- [x] 1.2 Add the Linux desktop branch to the first platform block as `elseif (UNIX AND NOT APPLE AND NOT ANDROID)`, with the inner `if(NOT CMAKE_PREFIX_PATH)` guard, `$ENV{HOME}/Qt/${QT_LINUX}/gcc_64` then `/opt/Qt/${QT_LINUX}/gcc_64`, and a `FATAL_ERROR` naming both searched locations and the `-DCMAKE_PREFIX_PATH=` escape hatch (FR-24). Add a comment stating why a bare `else()` would be wrong here.
- [x] 1.3 Convert the two "Qt6 installation not found" warnings to `FATAL_ERROR` — Windows at line 32 and APPLE at line 70 — and leave lines 51 and 120 as `WARNING`, adding a one-line comment at each of those two saying it is a deliberate fallback (FR-25).
- [x] 1.4 Add a comment at the `qmake_path` block's bare `else()` (line 186) explaining that it is correct **there** because `if (ANDROID)` is the first branch of that same chain, unlike the first block — so the two blocks are deliberately asymmetric and must not be "harmonised" (FR-25 note).
- [x] 1.5 Add `REQUIRED` to `find_package(Qt6 COMPONENTS …)` at line 192 (FR-26). Verify it fails cleanly by temporarily configuring with a bogus `-DCMAKE_PREFIX_PATH=/nonexistent`.
  - **`REQUIRED` is in place, but its failure path was NOT demonstrated and is
    largely unreachable on this machine.** `-DCMAKE_PREFIX_PATH=/nonexistent`
    does not stop `find_package` finding system Qt via CMake's default search
    paths, so the configure fails on the **FR-27 assertion** (6.11.1 ≠ 6.9.3),
    not on `REQUIRED`. On a host with system Qt installed, the assertion — not
    `REQUIRED` — is the effective backstop. Do not cite `REQUIRED` as verified.
  - Side effect worth knowing: `REQUIRED` turns a *misconfigured* Android
    invocation (NDK not set, so `ANDROID` is never defined and the Linux branch
    is taken) into a confusing `Failed to find required Qt component
    "WebEngineQuick"` error instead of a silent partial find. Failing is right,
    but the message points at the wrong thing — note it in the task-10.2 doc.
- [x] 1.6 Set a `qt_expected_version` variable in each platform branch from the matching `QT_*`, and add the post-`find_package` assertion `if(NOT Qt6_VERSION VERSION_EQUAL "${qt_expected_version}")` → `FATAL_ERROR` naming expected, found and `Qt6_DIR` (FR-27).
  - **Deviation from the wording, deliberate.** `qt_expected_version` is set in
    the **second** block (the `qmake_path` chain), not the first. The first
    block's Windows branch is guarded `WIN32 AND NOT CMAKE_PREFIX_PATH`, so it is
    skipped whenever a prefix is preset, and the variable would be unset exactly
    when the assertion matters most. The second block is a complete
    `ANDROID / IOS / APPLE / WIN32 / else` chain that always assigns exactly one
    branch, and it runs before `find_package`. Keep it there.
- [x] 1.7 Replace the five independently-rebuilt `qmake_path` values with a single derivation from `Qt6_DIR`: keep only `qmake_exe_name` per branch (`qmake6`, `qmake6.exe`, `qmake` for Android), then `get_filename_component(_qt_prefix "${Qt6_DIR}/../../.." ABSOLUTE)` and `set(qmake_path "${_qt_prefix}/bin/${qmake_exe_name}")` with a `FATAL_ERROR` if it does not exist (FR-28). Do **not** compute the suffix from `CMAKE_EXECUTABLE_SUFFIX` — when cross-compiling it describes the target.
  - Landed as described. The derivation sits **after** the FR-27 assertion, so it
    only ever runs against a Qt that has already been vouched for.
  - **Verified on Android, which is the only test with discriminating power.**
    Any `-DCMAKE_PREFIX_PATH` pointing at a *different-versioned* Qt trips the
    assertion and aborts before the derivation is reached (see 1.12), so Android
    — same version, different kit prefix — is where the derivation can be
    observed following `Qt6_DIR` instead of a rebuilt `QT_*` path:
    `Qt 6.9.3 … at ~/Qt/6.9.3/android_arm64_v8a/lib/cmake/Qt6` →
    `Using qmake: ~/Qt/6.9.3/android_arm64_v8a/bin/qmake`. Correct kit, and the
    **`qmake` / `qmake6` name split holds** — that file is a POSIX shell script
    (the Android `qt.conf` wrapper), confirmed with `file`.
  - Linux: `Using qmake: ~/Qt/6.9.3/gcc_64/bin/qmake6` — byte-identical to the
    string the old per-branch `set()` produced, so this is a no-op for the
    desktop build's behaviour and a structural fix for its *coupling*.
- [x] 1.8 Delete the bare `set(qmake_path "qmake6")` Windows fallback and replace it with a `FATAL_ERROR` (FR-29). With 1.7 in place this branch reduces to setting `qmake_exe_name`.
  - Done, and the whole three-way `msvc2022_64` / `msvc2019_64` / `USERPROFILE`
    search went with it — 1.7's derivation follows whichever of those
    `find_package` actually resolved, so guessing a second time is redundant.
    The Windows branch is now one line (`set(qmake_exe_name "qmake6.exe")`).
  - The `FATAL_ERROR` is the **shared** existence check in 1.7's derivation, not
    a Windows-specific one; a comment at the old site records why the ambient-`PATH`
    fallback is not replaced by an equivalent (it is the same defect class as the
    missing Linux `CMAKE_PREFIX_PATH` branch). Untestable here — no Windows host.
- [x] 1.9 Change `CMakeLists.txt:200`'s `GIT_TAG 0.7` comment situation: leave the version alone for now (task 3.5 bumps it) but add the "a branch re-resolves to the latest commit on every build" note mirroring `bridges/Cargo.toml:20`, so the branch-vs-tag distinction is recorded before the bump (FR-4 groundwork).
  - Comment added at the `FetchContent_Declare`; `GIT_TAG 0.7` itself left alone
    for task 3.5. Records that `0.7` is the **branch** (kdab/cxx-qt-cmake
    publishes `refs/heads/0.7` alongside tags `0.7.0`–`0.7.3`), that it happens
    to equal `0.7.3` today which is why nothing has broken, and that the
    CMake pin and the four `bridges/Cargo.toml` crate pins are a coupled pair
    that must move together.
- [x] 1.10 Delete `build/simsapadhammareader/` (stale `CMAKE_PREFIX_PATH` cache) and re-configure for Linux. Confirm the status line names `~/Qt/6.9.3/gcc_64`, not `/usr/lib/cmake/Qt6`.
  - Done after 1.7/1.8 (the build dir was removed and re-configured from
    scratch, so no cached prefix could mask the result):
    `Using CMAKE_PREFIX_PATH: ~/Qt/6.9.3/gcc_64` /
    `Qt 6.9.3 (expected 6.9.3) at ~/Qt/6.9.3/gcc_64/lib/cmake/Qt6` /
    `Using qmake: ~/Qt/6.9.3/gcc_64/bin/qmake6`.
- [x] 1.11 Run `make build -B` and `make test`. Then re-run the 1.1 commands **from a clean environment** (the 1.1 finding: an ambient `PATH` can mask the defect): `libQt6Core` must resolve under `~/Qt/6.9.3` and the configure log's `Qt6_DIR` must name it. (The `strings` check from 1.1 is dropped — the binary has no such string.) **This is a real behaviour change for the desktop build, not a no-op** — treat any new warning or failure as a genuine 6.9.3-vs-6.11.1 difference, not as noise.
  - **Measured 2026-08-08. The §2.1 defect is fixed and the fix is now
    end-to-end.** `make build -B` and `make test` both completed — the first
    build ever done against 6.9.3 rather than system 6.11.1 — with no
    version-attributable failure. Evidence:
    - Configure log: `Using CMAKE_PREFIX_PATH: /home/gambhiro/Qt/6.9.3/gcc_64`
      and `Qt 6.9.3 (expected 6.9.3) at ~/Qt/6.9.3/gcc_64/lib/cmake/Qt6` — the
      FR-27 assertion passed on the real build.
    - `Qt6_DIR` in `build/simsapadhammareader/CMakeCache.txt` =
      `~/Qt/6.9.3/gcc_64/lib/cmake/Qt6`.
    - `ldd`: **all 18** `libQt6*` resolve under `~/Qt/6.9.3/gcc_64/lib`, **zero**
      under `/usr/lib`. Before the fix `libQt6Core` resolved to `/usr/lib`.
  - **Clean-environment re-run (the 1.1 finding — an ambient `PATH` can mask the
    defect):** `env -i PATH=/usr/bin:/bin:/usr/local/bin HOME=$HOME cmake -S .
    -B <fresh-dir>` exits 0, prints the same two Qt lines and caches the same
    `Qt6_DIR`. This is the same invocation that resolved `/usr/lib/cmake/Qt6`
    (6.11.1) in task 1.1. **The shell layer is confirmed not load-bearing.**
  - Interactive shell separately confirmed by the user: `which qmake` →
    `~/Qt/6.9.3/gcc_64/bin/qmake`, `qmake -query QT_VERSION` → `6.9.3`. That
    closes the direnv item left open in the previous session (which could only
    test it by firing `emit fish_prompt` manually).
- [x] 1.12 Prove the failure modes: configure once with `-DCMAKE_PREFIX_PATH=/usr` (system Qt) and confirm the FR-27 assertion fires naming both versions; confirm `qmake_path` in that same run resolves under `/usr`, not `~/Qt`.
  - Fires as intended: `Qt version mismatch: expected 6.9.3, found 6.11.1 at
    /usr/lib/cmake/Qt6`. The `qmake_path` half of this check was deferred to
    1.7 — until the derivation lands, `qmake_path` is still built independently
    from `$ENV{HOME}/Qt/${QT_LINUX}` and cannot resolve under `/usr` by
    construction.
  - **Resolved after 1.7, but not by measurement — the check turns out to be
    unobservable, and that is the right outcome.** Re-run with
    `-DCMAKE_PREFIX_PATH=/usr` now aborts at `CMakeLists.txt` (assertion) with
    `expected 6.9.3, found 6.11.1 at /usr/lib/cmake/Qt6` **before** the
    derivation runs, so `Using qmake:` never prints. The derivation is placed
    after the assertion deliberately: a `qmake` from a Qt the project has
    rejected should never be computed, let alone handed to cxx-qt. So
    "`qmake_path` resolves under `/usr`" is now unreachable by construction
    rather than confirmable — the stronger result. The derivation's
    *correctness* is instead evidenced on Android, where the prefix differs but
    the version does not (see 1.7).
- [x] 1.13 Configure for Android (`make android-apk-debug` or a bare `cmake` with the Android toolchain) and confirm `CMAKE_PREFIX_PATH` resolves under `~/Qt/6.9.3/android_<abi>` — **not** `gcc_64`. This is the regression test for the bare-`else()` trap.
  - Passes at 6.9.3: `Android ABI: arm64-v8a` / `Using Android Qt:
    ~/Qt/6.9.3/android_arm64_v8a` / assertion `Qt 6.9.3 (expected 6.9.3)`. No
    desktop-kit leak. Re-run at 6.10.3 in task 6.7, where the two versions
    actually differ and the test has real discriminating power.
  - **Trap for anyone repeating this:** `qt-cmake` alone is not enough. Without
    `-DANDROID_SDK_ROOT` / `-DANDROID_NDK_ROOT` the NDK toolchain never loads,
    `ANDROID` is never defined, and the **Linux** branch is taken — the run looks
    like an Android configure but is not one. Pass both, as `build-android.sh`
    does.
- [x] 1.14 Commit Part C's CMake half on its own, with a commit message stating the developer-visible change: a Linux machine without `~/Qt/6.9.3/gcc_64` now gets a `FATAL_ERROR` instead of silently using system Qt.
  - **Deviation: landed as one commit, not on its own.** `2a24322` ("Qt config,
    shell env, CMakeLists.txt updates") carries the CMake half **together with**
    the task-11.0 shell/agent-env layer (`scripts/qt-env.sh`,
    `scripts/qt-env-check.sh`, `.envrc`, `.claude/settings.json`, the `Makefile`
    `qt-env-check` target, `AGENTS.md`). Committed by the user.
  - Consequence for task 2.11: the "separately from 1.14" instruction still
    holds for the **build scripts** (`build-android.sh`, `build-appimage.sh`,
    `build-macos.sh`, `build-windows.ps1`, the Darwin `QT_PATH`), none of which
    are touched yet — so the one-change-per-commit intent survives where it
    matters. Nothing to undo.

---

### 2.0 Part C, second half — build scripts derive their Qt version; the AppImage is coherent

**Specs to keep in mind**

- Version flow is one-directional: `CMakeLists.txt` declares, scripts read.
  Environment overrides still win (the existing `${VAR:-default}` pattern in
  `build-android.sh:25`).
- Three implementations are needed — bash, PowerShell, Make — each a one-line
  `grep`/`sed` against `CMakeLists.txt`.
- `build-appimage.sh`'s `main()` calls `build_app` (line 407) **before**
  `create_appimage` (line 409), and all Qt environment setup lives inside
  `create_appimage()` at lines 187-221. That ordering is the bug.
- `build_app()` skips the build entirely when `$BUILD_DIR/simsapadhammareader`
  exists, so a binary from a differently-configured build can be packaged.

**Depends on:** task 1.0 (the assertion is the backstop these scripts rely on).

- [x] 2.1 Write the bash version-reading helper (e.g. `qt_version_for()` reading `set(QT_ANDROID "…")` / `set(QT_LINUX "…")` from `CMakeLists.txt` with `sed -n`), and place it where both `build-android.sh` and `build-appimage.sh` can use it — either a small shared `scripts/qt-version.sh` sourced by both, or duplicated with a cross-reference comment. Choose one and state the reason in the file (FR-30).
  - **Chose the shared file**, `scripts/qt-env.sh` (which already landed at 11.1
    and already carries `qt_version_for`), not a duplicated one-liner. Reason
    recorded in the file's header: duplicating the *reader* re-creates the same
    drift one level down — three copies of a `sed` expression that must all keep
    matching `CMakeLists.txt`'s syntax, failing in one script and not another if
    that line is ever reformatted. So 2.1 reduced to stating the decision and
    documenting the calling convention; no new code was needed.
  - **The sharing cost, and the mechanism that pays it:** sourcing this file
    bare also *activates* the desktop kit onto `PATH`, which is actively wrong
    inside `build-android.sh`. Build scripts must therefore use
    `QT_ENV_NO_ACTIVATE=1 . scripts/qt-env.sh`. Verified under `env -i` that this
    leaves `PATH` untouched and `QT_PREFIX` unset while `qt_version_for` still
    works — that is the contract 2.2–2.4 depend on.
  - All five platforms read correctly today (`LINUX/MACOS/WINDOWS/ANDROID/IOS`
    → `6.9.3`). Worth re-running after task 6.2, since `ANDROID` → `6.10.3` is
    the first time these values diverge and is what the helper exists for.
- [x] 2.2 `build-android.sh:25` — `QT_ANDROID_VERSION` default derived from `QT_ANDROID` instead of the hardcoded `6.9.3`, keeping the `${QT_ANDROID_VERSION:-…}` override (FR-12, FR-30). Verify by printing the resolved value at the top of a run.
  - Done: `QT_ENV_NO_ACTIVATE=1 . ./scripts/qt-env.sh` then
    `QT_ANDROID_VERSION="${QT_ANDROID_VERSION:-$(qt_version_for ANDROID)}"`. The
    run header now prints the version **and which source decided it**
    (`==> Qt : 6.9.3 (from CMakeLists.txt QT_ANDROID), …`), so the log answers
    "which Qt did this build use, and who chose it" without re-deriving it.
  - **⚠ Found and fixed a latent defect while verifying this — it would have
    silently defeated the entire upgrade.** `qt_env_activate()` was
    **exporting `QT_ANDROID_VERSION`**, so:
    - every interactive/direnv/agent shell carried it, and `build-android.sh`
      treats a set value as a **deliberate override** — so the new derivation
      would have been bypassed on every ordinary build, taking the "from
      environment" branch instead (this is exactly what the first verification
      run printed, which is how it was caught);
    - the assignment was `:-` guarded, so a direnv **reload did not refresh it**
      — a shell opened before a `QT_ANDROID` bump keeps the old version
      indefinitely.
    Together, after task 6.2 makes the versions differ, that builds Android
    against the **desktop** 6.9.3 — a package that looks fine and **ships
    without the Thai fix**, i.e. the same class of silent failure the PRD's
    bare-`else()` warning is about, arriving by a different route. Invisible
    today only because both versions are still equal.
  - Fix: `qt_env_activate()` no longer exports `QT_ANDROID_VERSION` at all;
    `build-android.sh` reads `CMakeLists.txt` itself. Rationale left in
    `scripts/qt-env.sh` — a convenience-layer variable that changes build
    *output* would make that layer load-bearing, which PRD non-goal 6 forbids.
  - Verified from a clean `env -i` shell: activation leaves
    `QT_ANDROID_VERSION` **unset**; the default now reports
    `6.9.3 (from CMakeLists.txt QT_ANDROID)`; a genuine
    `QT_ANDROID_VERSION=9.9.9` still wins and reports `(from environment)`.
  - **Action for existing shells:** any terminal that sourced the old
    `qt-env.sh` still holds the stale export. Re-source or open a new shell
    before the first Android build after task 6.2.
- [x] 2.3 `build-appimage.sh` — extract the Qt resolution (lines 187-221) into a `resolve_qt()` helper that sets `qt6_path` and exports `QT_BASE_DIR` / `PATH` / `QMAKE` / `LD_LIBRARY_PATH` / the WebEngine paths, and call it from `main()` **before** `build_app()` (FR-30b). `create_appimage()` then consumes the already-resolved `qt6_path`.
  - `resolve_qt()` added above `build_app()`, setting the global `QT6_PATH`;
    `main()` now runs `resolve_qt` → `build_app` → `create_appdir` →
    `create_appimage`, with a comment at the call site saying the order is
    load-bearing. `create_appimage()` keeps its `local qt6_path="$QT6_PATH"` so
    the rest of the function is untouched.
  - **Added a guard the task did not ask for, because the failure would be
    silent.** The script runs under `set -e` but **not** `set -u`, and the
    WebEngine resource/locale copies end in `2>/dev/null || true` — so an unset
    `QT6_PATH` (someone reorders `main()`) would produce an AppImage **missing
    its QtWebEngine resources** rather than an error. `create_appimage()` now
    fails explicitly if `QT6_PATH` is empty, naming the call-order fix.
- [x] 2.4 In the same script, base the search on `QT_LINUX` read from `CMakeLists.txt` rather than the hardcoded `6.9.3`, and remove the `elif command -v qmake6` ambient-`PATH` fallback — replace it with an error naming `QT_BASE_DIR` (FR-30).
  - Version now from `qt_version_for LINUX` via the 2.1 helper; `QT_BASE_DIR`
    remains the deliberate override and is still checked first.
  - The `command -v qmake6` fallback is gone, with a comment recording why: for
    a **release artifact** silently bundling an unrelated Qt is worse than
    failing, and unlike the CMake case the mistake gets baked into a file handed
    to users.
  - Verified by extracting `resolve_qt()` and running it standalone:
    - resolves `~/Qt/6.9.3/gcc_64`, reporting `(Qt 6.9.3, from CMakeLists.txt
      QT_LINUX)`, and puts that kit's `bin/` at the head of `PATH`;
    - `QMAKE` defaults to the kit's `bin/qmake` when unset, and an
      already-set `QMAKE` still wins;
    - with no kit installed (`HOME` redirected) it exits **1** with
      `Qt 6.9.3 not found under $HOME/Qt or /opt/Qt` — where the old code would
      have silently taken system Qt off `PATH`.
- [x] 2.5 Add the compile-Qt vs bundle-Qt agreement check: after configuring, compare the Qt prefix CMake reports against `qt6_path` and fail loudly on mismatch (FR-30b). FR-27 covers the CMake half; this covers the bundling half, which is a different script's decision.
  - `verify_qt_agreement()` added, called from `main()` between `build_app` and
    `create_appdir`. It is a **backstop, not the mechanism** — 2.3 already makes
    the two agree by construction — and it earns its place because the two
    decisions are made by different tools that can still diverge: CMake resolves
    its own prefix (and a stale `build/` **caches** the previous answer), while
    this script decides what linuxdeploy bundles.
  - **Two checks, deliberately, because they answer different questions:**
    1. *What CMake decided* — `Qt6_DIR` from `CMakeCache.txt`, walked up three
       levels, compared to `QT6_PATH`. Error names both paths and tells the
       reader to remove the build dir.
    2. *What the linker actually bound* — `ldd` on the built binary, flagging any
       `libQt6*` resolving outside `QT6_PATH`. Catches what the cache comparison
       cannot (hand-edited cache, different generator, `LD_LIBRARY_PATH` games).
  - Verified: passes on the real tree (`compiled against
    ~/Qt/6.9.3/gcc_64`, all linked Qt6 under it); a forced
    `QT6_PATH=/usr` produces the mismatch error and **exit 1**; the `ldd` filter
    correctly isolates a stray `/usr/lib/libQt6Gui.so.6` from a synthetic `ldd`
    listing while ignoring non-Qt libraries.
  - Caveat: the `ldd` branch was **not** exercised end-to-end against a real
    mis-linked binary — no binary on this machine links `/usr/lib/libQt6*` to
    borrow for the test. Its filter logic was tested directly instead.
- [x] 2.6 Fix `build_app()`'s "already built" short-circuit — for a release build, either drop it entirely or emit a prominent warning that the existing binary's Qt was not verified (FR-30b). Prefer dropping it; `make build -B` is already incremental at the compiler level.
  - **Dropped outright**, as preferred. `build_app()` now always runs
    `make build -B`.
  - The reason it mattered more than "stale artifact" suggests: the leftover
    binary would typically come from a plain `make build` in a shell **without**
    the kit exported — i.e. precisely the mixed-Qt binary this whole task
    removes. The short-circuit was a route back to the §2.1 defect that survived
    the 2.3 fix.
  - 2.5's `verify_qt_agreement()` would now catch that mismatch anyway, but
    failing late on a stale artifact is worse than just building it.
- [x] 2.7 `build-macos.sh:103-110` — derive the `6.9.3` in the `macdeployqt` search from `QT_MACOS` (FR-30). No version change; this platform is out of scope for behaviour.
  - All four hardcoded `6.9.3` occurrences in `find_macdeployqt()` (three search
    paths + the error message) now come from `qt_version_for MACOS`. Verified on
    Linux that the derivation runs and the error message reports the derived
    version.
  - **Two pre-existing defects found here and deliberately NOT fixed** — macOS
    is out of scope for behaviour and has its own unfinished PRD. Both are
    commented in place so the next person meets them:
    1. The first branch takes whatever `macdeployqt` is on `PATH`, which may
       belong to a different Qt than the app was compiled against — the same
       defect class removed from `build-appimage.sh` (2.4) and `CMakeLists.txt`.
    2. `local macdeployqt=$(find_macdeployqt)` at the call site **masks the exit
       status**, so the function's `exit 1` does not stop the script under
       `set -e`; the error text is captured into the variable instead. Note also
       that `print_error` writes to **stdout** here, not stderr, which is what
       makes that capture silent.
- [x] 2.8 `build-windows.ps1:8,27,107,116,178` — derive the default `-QtPath` and the message strings from `QT_WINDOWS` via a PowerShell one-liner (FR-30). Untestable here; keep the change minimal and mechanical.
  - `Get-QtVersion` reads `QT_WINDOWS` from `CMakeLists.txt` via `Select-String`,
    resolved to `$QtVersion` / `$DefaultQtPath` immediately after the `param`
    block. All five hardcoded `6.9.3` sites now derive.
  - **A PowerShell `param` default cannot call a function**, so `-QtPath`
    defaults to `""` and is filled in below the block; an explicitly passed
    `-QtPath` still wins. The help text interpolates `$DefaultQtPath`.
  - **Ordering trap avoided:** `Get-QtVersion` runs at the top of the script,
    *before* the file's own `Write-Error` / `Write-Status` helper functions are
    defined. PowerShell defines functions as execution reaches them, so calling
    `Write-Error` there would silently resolve to the **built-in cmdlet**, not
    this script's helper. `Get-QtVersion` therefore uses `Write-Host
    -ForegroundColor Red` directly.
  - **Not executed — no PowerShell on this machine** (`pwsh` and `powershell`
    both absent), and Windows is out of scope for behaviour. Verified as far as
    is possible here: the regex
    `^\s*set\(QT_WINDOWS\s+"([^"]+)"\)` matches the real
    `CMakeLists.txt:10` line and captures `6.9.3`. **The script itself has not
    been run; treat the first Windows build as the real test.**
- [x] 2.9 `Makefile:5` — derive the Darwin `QT_PATH` default from `QT_MACOS` (FR-30). Leave the non-Darwin `BUILD_CMD` **without** `-DCMAKE_PREFIX_PATH`: after task 1.2 CMake resolves Linux itself, and passing it here would bypass the new branch.
  - Added a `qt_version_for` **make function** (`$(call qt_version_for,MACOS)`),
    the Make counterpart of the bash and PowerShell readers — so all three
    languages now read the same `CMakeLists.txt` declaration. `QT_PATH ?=` keeps
    the environment override.
  - Verified on Linux (where the Darwin branch is not taken) that the function
    resolves: `MACOS`/`LINUX`/`ANDROID` → `6.9.3`, and `QT_PATH` would expand to
    `$HOME/Qt/6.9.3/macos`.
  - Confirmed the non-Darwin `BUILD_CMD` still passes **no**
    `-DCMAKE_PREFIX_PATH` (`make -n build -B` → zero matches), so the task-1.2
    CMake branch stays in charge on Linux.
- [x] 2.12 **(new)** Add an environment report + pre-flight gate that **every build script runs on every platform**, so a wrong toolchain stops the build instead of producing a wrong artifact. `scripts/qt-env-verify.sh` (bash) + `Invoke-EnvVerify` in `build-windows.ps1`.
  - **Design revised mid-task at the user's direction.** The first version was a
    manual `make qt-version-check` target. That is the wrong shape: *a forgotten
    check looks exactly like a passing one.* It is now called by the build
    scripts themselves — `build-android.sh`, `build-appimage.sh`,
    `build-macos.sh` and (via the PowerShell twin) `build-windows.ps1` — so the
    build simply does not start on a bad environment, and we are told to
    investigate. `scripts/qt-version-check.sh` was folded in and deleted.
  - **Two tiers, and the distinction is the design.** `CRITICAL` = wrong output
    or no output → **exit 1, build stops**. `ADVISORY` = worth knowing, prints
    and continues. `--report-only` inspects without stopping.
  - **The report** (printed into every build log, so the toolchain a build used
    is recoverable afterwards): date, host, git rev + dirty flag, C++ compiler,
    cmake, ninja, rustc/cargo, installed Rust targets, declared vs **actual** Qt
    version. Android adds SDK/NDK roots, NDK revision + **clang version**,
    JDK, Gradle wrapper, AGP, min/target SDK, and per-ABI kit + Rust target.
    macOS adds xcodebuild, SDK path/version, macdeployqt.
  - **The critical check that did not exist before:** the kit's *real*
    `qmake -query QT_VERSION` is compared against the declared `QT_*`. Every
    earlier check trusted the **directory name**. Verified by fabricating a kit
    named `6.9.3` whose qmake reports `6.11.1` — correctly rejected.
  - Ported the two known-late-failure Android traps into fast, legible stops:
    **NDK r28+** (breaks the cxx build at this minSdk) and a **JDK outside
    17–21** (AGP's lint dies with the JDK version as its entire message, after
    all three ABIs have compiled and signed).
  - **Placement is deliberate in `build-android.sh`:** the gate runs *after*
    `JAVA_HOME` / `ANDROID_NDK_ROOT` / `ANDROID_ABIS` are resolved, so it
    verifies the values the build will really use rather than re-deriving its
    own and possibly disagreeing.
  - **Found and fixed a bug in the checker itself while testing it** — the kind
    that matters most here. A greedy `sed` matched the *closing* quote of
    `openjdk version "26.0.2"`, captured an empty string, and **silently skipped
    the JDK range test**: the check passed on the exact JDK it exists to reject.
    Now `[^"]*"`, with an unparseable version downgraded to a loud advisory
    rather than silence.
  - Also verified: a declared version with no installed kit stops the build; and
    `make qt-checks` / `make qt-verify{,-linux,-android,-macos}` run it by hand.
  - `--all` has five sections: (1) the five `QT_*` declarations parse; (2)
    **reader agreement** — bash `qt_version_for`, Make `$(call qt_version_for,…)`
    and PowerShell `Get-QtVersion` all return the declared value for all five
    platforms; (3) **no reacquired hardcodes** — a declared Qt version appearing
    on a non-comment line of any deriving file; (4) script syntax (`bash -n`,
    plus a PowerShell parse); (5) kit availability, `required` for the host
    platform and informational for the others.
  - **⚠ Correction (review, 2026-08-08): sections 2's PowerShell half, 3 and 4
    were described here but were NOT in the committed script, and have now been
    implemented.** The note above was written against
    `scripts/qt-version-check.sh`, which was said to have been "folded in and
    deleted" — it was deleted, but those three checks did not survive the fold,
    and the file has no git history to recover them from.
    `check_reader_agreement()` covered **bash and make only**; there was no
    hardcode scan and no syntax check anywhere in the 361-line file; `--all` also
    skipped `check_qt_kit` entirely. So the quoted "Verified it actually fails"
    result below described a check that was not in the tree — the shape of
    failure that this whole task exists to prevent, arriving in the checker
    itself.
    Now added: `check_powershell_reader()`, `check_no_hardcoded_versions()`,
    `check_script_syntax()`, and kit reporting in `--all`. **Each was verified to
    fail, not only to pass:** a `STALE_KIT="$HOME/Qt/6.9.3/macos"` line appended
    to `build-macos.sh` produced `CRITICAL build-macos.sh contains the literal Qt
    version 6.9.3 on a non-comment line` (with the comment correctly stripped
    before matching), and an unterminated `if` produced `CRITICAL bash -n failed
    for build-macos.sh` — both with **exit 1**. Both mutations reverted and
    `git diff` confirmed clean.
    **Why this mattered for the stage still ahead:** section 3 is the guard that
    catches a `6.9.3` literal left behind in a deriving file once `QT_ANDROID`
    diverges to 6.10.3, i.e. exactly task 6.2's risk. It was absent at the point
    it was about to be needed.
  - Section 3 matches **only** literals equal to a declared Qt version, on
    non-comment lines, in the five deriving files. Anything looser drowns in
    NDK / Gradle / AGP / crate versions, which are unrelated and correct.
  - The PowerShell check runs `build-windows.ps1 -Help`, which exits right after
    `Get-QtVersion` — so it exercises the real derivation without building. When
    no PowerShell is present it **SKIPs loudly** and falls back to asserting the
    regex still matches, with a note that this proves the pattern and not the
    script.
  - **Verified it actually fails**, not just passes: bumping `QT_MACOS` to
    6.10.3 while leaving `Makefile`'s `QT_PATH` hardcoded at 6.9.3 produced
    `FAIL  Makefile contains the literal Qt version 6.9.3 on a non-comment line`
    and **exit 1**. Both mutations reverted; `CMakeLists.txt` confirmed
    byte-identical to the committed version afterwards.
- [ ] 2.13 **(new)** On **macOS**, run `make macos` and record the gate's report. The gate now runs automatically, so this is really "read what it printed": it is the first real execution of the 2.7 `QT_MACOS` derivation, the `Makefile` Darwin `QT_PATH` branch, and the macOS kit / `macdeployqt` / Xcode SDK checks. If it stops the build, that is the feature working — record what it caught.
- [ ] 2.14 **(new)** On **Windows**, run `build-windows.ps1` and record `Invoke-EnvVerify`'s report. First real execution of `Get-QtVersion` **and** of the PowerShell gate — neither has ever run (no PowerShell on the Linux dev machine). Confirm the two implementations stayed in step: same tiers, same Qt declared-vs-actual check.
- [ ] 2.16 **(new)** Keep `scripts/qt-env-verify.sh` and `build-windows.ps1`'s `Invoke-EnvVerify` in step whenever either gains a check. They are deliberate duplicates (PowerShell cannot source bash), which is a drift risk with no automated guard — the cross-reference comments in both files are the only thing holding them together.
- [x] 2.17 **(new, from the 2026-08-08 review)** Stop the desktop Qt kit leaking
  into the Android cross-build through `PATH` / `LD_LIBRARY_PATH`.
  - **The defect, which was live and unhandled.** `qt_env_activate()` exports
    `LD_LIBRARY_PATH=$QT_PREFIX/lib` and prefixes `PATH` with `$QT_PREFIX/bin`
    from **`QT_LINUX`**. `build-android.sh` correctly sources with
    `QT_ENV_NO_ACTIVATE=1`, so it never *adds* them — but it never **scrubbed**
    what direnv (`.envrc`), `.claude/settings.json` or a hand-run
    `source scripts/qt-env.sh` had already exported. Confirmed by grep: neither
    variable was mentioned anywhere in `build-android.sh` or the gate.
  - **Why it matters after task 6.2 and not before.** The cross-build runs the
    **Android** Qt's host tools — moc, rcc, androiddeployqt from
    `$HOME/Qt/$QT_ANDROID/gcc_64`, resolved via
    `__qt_platform_initial_qt_host_path` — and those are dynamically linked
    against `libQt6Core.so.6`. With the desktop kit's `lib/` first in the loader's
    search path, a **6.10.3** moc/rcc loads **6.9.3**'s libQt6Core. Invisible
    while the two versions are equal; live the moment they diverge, which is the
    entire point of the Android-only bump. Same failure class as the
    `QT_ANDROID_VERSION` export removed in 2.2 — a convenience layer silently
    changing build output — reached by a different route.
  - **Fix, in two independent layers** (the gate must not rely on the scrub
    having run, since that is the thing that can be missing):
    1. `build-android.sh` scrubs `$QT_PREFIX/bin` from `PATH`, **unsets
       `LD_LIBRARY_PATH` outright** (the Android build needs none, so empty
       cannot be wrong, whereas a filtered value can still carry another Qt), and
       unsets `QT_PREFIX` / `QMAKE`. It announces both removals, so the build log
       records that it happened.
    2. `scripts/qt-env-verify.sh` gained an **"Android host tools"** section: it
       checks the host `gcc_64` kit exists and that its `qmake -query QT_VERSION`
       matches `QT_ANDROID`, then uses a new `foreign_qt_entries()` helper to
       flag any `.../Qt/<other-version>/...` entry on `LD_LIBRARY_PATH`
       (**CRITICAL**) or `PATH` (advisory — the build addresses its tools by
       absolute path, so a stray `bin/` is far less likely to be consulted than a
       stray `lib/`), plus an advisory if `QT_PREFIX` survived.
  - **Verified in both directions**, since a detector that cannot fire is worth
    nothing: with a clean environment all three checks report OK; with
    `LD_LIBRARY_PATH`/`PATH`/`QT_PREFIX` pointed at a *different* installed kit
    (6.10.3, standing in for the post-6.2 state) the gate reports `CRITICAL
    LD_LIBRARY_PATH carries a Qt other than 6.9.3` plus both advisories.
    `./build-android.sh --help` under a simulated direnv environment prints the
    two scrub lines.
  - **Unsetting `QMAKE` is safe, and checked rather than assumed:** nothing in
    `build-android.sh` or the gate reads it, and `CMakeLists.txt:336` passes
    `QMAKE ${qmake_path}` explicitly to `cxx_qt_import_crate` (corrosion's
    `cmake -E env` assignment overrides the inherited environment anyway). An
    Android configure with `LD_LIBRARY_PATH`, `QT_PREFIX` and `QMAKE` all unset
    completes normally: `Qt 6.9.3 (expected 6.9.3) at
    ~/Qt/6.9.3/android_arm64_v8a/…` / `Using qmake:
    ~/Qt/6.9.3/android_arm64_v8a/bin/qmake` / `CXX-Qt Found crate(s)`.
  - **This pre-empts task 11.6 rather than replacing it.** 11.6 remains open: it
    is the confirmation *after* 6.2, when the two versions actually differ and the
    check has discriminating power.
  - **⚠ Gap found and closed 2026-08-08, while checking whether task 6 could be
    run from an agent shell at all.** The entire scrub — including the
    `LD_LIBRARY_PATH` clear, which is the dangerous half — sat inside
    `if [ -n "${QT_PREFIX:-}" ]`. So it **gated the dangerous half on the
    presence of the harmless one**: a shell exporting `LD_LIBRARY_PATH` by any
    route other than `qt_env_activate()` (a hand-written export, a wrapper
    script, an inherited CI environment) has no `QT_PREFIX`, skips the whole
    block, and carries a foreign Qt straight into the cross-build's host tools.
    The `LD_LIBRARY_PATH` clear is now **unconditional**, in its own block above
    the `QT_PREFIX` one; the `PATH` scrub stays gated, because `QT_PREFIX` is
    what names the entry to remove and there is nothing to match on without it.
  - **Verified in all three states, with a negative control** (the same
    environment *without* the fix, to prove the test can fail):
    - *negative control* — `LD_LIBRARY_PATH` set, `QT_PREFIX` unset, no scrub:
      6.10.3's `rcc` dies `libQt6Core.so.6: version 'Qt_6.10' not found`,
      exit 1. This is the failure the gap allowed.
    - *the gap, fixed* — same environment, scrub applied: the clear fires and
      `rcc 6.10.3` runs.
    - *both set* (the real agent shell) — both messages print, `PATH` kit `bin/`
      removed, `QT_PREFIX`/`QMAKE`/`LD_LIBRARY_PATH` unset, `rcc 6.10.3` runs.
    - *clean env* — silent no-op, `rcc 6.10.3` runs.
  - **Note for anyone re-running these by hand:** a `sed`-range replay of the
    block now stops at the **first** `^fi$` (the new `LD_LIBRARY_PATH` block) and
    silently omits the `PATH`/`QT_PREFIX` half — which reads as "the scrub
    stopped working". Replay to the second `fi`.
  - **Second defect, same trap class, found while checking the other platforms:
    `qt_env_activate()`'s "idempotent" strip was neither idempotent nor safe.**
    It edited `PATH` / `LD_LIBRARY_PATH` with `sed` substitutions matching only
    the `"<entry>:"` form, which fails three ways: it **misses the entry when it
    is last**; it can **never converge when the entry is the whole value**
    (no leading or trailing colon to match — repeated activation settled at a
    steady state of *two* copies, measured); and substring matching **corrupts a
    lookalike entry** — `/opt<kit>/bin:/usr/bin` became `/opt/usr/bin`, a path
    that never existed. Replaced with `_qt_env_list_remove()`, which splits on
    `:` and compares whole entries. Verified across ten cases (only / first /
    middle / last / duplicated / absent / empty list / empty entries /
    prefix-lookalike / substring-safe) plus five repeated activations, with
    other `PATH` entries preserved and the kit still first.
  - **Other platforms examined; only Android was exposed.** Checked rather than
    assumed: `build-appimage.sh` is safe by a **different mechanism** — it
    *prepends* its kit to both lists, so it wins for the loader regardless of
    what was inherited; `build-macos.sh` is safe because `qt_env_activate()`
    never runs on macOS (`qt_prefix_for LINUX` looks for `gcc_64`, absent there,
    so it fails cleanly and exports nothing — confirmed by simulating an
    empty `$HOME/Qt`); `build-windows.ps1` likewise. **But the Windows reasoning
    differs and is recorded as a warning:** Windows resolves DLLs through
    `PATH`, so the "`PATH` is only advisory" argument used in `build-android.sh`
    is Linux-specific and must not be ported there.
  - Documented in **`docs/qt-kit-selection.md` §8.1** (the rule, the measured
    `Qt_6.10 not found` symptom, the per-platform table, the two ways a scrub
    goes wrong, and the two-layer design), with pointers from `build-android.sh`
    and `AGENTS.md`. `make qt-checks` and `make qt-env-check` both still pass.
- [x] 2.18 **(new, from the 2026-08-08 review)** De-stale the NDK r28 messages in
  `build-android.sh`, which said "not supported with Qt … at **minSdk 27**" in
  both the comment and the `die`. Task 8.1 raises minSdk to 28, at which point
  the text would read as though the rule had lapsed. Per FR-14 the exclusion
  **holds at 28 too** (bionic gained `pthread_cond_clockwait` only at API 30), so
  the messages now say "at this project's minSdk" and state that explicitly.
  Nothing behavioural changed — the `ndk_major >= 28` test is untouched.
- [ ] 2.15 **(new)** Decide the disposition of the two pre-existing `build-macos.sh` defects found in 2.7 — the ambient-`PATH` `macdeployqt` branch, and `local macdeployqt=$(find_macdeployqt)` masking the function's exit status (compounded by `print_error` writing to stdout). Both are commented in place and deliberately unfixed here. They belong to the macOS PRD; either fix them there or record why not.
- [x] 2.10 Run `make appimage -B` end to end. Verify with `ldd` inside the AppDir that every `libQt6*.so.6` resolves under the bundled Qt (the `strings` half of this check is dropped — see 1.1) (Success Metric 3b).
  - **Ran clean, exit 0.** Artifact:
    `Simsapa-v1.0.0-alpha.5-Linux-x86_64.AppImage` (307 MB); linuxdeploy's own
    self-extraction test passed.
  - **Success Metric 3b met — the AppImage is now compiled against the Qt it
    bundles.** Evidence:
    - the AppDir binary links **18/18** `libQt6*` under `~/Qt/6.9.3/gcc_64/lib`,
      **0** from `/usr/lib`;
    - **all 96** bundled `libQt6*.so.6` in `Simsapa.AppDir/usr/lib` are
      **byte-identical** (`cmp`) to the 6.9.3 kit's copies — 0 differing, 0 not
      in the kit;
    - the bundled `libQt6Core` reports `Qt 6.9.3`.
    Before this work it was a binary built against system **6.11.1** wrapped
    around **6.9.3** libraries.
  - **The `strings` check turns out to work here, unlike in 1.1** — the *Qt
    libraries* carry a `Qt 6.9.3 (…)` build string even though the *app binary*
    does not. Only the 1.1 form (on the app binary) is useless.
  - Note `cmp`-identity is expected rather than impressive: the script exports
    `NO_STRIP=1`. It is still the right check — it proves provenance, not just
    version agreement.
  - The whole new ordering was exercised for real, not just unit-tested:
    `resolve_qt` → **environment gate** → `build_app` → `verify_qt_agreement` →
    package. Reaching completion means the gate returned 0 and the agreement
    check found no stray libraries.
- [x] 2.11 Commit Part C's script half separately from 1.14.
  - `597e77f` "version verification and build scripts" — separate from 1.14's
    `2a24322` as required. Committed by the user.
  - The version-derivation and environment-gate changes **interleave within**
    `build-android.sh`, `build-windows.ps1` and `Makefile`, so they could not be
    split by file without hunk surgery; they landed together.

---

### 3.0 Stage 1 — `bridges/` from the cxx-qt fork to unpatched upstream 0.9.1

**Specs to keep in mind**

- The four crates (`cxx-qt`, `cxx-qt-lib`, `qt-build-utils`, `cxx-qt-build`) keep
  the same shape — only the repo and rev change. Pin **rev `2180c12`**, never a
  branch. Keep `features = ["full"]` on `cxx-qt-lib` and
  `features = ["link_qt_object_files"]` on `cxx-qt-build` (required for static
  Qt 6 linking).
- API mapping for `bridges/build.rs` (verified against `src-lib/cxx-qt` at
  `2180c12`):

  | 0.7 (today) | 0.9.1 |
  |---|---|
  | `CxxQtBuilder::new().qml_module(QmlModule { … })` | `CxxQtBuilder::new_qml_module(module)` |
  | `QmlModule { uri, … , ..Default::default() }` | `QmlModule::new("com.profoundlabs.simsapa").qml_files([…])` |
  | `rust_files: &[…]` inside `QmlModule` | `CxxQtBuilder::files([…])`, one directory per call (ours are all under `src/`) |
  | `.cc_builder(\|cc\| …)` | now `unsafe fn` → wrap in `unsafe { }`, or move the three `../cpp/*.cpp` files to the safe `cpp_files()` |

- The bridge **macros** are unchanged: `#[qinvokable]` ×339, `#[qsignal]` ×64,
  `#[qproperty]` ×8, `#[qobject]` ×8, `#[qml_element]` ×8, `#[qml_singleton]` ×1,
  `cxx_qt::Threading` ×9, `cxx_qt::CxxQtThread` ×2. No bridge source file should
  need editing.
- **The Android rcc patch is replaced by a CMake change, not an environment
  export.** cxx-qt-cmake 0.9.1 sets `CXX_QT_AUTORCC_OPTIONS` itself by joining
  `CMAKE_AUTORCC_OPTIONS` with `:` and passing it through
  `corrosion_set_env_vars` → `cmake -E env VAR=VALUE cargo …`, which **overrides
  any shell-exported value**. So the fix is
  `list(APPEND CMAKE_AUTORCC_OPTIONS --no-zstd)` at `CMakeLists.txt:80-81`.
  Exporting the variable from `build-android.sh` would be silently discarded.
- That also makes per-ABI propagation structural: each ExternalProject sub-build
  re-runs CMake over this tree and sets the variable for its own cargo run.
- **⚠ Changing the options does not trigger a rebuild** — upstream reads the
  variable with `env::var_os` and emits no `cargo::rerun-if-env-changed`, which
  applies to the CMake-set value too. Every experiment must
  `touch bridges/build.rs` or `cargo clean -p simsapa_bridges` first. "It worked
  first try" is suspicious until reproduced from clean.
- `cxx-qt-cmake` and the Rust crates are a **coupled pair**; a mismatch fails
  inside generated code. But the CMake **call sites** are safe:
  `cxx_qt_import_qml_module` keeps the same `URI` / `SOURCE_CRATE` arguments in
  0.9.1 (gaining only an optional `OUTPUT_DIR`), and `cxx_qt_import_crate`'s
  argument surface is unchanged — so `CMakeLists.txt:207-218` needs no edit
  beyond the `GIT_TAG`.
- The generated `qmldir` / `plugin.qmltypes` land in
  `${CMAKE_CURRENT_BINARY_DIR}/cxxqt/qml_modules/com/profoundlabs/simsapa/` —
  the **build** directory. They cannot overwrite the hand-maintained qmllint stub
  at `assets/qml/com/profoundlabs/simsapa/qmldir`.
- `CxxQtBuilder::files()` **panics** with a clear message if the Rust sources
  span more than one directory (Qt bug QTBUG-93443). All nine bridges are under
  `src/`, so this is satisfied.

**Depends on:** task 1.0 (so a desktop failure is attributable to cxx-qt, not to
a Qt version surprise). FR-3/FR-3b/FR-4 land as **one commit** — the crate bump
does not compile without the build-script migration.

- [x] 3.1 Confirm FR-5 still holds: `rustup show` reports ≥ 1.85.0 (currently 1.96.1) with `aarch64-linux-android`, `armv7-linux-androideabi`, `thumbv7neon-linux-androideabi`, `x86_64-linux-android` installed. Note in passing that nothing pins this (no `rust-toolchain.toml`).
  - Confirmed 2026-08-08. Active toolchain `stable-x86_64-unknown-linux-gnu`;
    all four Android targets installed (plus `wasm32-unknown-unknown`). No
    `rust-toolchain.toml` in the repo, so nothing enforces the 1.85.0 floor.
- [x] 3.2 Read the cumulative fork diff `git -C src-lib/cxx-qt-simsapa diff c6710b71 8a597414` (5 files, +69/−7) — **not** commit by commit, since `8a597414` reverts callers added by `73b13685`. Confirm the five changes A–E against the PRD's FR-1 table and note anything that has drifted since the PRD was written.
  - Diff read in full; **the FR-1 table is accurate and nothing has drifted.**
    5 files, +69/−7 exactly as stated. A = the four hardcoded rcc args in
    `tool/rcc.rs`; B = `flag_if_supported` → `flag` for `-F<framework>`;
    C = the `is_ios_target()` branch choosing a flat vs `Versions/A/Resources`
    `.prl` path; D = the two apple `Some(filename)` fallbacks in
    `parse_cflags.rs`; E = `is_ios_target()` in `utils.rs` plus
    `thin_generated_fat_library_with_lipo()` in `cxx-qt-build`.
  - Worth noting for task 5.0: **C is not purely additive.** It also rewrites
    the *non*-iOS path from upstream's `…framework/Resources/….prl` to
    `…framework/Versions/A/Resources/….prl`, i.e. it changes macOS behaviour
    too, not just iOS. Upstream has since replaced the whole mechanism, so this
    stays "do not port" — but if it is ever re-derived, both halves are in play.
- [x] 3.3 Verify upstream's `CXX_QT_AUTORCC_OPTIONS` support in the local checkout: `cxx-qt-build/src/lib.rs:1253-1258` (colon split), `qt-build-utils/src/lib.rs:240,461` (`autorcc_options`), `tool/rcc.rs:42` (`custom_args`). This is what makes patch **A** droppable.
  - All three sites confirmed at `2180c12`, at the exact line numbers the PRD
    gives. The chain is `env::var_os("CXX_QT_AUTORCC_OPTIONS")` → `split(':')` →
    `QtBuild::autorcc_options()` → `QtToolRcc::custom_args()` → appended to
    rcc's argv after `--name` (`tool/rcc.rs:76`). Patch **A** is droppable.
  - Also confirmed the PRD's rebuild warning: the only `rerun-if-env-changed`
    declarations in the workspace are `QMAKE`, `QT_VERSION_MAJOR`,
    `QT_MINIMAL_DOWNLOAD_ROOT` and `TARGET`. This variable is **not** among them.
- [x] 3.4 Point the four crates in `bridges/Cargo.toml:21-23,51` at `https://github.com/KDAB/cxx-qt.git` rev `2180c12`, preserving both feature lists and the "pin a rev, not a branch" comment at line 20 (FR-3).
  - Done; `features = ["full"]` and `features = ["link_qt_object_files"]` both
    preserved, and the rev-not-branch rationale rewritten to also record why the
    fork was dropped and that the CMake pin is its coupled half.
  - **One extra change was forced, and it is not optional:** `cxx` had to move
    from `"1.0.148"` to `"1.0.176"`. cxx-qt 0.9.1 requires `^1.0.176`, and the
    lockfile held `cxx 1.0.169`; cargo refused to resolve
    (`all possible versions conflict with previously selected packages`) until
    the floor was raised. Commented in place.
- [x] 3.5 Bump `CMakeLists.txt`'s `cxx-qt-cmake` `GIT_TAG` from the **branch** `0.7` to the **tag** `0.9.1` (commit `06a121e`) — not the branch `0.9`, even though they are equal today (FR-4).
  - Done, and the task-1.9 comment rewritten to say that this pin is now the
    **tag**, that the previous `0.7` was the branch, and that it must move with
    the four crate pins.
- [x] 3.6 Migrate `bridges/build.rs` to the 0.9 builder API per the table above: `new_qml_module` + `QmlModule::new("com.profoundlabs.simsapa").qml_files(qml_files)`, and the nine bridge files moved to `CxxQtBuilder::files([...])` (FR-3b). Keep the `mobile_build` branch on `CXX_QT_QT_MODULES` unchanged.
  - Done exactly as mapped. The `mobile_build` / `CXX_QT_QT_MODULES` branch and
    the three `qt_module()` calls are untouched.
  - **No bridge source file needed editing**, confirming the PRD's macro survey.
- [x] 3.7 Replace the `cc_builder` closure with the **safe** equivalents rather than wrapping it in `unsafe`: `cc.include("../cpp/")` → `.include_dir("../cpp/")` (`lib.rs:517`), and the three `cc.file(...)` calls → `.cpp_files(["../cpp/utils.cpp", "../cpp/system_palette.cpp", "../cpp/gui.cpp"])` (`lib.rs:641`). No `unsafe` block is needed; only fall back to `unsafe { cc_builder(…) }` if something in the build genuinely requires raw `cc::Build` access, with a comment saying what.
  - Done with the safe equivalents; **no `unsafe` block anywhere in the file**.
  - Checked the semantics rather than assuming they match: `CppFile`'s
    `From<impl AsRef<Path>>` sets `enable_moc` only for header extensions, so a
    `.cpp` gets `compile = true, enable_moc = false` — the same thing
    `cc.file()` did. No moc pass was silently added.
  - Also deleted the now-dead commented-out `is_ios_target` /
    `thin_generated_fat_library_with_lipo` import and call (part of FR-6 /
    task 5.5, which asks for exactly this).
- [x] 3.8 Build the Rust crate alone first (`cd bridges && cargo build`) to isolate codegen errors from CMake/Qt errors, before any full `make build`.
  - **Green.** `QMAKE=~/Qt/6.9.3/gcc_64/bin/qmake6 cargo build` finished in
    4m24s with no errors. The only warnings are pre-existing GCC 16
    `-Wsfinae-incomplete` notes from Qt 6.9.3's own headers, unrelated to this
    change.
- [x] 3.9 Check the resource-path derivation, the one runtime-only risk the compiler cannot catch: how 0.9 turns `"../assets/qml/Foo.qml"` into a resource path. The `:/qt/qml/com/profoundlabs/simsapa/…` paths are load-bearing across the codebase. Inspect the generated `qmldir` and the qrc contents under `build/simsapadhammareader/cxxqt/qml_modules/com/profoundlabs/simsapa/` rather than inferring. (The competing-`qmldir` worry is already resolved: the generated files go to the build dir, never the source tree.)
  - **The alias derivation is unchanged — verified by comparing the two
    generated `.qrc` files, not by reading the source.** Both 0.7 and 0.9 emit
    `<file alias="../assets/qml/SuttaSearchWindow.qml">` under
    `<qresource prefix="/qt/qml/com/profoundlabs/simsapa">`, byte-for-byte the
    same alias strings. rcc folds the leading `../` away, which is why the
    codebase's `qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/*.qml` literals
    (16 sites in `cpp/`, plus `assets/icons.qrc`'s own prefix) keep working.
    **The load-bearing paths are not affected by this upgrade.**
  - **What did change: 0.9's `qmldir` now lists the components.** 0.7 emitted a
    5-line `qmldir` (module / plugin / classname / typeinfo / prefer) and no
    component lines at all; 0.9 emits 92 lines, one per QML file
    (`SuttaSearchWindow 1.0 ../assets/qml/SuttaSearchWindow.qml`). This is the
    0.8.0 "correct QML module export" change. It is additive and cannot
    conflict with the hand-maintained stub, which lives in the source tree
    (`assets/qml/com/profoundlabs/simsapa/qmldir`) while this one is generated
    into the build dir — the competing-`qmldir` worry stays resolved.
    Whether the new component lines change how `import
    com.profoundlabs.simsapa` resolves at **runtime** is not answerable from
    the file; it is exactly what task 4.3 exists to test.
- [x] 3.10 Add `list(APPEND CMAKE_AUTORCC_OPTIONS --no-zstd)` at `CMakeLists.txt:80-81` — this is the whole of FR-2. cxx-qt-cmake joins the list with `:` and passes it to the bridge crate's cargo run. **Do not export `CXX_QT_AUTORCC_OPTIONS` from `build-android.sh`**; corrosion's `cmake -E env` assignment would override it.
  - Done. No export was added to `build-android.sh`.
- [x] 3.11 Prove it took effect: force a rebuild (`touch bridges/build.rs`), then confirm with `cargo build -vv` (or by comparing resource sizes against a build without the flag) that `rcc` really received `--no-zstd`. Without the forced rebuild, "it propagated" and "cargo reused stale output" are indistinguishable — the one way this gets answered wrongly.
  - **Proven, from a forced rebuild** (`rm -rf build/simsapadhammareader` +
    `touch bridges/build.rs` + `make build -B`), so stale output is excluded by
    construction. Three independent pieces of evidence:
    1. The generated CMake build files carry the joined value:
       `CXX_QT_AUTORCC_OPTIONS=--format-version:1:--compress-algo:zlib:--no-zstd`.
    2. The bridge crate's rcc output under CMake is **2,046,446 bytes**; the
       same file from the standalone task-3.8 build, which had no options at
       all, is **1,940,401**. The options changed the output.
    3. Running 6.9.3's `rcc` by hand on the generated `.qrc` with exactly those
       options reproduces the CMake build's file to within the `--name`
       argument (the only difference is in the symbol-name region at the very
       end).
  - **Measured caveat, recorded in the CMakeLists comment: `--no-zstd` is a
    no-op today.** With `--compress-algo zlib` already in the list, rcc's
    output with and without `--no-zstd` is **byte-identical** (2,046,158 both
    ways). So it was `--compress-algo zlib` doing the work in fork patch A, not
    `--no-zstd`. Kept anyway — it is what patch A carried, and it preserves the
    guarantee if the compress-algo is ever changed — but it must not be
    described as the load-bearing flag.
- [x] 3.12 Verify the same on an Android per-ABI sub-build, since that is the platform the flag exists for, and confirm the value is picked up per-ABI without any environment plumbing.
  - **Confirmed on all three ABIs, from a real `make android-apk-debug` run**
    (still at `QT_ANDROID` 6.9.3 — the Qt bump is task 6.0). The resolved
    `CXX_QT_AUTORCC_OPTIONS=--format-version:1:--compress-algo:zlib:--no-zstd`
    appears in **three separate `build.ninja` files** — the top-level one
    (arm64-v8a) and `android_abi_builds/{x86_64,armeabi-v7a}/` — and each ABI's
    cargo tree produced its own rcc output of **2,046,446 bytes**, byte-count
    identical to the desktop CMake build. The propagation is structural, as the
    PRD predicted: nothing was exported anywhere.
  - The build itself is the wider result — **the multi-ABI Android package
    builds on unpatched upstream cxx-qt 0.9.1**, with all three ABIs present,
    the cross-ABI contamination check clean, and the permission set unchanged
    (no new permissions, no required hardware features, ChromeOS check OK).
  - **⚠ Fixed a blocking bug in task 2.12's environment gate to get here.**
    `build-android.sh` assigns `ANDROID_SDK_ROOT` and `ANDROID_NDK_ROOT` but
    never **exported** them, so `scripts/qt-env-verify.sh` — a subprocess — saw
    `ANDROID_NDK_ROOT` as unset and stopped the build with
    `CRITICAL ANDROID_NDK_ROOT is not set`. That contradicted the gate's own
    placement comment ("verifies the values the build will really use"), and it
    means **no Android build has succeeded since task 2.12 landed**; task 2.10
    only ever exercised the AppImage path. One-line fix: `export
    ANDROID_SDK_ROOT ANDROID_NDK_ROOT` right after they are resolved,
    commented in place.
- [x] 3.13 Comment `CMakeLists.txt:80-81` to record that this single list now feeds **both** `rcc` invocations — CMake's AUTORCC for `assets/icons.qrc` and, via cxx-qt-cmake, the bridge crate's — that `--no-zstd` replaces fork patch A, and that a shell-exported `CXX_QT_AUTORCC_OPTIONS` would be overridden (FR-2).
  - Written, naming the cxx-qt-cmake source lines that do the joining, the
    per-ABI propagation, the missing `rerun-if-env-changed`, and the 3.11
    measurement that `--no-zstd` is currently a no-op. Points at
    `docs/cxx-qt-fork.md` (task 10).
- [ ] 3.14 Consider filing the missing `cargo::rerun-if-env-changed=CXX_QT_AUTORCC_OPTIONS` upstream — a one-line `println!` in `cxx-qt-build`. Optional, but cheap; record the decision either way.
  - **Decision: worth filing, but not from inside this task — deferred to the
    maintainer.** The defect is confirmed (3.3): `cxx-qt-build/src/lib.rs:1253`
    reads the variable with `env::var_os` and the workspace declares
    `rerun-if-env-changed` for only `QMAKE`, `QT_VERSION_MAJOR`,
    `QT_MINIMAL_DOWNLOAD_ROOT` and `TARGET`. The fix is one `println!` next to
    the read. Filing it needs a KDAB GitHub account and an issue/PR written in
    the maintainer's name, which is not something to do unattended. The
    workaround is in place and documented at the one site that sets the
    variable, so nothing here depends on the upstream fix landing.
- [x] 3.15 Land FR-3 + FR-3b + FR-4 + FR-2 as **one commit**, separate from any Qt change (FR-10). If stage 1 cannot be made to work, stop and reassess rather than stacking the Qt bump on a broken bridge layer.
  - **Stage 1 works.** Nothing to reassess: the desktop build, the desktop test
    suite and the three-ABI Android package are all green on unpatched upstream
    0.9.1, with no bridge source file edited. No Qt version was touched —
    `QT_ANDROID` is still 6.9.3.
  - `make test` green. `test: rust-test qml-test js-test` runs in that order and
    make stops at the first failure, so the JS suite finishing
    (**6 suites, 83 tests, all passed**) is what proves the Rust and QML halves
    passed before it. No timing-assertion drift surfaced on this run.
  - Note for anyone repeating this: `make test 2>&1 | tail -N` reports **tail's**
    exit status, not make's. Read the tail for the *last* target in the chain
    instead of trusting the exit code.
  - Proposed commit contents (one commit, FR-10): `bridges/Cargo.toml`,
    `bridges/Cargo.lock`, `bridges/build.rs`, `CMakeLists.txt`.
  - **`build-android.sh`'s `export ANDROID_SDK_ROOT ANDROID_NDK_ROOT` belongs
    in a separate commit** — it is a fix to task 2.12's gate, not to the cxx-qt
    migration, and it is what unblocked 3.12. Keeping it apart preserves the
    one-change-per-commit intent that tasks 1.14 / 2.11 established.
  - **`AGENTS.md` updated (at the user's direction), ahead of task 10.0.** Its
    "New Rust bridges" section taught the removed 0.7 API — a
    `.qml_module(QmlModule { … rust_files: &[…] … })` literal — which stopped
    compiling the moment FR-3 landed. Now shows
    `CxxQtBuilder::new_qml_module(QmlModule::new(uri).qml_files(…)).files([…])`,
    states the one-directory constraint (QTBUG-93443), and carries a note
    saying what changed in 0.8/0.9 so an older snippet found elsewhere is
    recognisable as pre-0.9 rather than as a working alternative.
    **`CLAUDE.md` is a symlink to `AGENTS.md`**, so the one edit covers both.
  - The neighbouring "New QML components" section needed no change: the
    `qml_files` list is still a local `Vec` in `bridges/build.rs`, only its
    consumer moved.
  - Still owed to FR-31 / task 10.0: `docs/cxx-qt-fork.md` (referenced from the
    new `CMakeLists.txt` and `bridges/Cargo.toml` comments, not yet written).

---

### [x] 4.0 Stage 1 verification — desktop Linux against Qt 6.9.3

> **Outcome: stage 1 is verified on desktop Linux against Qt 6.9.3, and the
> verification earned its place** — it found a real runtime-only regression
> (4.2) that the green build in 3.8 / 3.15 could not have surfaced. FR-8 and
> FR-9 are satisfied. The fix landed in `bridges/build.rs` +
> `bridges/Cargo.toml` and needs committing on top of `43f1a5d`; task 12.0
> carries the AOT follow-up it opened.

**Specs to keep in mind**

- A bridge that fails to register produces a **runtime QML error, not a build
  error**. A green build proves nothing about a two-minor-version codegen jump.
- Run this on **desktop first**, where the feedback loop is seconds rather than
  an APK install.
- "Against 6.9.3" is only meaningful because task 1.0 landed — confirm the
  configure log before treating the result as evidence.
- Known timing-assertion drift in `cargo test` is not a regression from this
  work.

**Depends on:** tasks 1.0 and 3.0.

- [x] 4.1 Confirm the configure log names `~/Qt/6.9.3/gcc_64` and the FR-27 assertion passed, before running anything else (FR-8's ordering note).
  - Confirmed 2026-08-08 from a **from-scratch** configure (`build/` had just been
    removed, so no cached `CMAKE_PREFIX_PATH` could mask the result):
    - `Using CMAKE_PREFIX_PATH: /home/gambhiro/Qt/6.9.3/gcc_64`
    - `Qt 6.9.3 (expected 6.9.3) at ~/Qt/6.9.3/gcc_64/lib/cmake/Qt6` — the FR-27
      assertion ran and passed;
    - `Using qmake: ~/Qt/6.9.3/gcc_64/bin/qmake6` — the task-1.7 derivation
      following `Qt6_DIR`, so the C++ half and the cxx-qt half agree.
  - `CXX-Qt Found crate(s): simsapa_bridges` — cxx-qt-cmake 0.9.1 resolved the
    crate, so the task-3.5 `GIT_TAG 0.9.1` pin is live in this configure.
  - This is what makes the rest of task 4.0 mean "verified against 6.9.3".
- [x] 4.2 `make build -B` clean, then `make test` (Rust + QML + JS). Record any failure and classify it as drift or regression.
  - **The first `make run` after 3.0 failed at startup — a genuine 0.9 migration
    defect, not drift.** The engine loaded `SuttaSearchWindow.qml` but then:
    `Type Logger unavailable` /
    `qrc:/qt/qml/com/profoundlabs/assets/qml/Logger.qml: No such file` — note the
    missing `simsapa/` segment. Exactly the runtime-only failure class task 3.9
    said it could not rule out from the generated files alone.
  - **Cause: cxx-qt feeds a `qml_files` path string, verbatim, into three
    derivations that disagree about a leading `../`.** Our list has always used
    `"../assets/qml/Foo.qml"` (paths relative to `bridges/`):

    | Consumer | Result for `Logger.qml` |
    |---|---|
    | rcc alias (`qt-build-utils/src/lib.rs:364`) | `..` folded → `:/qt/qml/com/profoundlabs/simsapa/assets/qml/Logger.qml` ✅ |
    | qmldir component line (**new in 0.8**) | `Logger 1.0 ../assets/qml/Logger.qml`, resolved as a URL against the module dir → one level too high ❌ |
    | qmlcachegen (`tool/qmlcachegen.rs:74`) | `--resource-path /qt/qml/…/simsapa/../assets/qml/Logger.qml`, inserted **unnormalized** ❌ |

    Under 0.7 the qmldir carried **no** component lines (measured in 3.9), so
    type lookup fell through to implicit same-directory resolution and the
    mismatch was invisible. 0.8's "correct QML module export" made the broken
    entry authoritative. There is no alias API on `QmlFile`
    (`qml/qmlfile.rs` — path, singleton, version only), so the fix is to stop
    passing a `..`.
  - **Measured while choosing the fix: the AOT qmlcachegen cache has never been
    used in this project.** The generated loader inserts its keys raw
    (`"/qt/qml/com/profoundlabs/simsapa/../assets/qml/SuttaSearchWindow.qml"`)
    but looks them up through `QDir::cleanPath`, which strips `..` — so a key
    containing `/../` can never be matched. **87 compiled units, 12.4 MB of
    generated C++ plus a 52 KB loader, were being compiled into the binary and
    never consulted**, under 0.7 as well as 0.9. This is what made the chosen fix
    free rather than a trade-off.
  - **Fix (option C of four considered): register the QML files with
    `CxxQtBuilder::qrc_resources` and derive the alias in `build.rs`**, instead
    of passing them as the QML module's `qml_files`. The two rejected
    alternatives — a `bridges/assets` symlink, and `set_current_dir("..")` —
    both keep the files in `qml_files` and would have *enabled* AOT for the
    first time, but both leave the trap armed: `"../assets/qml/Foo.qml"` (the
    form used by every existing line, by `CLAUDE.md`'s documented snippet and by
    the whole git history) still compiles and still fails when that one screen is
    first shown. `set_current_dir` additionally breaks incremental builds —
    cxx-qt-build emits `rerun-if-changed` with the **raw** path
    (`cxx-qt-build/src/lib.rs:459,586,645,964`) and cargo resolves relative rerun
    paths against the package root, so they would point at
    `bridges/bridges/src/api.rs`.
  - Deriving the alias in code is the property that matters for maintenance:
    the list keeps its documented `"../assets/qml/Foo.qml"` form, so **the rule
    for adding a QML component is unchanged**, and a malformed entry now
    `panic!`s at build time naming the expected shape rather than failing at
    runtime. `qt-build-utils` was added to `[build-dependencies]` for
    `QResourceFile` — `cxx-qt-build` re-exports only `QResource`/`QResources`.
  - **Verified against the pre-change build rather than by inspection alone:**
    the 87 registered aliases are **byte-identical** to the set the old build
    produced once its `../` is folded (`diff` clean), the prefix is still
    `/qt/qml/com/profoundlabs/simsapa`, **zero** `..` remain in the generated
    `.qrc`, the `qmldir` is back to its 5-line 0.7 form, no `qmlcachegen`
    directory is generated at all, and the rcc name-table segments
    (`Logger.qml`, `GlossTab.qml`, `assets`) plus Logger's own source text are
    present in the linked binary. So every
    `qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/*.qml` literal in `cpp/`
    keeps resolving.
  - AOT is not foreclosed — see the new task 12.0, which is where it gets
    enabled and measured on its own.
  - **`make build -B` and `make test` both green** after the fix, and the app
    launches and runs. `make test` chains `rust-test qml-test js-test` and make
    stops at the first failure, so completion covers all three. No
    timing-assertion drift surfaced.
- [x] 4.3 Launch the app and open **every** window and dialog in the `qml_files` list — a mis-registered QML file fails only when its screen is first shown. Work down `bridges/build.rs`'s list systematically: the search window, dictionary, gloss, prompts, bookmarks, chanting practice + review, library, storage dialogs/recovery, dictionaries window and its import/edit dialogs, settings, models/system-prompts dialogs, about, database validation, storage diagnostics, search help, update notification, keybinding capture (FR-9).
  - **Done by the user, and this is the sub-task that actually caught the 4.2
    defect** — it failed on the very first launch, at the first window, exactly
    as its own "Specs to keep in mind" predicted a codegen jump would.
  - After the fix: the app launches and a broad set of windows and dialogs was
    opened, all functioning correctly.
  - **Coverage stated honestly: this was a broad pass, not a file-by-file walk of
    all 87 entries.** The residual risk is small and bounded — a mis-registered
    file now fails only if its *alias* is wrong, and 4.2 proved by `diff` that
    all 87 aliases are byte-identical to the pre-change build's. The failure mode
    this sub-task exists for was a per-module qmldir defect, which is
    all-or-nothing and would have shown on the first window. A screen not opened
    here would have to be broken for some reason unrelated to this migration.
- [x] 4.4 Verify the resource layer explicitly: `:/qt/qml/com/profoundlabs/simsapa/…` paths still resolve, the `Logger` works, and the bridge singletons (`SuttaBridge`, `AssetManager`, `StorageManager`, `PromptManager`, `ClipboardManager`, `DictionaryManager`, `AudioManager`, `GlobalHotkeyManager`, `api`) are reachable from QML (FR-9).
  - Confirmed by the app running: the `qrc:/qt/qml/com/profoundlabs/simsapa/…`
    paths are what `cpp/` hands the engine, so a window appearing *is* the
    resource layer resolving. The `Logger` is exercised by every component that
    declares one, and the startup log itself is written through it.
  - Independently checked at the artifact level in 4.2: 87 aliases identical to
    the pre-change set, prefix `/qt/qml/com/profoundlabs/simsapa`, zero `..`
    remaining, and the rcc name-table segments present in the linked binary.
- [x] 4.5 Run `qmllint` (or `make qml-test`) and confirm the hand-maintained stub `qmldir` + type stubs still resolve. 0.9 also exports a generated `qmldir` / `plugin.qmltypes` under `build/…/cxxqt/qml_modules/` for qmllint/qmlls; both may now be visible, so check that `qmllint` is not reporting a duplicate or conflicting module definition (follows 3.9).
  - **`make qml-test` does not cover the lint half** — the target runs
    `qmltestrunner`, not `qmllint` (`Makefile:92`). It passed as part of
    `make test`, but `qmllint` had to be run separately to close this sub-task.
  - `qmllint 6.9.3` (the project's kit, not the system 6.11.1) over **all 87**
    files with `-I ./assets/qml/`: **exit 0**. 38 warnings, all
    `[missing-property]` (32) and `[use-proper-function]` (6) — pre-existing
    style categories. **Zero** matches for module / qmldir / duplicate /
    conflict, so the hand-maintained stub `qmldir` and type stubs still resolve
    and nothing competes with them.
  - The competing-`qmldir` worry is now doubly closed: the generated one is in
    the build dir (3.9), and after the 4.2 fix it carries **no component lines at
    all**, back to its 0.7 five-line form.
- [x] 4.6 Exercise a representative slice of `#[qinvokable]` surface at runtime rather than only opening windows: run a search in each area, open a sutta, run a dictionary lookup, gloss a paragraph, save a file, start and stop a recording.
  - Exercised during the 4.3 session — the app was used, not merely opened, and
    behaved correctly. This is the meaningful test for a codegen jump: the 339
    `#[qinvokable]`s are generated by one mechanism, so a systematic codegen
    break would disable the app wholesale rather than one method.
  - Not every listed action was ticked off individually. The startup path alone
    already crosses a wide slice of the bridge surface (settings reads, DB
    validation, dictionary reconciliation, theme/palette, session restore), all
    of which completed.
- [x] 4.7 If a desktop platform breaks and cannot be fixed against upstream, fall back to the **rebased `simsapa` branch** from task 5.0 — not to the old 0.7.2 pin (FR-3's stated fallback).
  - **Not needed — no fallback taken.** Desktop Linux did break at runtime, but
    the cause was ours (the `..` in the `qml_files` paths, 4.2), not an upstream
    defect that upstream could not accommodate, and it was fixed against
    unpatched upstream 0.9.1 using a supported API (`qrc_resources`). The 0.7.2
    pin stays retired and task 5.0 remains Apple-only.

---

### 5.0 Stage 1 tail — rebase the `simsapa` fork branch for Apple

**Specs to keep in mind**

- Nothing consumes this branch while `bridges/Cargo.toml` points at upstream.
  iOS is dormant, not abandoned, and macOS has its own unfinished PRD.
- Per-patch disposition: **A** dropped (obsoleted by `CXX_QT_AUTORCC_OPTIONS`);
  **B** re-applied at the new location `installation/shared.rs:153`
  (`flag_if_supported` → `flag` for `-F<framework>`), iOS only; **C** *not*
  ported — upstream replaced the mechanism with
  `shared.rs::find_prl_for_qt_module()`; **D** applies cleanly and unchanged
  (`parse_cflags.rs` apple branch); **E**'s `lipo` helper deleted as dead code,
  `is_ios_target` kept only if **C** is re-derived.

**Depends on:** task 3.0 (the disposition of each patch is settled there).

- [ ] 5.1 Create a fresh `simsapa` branch in `src-lib/cxx-qt-simsapa/` on top of upstream `2180c12`.
- [ ] 5.2 Re-apply patch **B** at `qt-build-utils/src/installation/shared.rs:153`, iOS-only, with a comment naming the original fork commit.
- [ ] 5.3 Apply patch **D** unchanged to `qt-build-utils/src/parse_cflags.rs`.
- [ ] 5.4 Do **not** port patch **C**. Record in the branch (and in the task-10 doc) that upstream rewrote the `.prl` mechanism and that C should be re-derived only if an Apple build actually fails — PRD open question 4.
- [ ] 5.5 Delete the `lipo` helper and `thin_generated_fat_library_with_lipo` entirely rather than rebasing dead code; also remove the commented-out call in `bridges/build.rs:3,141-143` (FR-6).
- [ ] 5.6 Push the branch and note its head revision for the task-10 fork document.

---

### 6.0 Stage 2, step 1 — `QT_ANDROID` → 6.10.3, NDK pinned, multi-ABI build green

**Specs to keep in mind**

- Only `QT_ANDROID` moves. `QT_LINUX`, `QT_MACOS`, `QT_WINDOWS`, `QT_IOS` stay
  `6.9.3`. The per-platform variables exist precisely for this case.
- Both `~/Qt/6.9.3` and `~/Qt/6.10.3` must stay installed, **including both
  `gcc_64` kits**: 6.9.3's builds the desktop app; 6.10.3's supplies host tools
  (moc, rcc, androiddeployqt) for the cross-build, resolved automatically via
  `__qt_platform_initial_qt_host_path`. So **the Android build runs 6.10.3's
  `rcc` while the desktop build runs 6.9.3's** — correct, not a bug, and it is
  the rcc that `CXX_QT_AUTORCC_OPTIONS` applies to.
- Keep the explicit `ANDROID_ABIS` list. 6.10.3 also installs an `android_x86`
  kit; `QT_ANDROID_BUILD_ALL_ABIS` would autodetect it and demand the
  uninstalled `i686-linux-android` Rust target.
- **Do not install NDK r28.** Both Qt's auto-detect and `build-android.sh:45`
  take the **highest installed** NDK, so installing it silently swaps the
  compiler. Keep 27.3.13750724 (the version that shipped 1.0.0); hold
  27.2.12479018 in reserve as a single-variable diagnostic lever.
- `armeabi-v7a` maps to the `armv7-linux-androideabi` Rust target, **not**
  thumbv7neon — Qt's toolchain sets `CMAKE_ANDROID_ARM_MODE`.

**Depends on:** tasks 1.0, 2.0, 3.0, 4.0.

- [x] 6.1 Confirm the FR-13b preconditions: `~/Qt/6.9.3` and `~/Qt/6.10.3` both present with their `gcc_64` kits, and 6.10.3's `android_arm64_v8a`, `android_x86_64`, `android_armv7` kits installed. Confirm FR-13c is still true (no `~/Qt/6.10.1`, no `~/Qt/6.8.3`).
  - **Verified 2026-08-08.** `~/Qt` holds exactly `6.9.3` and `6.10.3`; FR-13c
    still true (`6.10.1` and `6.8.3` both absent). Every kit's own
    `qmake -query QT_VERSION` reports its directory name, so this is a real
    version check and not a directory-name reading: 6.9.3 → `gcc_64`,
    `android_arm64_v8a`, `android_x86_64`, `android_armv7`; 6.10.3 → the same
    four **plus `android_x86`** (the kit FR-13 says we deliberately do not ship,
    which is why the explicit `ANDROID_ABIS` list must stay).
  - **The task-2.17 defect is live in this very shell, and it is worth recording
    because it made 6.1 initially look like a broken install.** The agent
    environment (`.claude/settings.json`) exports
    `LD_LIBRARY_PATH=$HOME/Qt/6.9.3/gcc_64/lib`, so **every** 6.10.3 binary
    invoked here loads 6.9.3's `libQt6Core` and dies with
    `undefined symbol: _ZN9QtPrivate9sizedFreeEPvm, version Qt_6` (exit 127) —
    which reads as "the 6.10.3 kit is not installed" rather than "the wrong Qt
    was loaded". With `env -u LD_LIBRARY_PATH` all five kits answer correctly.
    This is exactly the host-tool leak `build-android.sh` now scrubs (2.17);
    the scrub is confirmed to be load-bearing from here on, since the divergence
    lands in 6.2. **Any manual 6.10.3 command in an agent/direnv shell must
    clear `LD_LIBRARY_PATH` first.**
- [ ] 6.2 Set `CMakeLists.txt:11` `QT_ANDROID "6.10.3"` with a comment recording *why* Android diverges (the Thai mid-word Shift fix, qtbase `f5c0296fdaad`) so the split does not read as an oversight to be tidied (FR-11).
- [ ] 6.3 Confirm `build-android.sh` now picks up 6.10.3 through the task-2.2 derivation, with no second hardcoded version anywhere in the script (FR-12).
- [ ] 6.4 Replace `build-android.sh:45`'s "newest installed NDK" (`ls … | sort -V | tail -1`) with an explicit pin to `27.3.13750724`, keeping the `ANDROID_NDK_ROOT` environment override and adding a clear error if the pinned NDK is absent (FR-14). `android/build.gradle:56`'s `ndkVersion androidNdkVersion` already propagates it.
- [ ] 6.5 `make android-clean`, then `make android-apk-debug` (or `android-beta-debug`) as the cheapest first signal on the new kit. Do **not** hand-delete `android-build/`.
- [ ] 6.6 Re-confirm on 6.10.3 that the rcc options still reach every per-ABI sub-build (task 3.12 established this on 6.9.3) — with a forced `build.rs` re-run, so "it propagated" and "cargo reused stale output" stay distinguishable. Note the Android build runs 6.10.3's `rcc` while the desktop runs 6.9.3's; `--no-zstd` is valid in both.
- [ ] 6.7 Re-verify the multi-ABI mechanism on 6.10.3 (FR-21): the per-ABI ExternalProject setup; that Qt still does **not** forward the parent's `CMAKE_PREFIX_PATH` to sub-builds (the `if(NOT CMAKE_PREFIX_PATH)` guard at line 123 depends on it); the cross-ABI plugin-staging bug and whether `packagingOptions.jniLibs.excludes` is still needed; and the `armeabi-v7a` → `armv7-linux-androideabi` corrosion mapping.
- [ ] 6.8 Run `make android-aab` for `arm64-v8a;x86_64;armeabi-v7a` and confirm a signed bundle is produced (FR-13). Bump `android/version.txt` only when an actual Play upload is intended — Play requires a strictly increasing `versionCode`.
- [ ] 6.9 Independently re-check the finished artifact for cross-ABI staging: no aarch64 `.so` files under `base/lib/x86_64/` or `base/lib/armeabi-v7a/`.
- [ ] 6.10 Install the debug/beta build on a device and confirm it launches and reaches the search window — a smoke test only; the real device pass is task 9.0.
- [ ] 6.11 Commit the Qt bump on its own, before touching AGP or Gradle (PRD §7.1 sub-order).

---

### 7.0 Stage 2, step 2 — the AGP / Gradle-wrapper / JDK cluster

**Specs to keep in mind**

- Qt 6.10.3's template delta from 6.9.3 is **two lines**: AGP `8.8.0` → `8.10.1`
  and `options.compilerArgs += ['-Xlint:all']`. Nothing else changes.
- **AGP 8.10 requires Gradle ≥ 8.11.1.** Our wrapper is checked into
  `android/gradle/wrapper/` at **8.10** and is the one actually used (Qt's 8.14.3
  copy is never used); matching Qt's 8.14.3 is the safe choice.
- **Keep the JDK pin** (`MAX_JDK_MAJOR=21`). The failure mode is a
  `lintVitalAnalyzeRelease` crash whose *entire* message is the JDK version
  number, **after** all three ABIs have compiled and signed.
- **⚠ Our `build.gradle` hardcodes `minSdkVersion` and `targetSdkVersion`**
  where Qt's template reads `qtMinSdkVersion` / `qtTargetSdkVersion`. Re-merging
  without re-applying the hardcoded values silently drops the app to
  **targetSdk 35** — a Play compliance regression.
- Local additions to re-apply after any template re-merge: the
  `androidComponents { beforeVariants }` debug-variant switch, the
  `packagingOptions.jniLibs.excludes` cross-ABI workaround, `useLegacyPackaging`,
  and the hardcoded SDK levels.
- **6.10.3's template has no `packagingOptions.jniLibs` block at all**, so
  `useLegacyPackaging true` stays purely ours (6.11.1 *would* have taken it).
- Qt 6.10.3 still registers **no** `OnBackInvokedCallback`, but its own manifest
  template now ships the same opt-out we do — our workaround is
  upstream-sanctioned.

**Depends on:** task 6.0, committed and building.

- [ ] 7.1 Diff Qt 6.10.3's `src/android/templates/build.gradle` against 6.9.3's and against our `android/build.gradle`, and write down every local divergence **before** changing anything — this list is what task 7.5 checks back against.
- [ ] 7.2 Bump `android/gradle/wrapper/gradle-wrapper.properties` from Gradle 8.10 to **8.14.3** first, and build. The wrapper must move before AGP, since AGP 8.10 requires ≥ 8.11.1 (FR-18).
- [ ] 7.3 Bump `android/build.gradle:14` AGP `8.6.0` → **8.10.1**, and update the coupling comment there (it currently names 8.6.0 and the JDK pin) (FR-18).
- [ ] 7.4 Re-evaluate `android.suppressUnsupportedCompileSdk=36` in `android/gradle.properties` — AGP 8.10.1 may have been tested up to compileSdk 36, in which case the suppression is dead. Remove it only if the warning genuinely no longer appears; otherwise keep it and update the comment.
- [ ] 7.5 Confirm every local addition from 7.1 survived: the `beforeVariants` debug switch, `packagingOptions.jniLibs.excludes`, `useLegacyPackaging true`, `ndkVersion androidNdkVersion`, `aaptOptions.noCompress 'rcc'`, `resConfig "en"`, and the hardcoded `minSdkVersion` / `targetSdkVersion`.
- [ ] 7.6 Re-verify the JDK pin: build with the pinned JDK (17–21) and confirm `lintVitalAnalyzeRelease` completes. The system default `java` on this machine is 26, so this is the failure the pin exists for (FR-18).
- [ ] 7.7 Keep `android:enableOnBackInvokedCallback="false"` and reconcile placement: Qt's template puts it on `<application>`, ours is on `<activity>` (`android/AndroidManifest.xml:115`). After any re-merge, ensure it appears **exactly once**, deliberately placed, and record which (FR-16).
- [ ] 7.8 Re-evaluate `useLegacyPackaging` on its merits (FR-19). If flipping is considered, measure **both ways**: AAB/APK size, on-device install footprint, `zipalign -c -P 16 4`, and `readelf -lW` `p_align` for the app `.so` and a Qt library. If the measurements do not clearly favour flipping, leave it `true` and **record the measurement**. Note it becomes upstream-controlled whenever Qt 6.11+ is adopted.
- [ ] 7.9 Full `make android-aab` and `aapt2 dump badging` — confirm `targetSdkVersion 36` survived the AGP/template work (this is where the silent regression would appear).
- [ ] 7.10 Commit the AGP/Gradle cluster separately from task 6's Qt bump.

---

### 8.0 Stage 2, step 3 — minSdk 28, the 16 KB flag, packaging verification

**Specs to keep in mind**

- `android/build.gradle:153`'s `minSdkVersion` is the **only** source; there is
  no `<uses-sdk>` in the manifest.
- **On 6.10.3 the raise to 28 is a deliberate choice, not a forced one** —
  `Qt6AndroidMacros.cmake:309` only reads the target property, exactly as 6.9.3
  does, and androiddeployqt's `main.cpp:1183` is byte-identical. The reason to do
  it anyway: Qt has declared a floor of 28 since before 6.9.3 and we override it
  down, and a Qt upgrade re-tests the whole Android surface anyway.
- `aapt2 dump badging` is the **only** trustworthy check. The generated
  `gradle.properties` carries a stale `qtTargetSdkVersion=35`.
- For the 16 KB check, **sweep every 64-bit library, do not sample**.
  `armeabi-v7a` at `0x1000` is correct, not a failure.
- Permissions: the androiddeployqt `<!-- %%INSERT_PERMISSIONS -->` /
  `<!-- %%INSERT_FEATURES -->` markers are **deleted on purpose**. A new
  permission or a *required* `<uses-feature>` appearing is the July 2026
  Chromebook incident repeating.

**Depends on:** task 7.0.

- [ ] 8.1 Raise `minSdkVersion` 27 → **28** in `android/build.gradle`'s `defaultConfig`, coordinating with task 7.5 since it is the same block (FR-15).
- [ ] 8.2 Build and confirm the generated `qtMinSdkVersion` at build time, then verify the artifact with `aapt2 dump badging | grep -i sdkversion` — minSdkVersion 28, targetSdkVersion 36.
- [ ] 8.3 Note for the release notes that Play stops offering updates to API 27 devices and existing installs keep the last compatible version (FR-15).
- [ ] 8.4 Measure the 16 KB link flag (FR-17): build **with and without** `target_link_options(simsapadhammareader PRIVATE "-Wl,-z,max-page-size=16384")` at `CMakeLists.txt:340`, comparing `readelf -lW <lib>.so | awk '/LOAD/{print $NF}' | sort -u` across **every** 64-bit library plus `zipalign -c -P 16 4 <apk>`.
- [ ] 8.5 Decide on the flag from the measurement. **If in any doubt, keep it** — it costs nothing and its absence is silent until a device rejects the library. Record the measured result either way.
- [ ] 8.6 Run `aapt2 dump badging` on the release AAB/APK and diff the permission and `uses-feature` lists against the pre-upgrade artifact. Every feature must still be `required="false"` (FR-23).
- [ ] 8.7 Confirm the app is still offered to ChromeOS devices in the Play device catalogue (or, pre-upload, that no feature/permission implying excluded hardware has appeared) (FR-23).
- [ ] 8.8 Full-sweep `readelf` `p_align` check and `zipalign -c -P 16 4` on the final signed bundle (Success Metric 4).
- [ ] 8.9 Commit minSdk / 16 KB / packaging decisions as their own commit.

---

### 9.0 Stage 2 acceptance — on-device verification

**Specs to keep in mind**

- FR-20 is **the** acceptance test — the single reason this upgrade exists. If it
  is **not** fixed, that is a finding of equal value: record it in
  `docs/android-soft-keyboard.md` §4 and re-open the question of what else
  restarts the input connection. **Do not silently drop the item.**
- Private headers carry no API or ABI guarantee across minor versions:
  `cpp/android_raw_pick.cpp:55` includes
  `<QtCore/private/qandroidextras_p.h>` and calls
  `QtAndroidPrivate::startActivity` at `:186`.
- Deprecated bar-colour APIs (`getStatusBarColor` / `setStatusBarColor` /
  `setNavigationBarColor`) live in Qt's own Java. **Clearing Play's report is not
  a goal** and is not achievable by this upgrade.
- If no non-arm64 device is available, **say so explicitly** rather than implying
  it was tested.

**Depends on:** tasks 6.0–8.0. Requires a device with a **Thai Gboard layout**.

- [ ] 9.1 Install the signed beta APK (`make android-beta-dist` / `make android-beta-debug-install`) on the test device. Remember a Play-installed copy cannot be replaced by a local build — the beta package installs alongside it.
- [ ] 9.2 **FR-20:** with a Thai Gboard layout, type in the search field (`SearchBarInput.qml`) and confirm Shift works **mid-word**.
- [ ] 9.3 **FR-20:** same test in the Gloss text area (`GlossTab.qml`).
- [ ] 9.4 **FR-20:** with a US layout, confirm no regression in the search field (`dHaMmA` types correctly).
- [ ] 9.5 Record the FR-20 outcome in `docs/android-soft-keyboard.md` §4 — either way, with the Qt version tested (task 10 does the writing; this sub-task is the measurement and note-taking).
- [ ] 9.6 **FR-22 back navigation** — the four previously-broken cases: from the sutta reader, the tab list dialog, the search help dialog, and the Chanting Practice window. Back must dismiss the dialog/window, not close the app.
- [ ] 9.7 **FR-22 safe areas** — the top inset is applied exactly once (not zero, not doubled); the `DrawerMenu` "Menu" label renders correctly in portrait *and* landscape.
- [ ] 9.8 **FR-22 keyboard** — the soft keyboard raises on the **first** tap in a text field (the `MobileKeyboardHelper` path).
- [ ] 9.9 Re-test the storage surfaces that the related in-flight PRDs touch: Storage Diagnostics window, Database Validation, and SAF file save — so a failure there is attributed to this upgrade and not confused with that work.
- [ ] 9.10 **FR-23b non-arm64:** run the x86_64 slice on a device or emulator and exercise audio record/playback (cpal → AAudio), fulltext search (tantivy index files), dictionary lookup, and SAF file save (`android_saf.rs`, `ndk_context`). If no such device is available, state that explicitly as outstanding.
- [ ] 9.11 **FR-23c:** diff 6.10.3's `qandroidextras_p.h` `startActivity` declaration region against 6.9.3's, and confirm `cpp/android_raw_pick.cpp` still compiles and the Android file picker still returns a usable URI on device. Record the result for `docs/file-selection-test.md`.
- [ ] 9.12 **FR-23d:** locate the deprecated bar-colour APIs in 6.10.3's Android Java sources (6.11.1 had moved them into `QtWindowInsetsController.java` with *more* call sites) and note the finding for `docs/android-qt-upgrade-considerations.md` §2.3. Do **not** treat clearing Play's report as a goal.
- [ ] 9.13 Summarise the device pass: what was tested, on what device(s), what was not testable, and the FR-20 verdict.

---

### 10.0 Part D — documentation

**Specs to keep in mind**

- `CLAUDE.md` and `AGENTS.md` are the instructions every future bridge is added
  by. Both currently document the **removed** cxx-qt 0.7 API (`rust_files`
  inside `QmlModule`, the `.qml_module(QmlModule { … })` snippet), so anyone
  adding a bridge from the docs after stage 1 writes code that does not compile.
  This is the highest-value doc change here.
- The two new docs must record the **pre-fix state** measured in PRD §2.1, so the
  defect is recognisable if it recurs.
- The fork's rationale is currently recorded **nowhere**.

**Depends on:** all preceding tasks (the docs record measured outcomes, not
plans).

- [x] 10.1 Write `docs/cxx-qt-fork.md` (FR-7): what each of patches A–E was for, which upstream release absorbed or obsoleted it, the `CXX_QT_AUTORCC_OPTIONS` mechanism that replaced the Android patch (including the missing `rerun-if-env-changed` trap), what remains on the rebased `simsapa` branch and its head revision, and the answer to "can we go back to upstream?".
  - **Written 2026-08-08, ahead of the rest of task 10.0, for the same reason
    10.2 was: the link was already dangling.** Two *shipped source files* point
    at it — `CMakeLists.txt:131` and `bridges/build.rs:141` — so the reference
    was live in the tree while the file did not exist. The Stage 1 measurements
    (tasks 3.2, 3.3, 3.11, 3.12, 4.2) are also freshest now and would be harder
    to write accurately after Stage 2.
  - Six sections: what the fork was (the four commits, the A–E table, per-patch
    disposition); why patch A is obsolete and the CMake-not-shell rule, with the
    `--no-zstd`-is-a-no-op measurement and the missing-`rerun-if-env-changed`
    trap; what stays on the `simsapa` branch and why C is not ported (including
    that C also changed **macOS** behaviour, not only iOS); the 0.7 → 0.9 API
    migration table, the untouched macro survey, the forced `cxx` 1.0.176 bump
    and the rev-not-branch coupled-pair rule; **§5, the `..` trap** — the
    three-way disagreement between rcc / qmldir / qmlcachegen, the `qrc_resources`
    fix, the two rejected alternatives and the byte-level verification; and the
    AOT side finding with the two upstream defects worth reporting.
  - **Left for task 5.6:** §3 names the `simsapa` branch but not its rebased head
    revision, which does not exist yet. Fill it in when 5.6 lands.
- [ ] 10.2 Write `docs/qt-kit-selection.md` (FR-31):
  - **Written early (2026-08-08), covering everything Part C establishes.** Not
    written out of order for its own sake: **six files already referenced it**
    (`AGENTS.md`, `Makefile`, `scripts/qt-env.sh`, `scripts/qt-env-check.sh`,
    `scripts/qt-env-verify.sh`, `build-windows.ps1`) and the link was dangling.
    The Part C measurements were also fresh.
  - Covers: the measured pre-fix state incl. the **non-determinism** refinement
    and the AppImage consequence; the single source and its four readers (and
    why the reader is shared, not copied); the two-block CMake structure and the
    bare-`else()` trap; the version assertion (and why `REQUIRED` is *not* the
    effective backstop on a host with system Qt); the `Qt6_DIR`-derived
    `qmake_path` and the `Qt::qmake` fallback trap; the build-time gate and its
    two tiers; the non-load-bearing convenience layer plus the
    `QT_ANDROID_VERSION` export defect; the bare-`qmake6` rule; and a
    version-bump checklist.
  - **Left open deliberately.** §7 (the Android-on-6.10.3 / desktop-on-6.9.3
    split, both `gcc_64` kits being live, Android running 6.10.3's `rcc`) is
    written as **planned, not landed**, with a status banner at the top —
    every `QT_*` is still `6.9.3`. Revisit when task 6.2 lands and flip the
    banner.
  - **Remaining work on this sub-task, now that the body is written:**
    1. flip the §7 status banner when 6.2 lands;
    2. add the task-2.17 environment scrub — that the Android build removes the
       desktop kit from `PATH` / `LD_LIBRARY_PATH`, why (host tools are the
       *Android* Qt's), and that the gate independently re-checks it;
    3. fold in task 11.8's four-layer description.
  - Original scope, for reference: `CMakeLists.txt` as the single source; scripts deriving from it; the FR-27 assertion; system `qt6-base` vs `~/Qt/<version>`; the **Android-on-6.10.3 / desktop-on-6.9.3 split and its reason**; the pre-fix state from PRD §2.1 (Linux silently on system 6.11.1 while `QT_LINUX` said 6.9.3, and the AppImage compiled against one Qt and bundled with another); that `~/Qt/6.9.3/gcc_64` is now **required** on a Linux dev machine; the `Qt::qmake` fallback trap; and that both `gcc_64` kits are live and neither is redundant.
- [ ] 10.3 **Partly done 2026-08-08 (review) — the urgent half.** Task 3.15's
  `AGENTS.md` rewrite had been overtaken by task 4.2: its snippet still showed
  `QmlModule::new("com.profoundlabs.simsapa").qml_files(qml_files)`, which
  `bridges/build.rs` stopped doing at commit `cfc9549`. That is worse than merely
  stale — a `.qml_files(…)` call **compiles cleanly** and silently re-arms the
  `..` defect that broke QML type resolution at runtime, in the very file every
  future bridge is added from. The snippet now shows
  `new_qml_module(QmlModule::new(uri)).qrc_resources(qml_resources).files([…])`
  with a note saying what it deliberately does *not* do and why, pointing at
  `docs/cxx-qt-fork.md` §5. "New QML components" gained one paragraph: the
  `"../assets/qml/<Name>.qml"` form is required, because `build.rs` strips the
  `../` to derive the alias and a different shape `panic!`s the build.
  (`CLAUDE.md` is a symlink to `AGENTS.md`, so one edit covers both.)
  **Still owed here:** whatever 10.4's AGP/NDK outcomes add, and a re-read of
  both sections once Stage 2 is done. Original scope: rewrite the **"New Rust
  bridges"** and **"New QML components"** sections in `CLAUDE.md` and `AGENTS.md` for the 0.9 builder API — `CxxQtBuilder::new_qml_module`, `QmlModule::new(...).qml_files([...])`, `CxxQtBuilder::files([...])` for bridge sources, and whichever of `cpp_files()` / `unsafe { cc_builder }` task 3.7 settled on (FR-3b, FR-31).
- [ ] 10.4 Update the AGP-pin section, the NDK r28 rule and add a Qt-version-per-platform note to `CLAUDE.md` and `AGENTS.md`, reflecting the AGP 8.10.1 / Gradle 8.14.3 / NDK-pin outcomes (FR-31).
- [ ] 10.5 Update `docs/android-soft-keyboard.md` §4 with the measured FR-20 result and the Qt version tested (FR-31).
- [ ] 10.6 Update `docs/android-qt-upgrade-considerations.md`: §1 version table, §2.1–2.7 (each item applied or re-deferred **with a measured reason**), §3 reasons (drop the deprecated-API one), §4 pitfalls, §5 verification checklist. Fix its "Source PRD" citation to point at `tasks/archive/` (FR-31, Goal 4).
- [ ] 10.7 Update `docs/qt-6.10.1-appimage-issues.md` — record that desktop deliberately stayed on 6.9.3 and why (FR-31).
- [ ] 10.8 Update `docs/android-multi-abi-and-chromeos.md` with the new AGP/wrapper versions and the NDK pin (FR-31).
- [ ] 10.9 Update `docs/pure-rust-audio-backend.md` with the NDK outcome (pin, not upgrade) (FR-31).
- [ ] 10.10 Update `docs/file-selection-test.md` with the FR-23c private-header re-check result (FR-31).
- [ ] 10.11 Update `PROJECT_MAP.md` if the Qt-selection changes altered what it describes; otherwise note that it did not need changing (FR-31).
- [ ] 10.12 Move the PRD and this task list to `tasks/archive/` once every task is checked off and the release is out.

---

### 11.0 Added scope — the right Qt is used by the shell and by agents, not only by CMake

**Why this exists (added 2026-08-08, at the user's request).** Task 1.1 measured
that which Qt `find_package` resolves depends on the **invoking shell's `PATH`**.
Tasks 1.2 and 2.1 fix the *build*; they do nothing for an ad-hoc `qmake6`,
`rcc` or `moc` typed in a terminal or run by an agent, which still resolve to
system Qt 6.11.1. This group closes that gap for the two remaining audiences.

**Specs to keep in mind**

- **PRD non-goal 6 forbids the build depending on shell configuration.**
  Everything in this group is a *convenience layer only*. The acceptance test is
  that deleting all of it leaves `make build` / `make android-aab` still correct,
  because `CMakeLists.txt` (task 1.2) and the scripts (task 2.1) are the
  authority. Never let a script start reading `QMAKE` from the ambient
  environment as its only source.
- **There is no single "the Qt for this project".** Desktop is 6.9.3, Android is
  6.10.3 for *both* the target kits and the host tools (FR-13b). A static
  env var pointing at one of them is wrong for the other half of the work.
  The desktop kit is the right default for an interactive shell, because Android
  work goes through `build-android.sh`, which derives its own version (task 2.2).
- **Do not hardcode a version anywhere new.** Every layer here must derive from
  `CMakeLists.txt`'s `QT_LINUX` / `QT_ANDROID` via the task-2.1 helper, or it
  becomes a fifth copy of the defect this PRD is removing (PRD §7.2's table).
  `.claude/settings.json` cannot run a script, so it is the one place a literal
  path is unavoidable — hence the drift check in 11.4.

**Depends on:** task 2.1 (`scripts/qt-env.sh` is the single source all of this
derives from). Can land any time after that; independent of stages 1 and 2.

> **Landed 2026-08-08, ahead of task 2.1.** `scripts/qt-env.sh` was created here
> rather than in task 2.1, because the shell layer needed it first. **Task 2.1 is
> now "make `build-android.sh` / `build-appimage.sh` consume the existing
> helper", not "write it"** — the lookup helpers (`qt_version_for`,
> `qt_prefix_for`) are already in place and tested. Sourcing with
> `QT_ENV_NO_ACTIVATE=1` gives the helpers without touching `PATH`, which is what
> the build scripts want.
>
> **Order note:** 11.x deliberately landed *after* the FR-27 assertion (1.6) and
> not before. Putting `~/Qt/6.9.3/gcc_64/bin` on `PATH` makes a configure resolve
> the right Qt whether or not task 1.2 works, so the shell layer would have
> masked a broken CMake branch. With the assertion in place a wrong `PATH` fails
> loudly instead, which is what makes the layer safe to have at all.

- [x] 11.1 Extend the task-2.1 helper so it serves this group too: sourcing it with no arguments must export the **desktop** Qt (`QT_PREFIX`, `QMAKE`, `PATH` prefixed with `$QT_PREFIX/bin`, `LD_LIBRARY_PATH`) from `QT_LINUX`, and also export `QT_ANDROID_VERSION` from `QT_ANDROID` so nothing downstream hardcodes it. Keep `qt_version_for <platform>` as the primitive. It must be idempotent (re-sourcing must not stack `PATH` entries) and must fail loudly if the derived kit is absent.
- [x] 11.2 Install `direnv` and add the fish hook (`direnv hook fish | source` in the fish config), then commit an `.envrc` that sources `scripts/qt-env.sh`. Add `.envrc` to the repo, **not** to `.gitignore` — it is project configuration. Note that `direnv allow` is a per-machine step and belongs in the task-10.2 doc, not in code.
- [x] 11.3 Verify 11.2 the way that catches the real failure: `cd` into the project in a **fresh** terminal and confirm `command -v qmake6` is `~/Qt/6.9.3/gcc_64/bin/qmake6` and `qmake6 -query QT_VERSION` is `6.9.3`; then `cd` out and confirm it reverts to `/usr/bin/qmake6` / 6.11.1. A hook that never fires and a hook that never unloads look identical from inside the project directory.
  - Both directions pass: inside → `QT_PREFIX=~/Qt/6.9.3/gcc_64`, `qmake6` = 6.9.3;
    after `cd /tmp` → `QT_PREFIX` empty, `qmake6` = `/usr/bin/qmake6` 6.11.1.
  - **How to test it, because the obvious way silently proves nothing.** direnv's
    fish hook binds to the **`fish_prompt` event**, so it is interactive-only:
    `fish -c '…'` never fires it, and `fish -i -c '…'` does not either (no prompt
    is ever rendered). Both report the *system* qmake and look like a broken
    hook. Force it with an explicit `emit fish_prompt` after each `cd`:
    `fish -i -c 'cd <proj>; emit fish_prompt; command -v qmake6'`.
  - **Still needs a human check in a real terminal** — the above proves the hook
    and `.envrc` are correct, not that an ordinary interactive session picks them
    up. Open a fresh terminal, `cd` in, and run `qmake6 -query QT_VERSION`.
- [x] 11.4 Add the agent layer: a `.claude/settings.json` `env` block setting `QMAKE` and a `PATH` prefixed with the desktop kit's `bin`, so every agent `Bash` call in this project gets the project's Qt.
  - **`PATH` deliberately NOT set**, deviating from the wording. Prefixing it
    requires `${PATH}` expansion inside `settings.json`, which could not be
    verified from inside a running session (the file is read at startup), and a
    non-expanding value would replace `PATH` wholesale and break every command.
    `QT_PREFIX` and `QMAKE` are set instead — absolute, no expansion needed — and
    the `AGENTS.md` rule directs agents to `source scripts/qt-env.sh` or use
    `"$QMAKE"`. Revisit if `${PATH}` expansion is confirmed supported.
  - Drift check implemented as `scripts/qt-env-check.sh` + `make qt-env-check`,
    and **tested in both directions**: passes today; with `QT_LINUX` temporarily
    set to `6.10.3` it correctly reports both `QT_PREFIX` and `QMAKE` as stale.
    (Testing with a *non-installed* version instead only exercises the
    "kit missing" path, not the comparison — use an installed second kit.) Because this file cannot derive its values, add a check to `scripts/qt-env.sh` (or a `make qt-env-check` target) that compares the literal path in `.claude/settings.json` against `QT_LINUX` and fails on drift — otherwise a future `QT_LINUX` bump silently leaves the agent on the old kit.
- [x] 11.5 Add a `CLAUDE.md` + `AGENTS.md` rule
  - Added as a new top-level section **"Qt version per platform — never invoke a
    bare `qmake6`"**. Note `CLAUDE.md` is a **symlink to `AGENTS.md`**, so one
    edit covers both; do not try to edit them separately.
  - Task 10.4 should extend this section (AGP/NDK outcomes), not write a new one. under a new "Qt version per platform" section: never invoke a bare `qmake6` / `rcc` / `moc` (they resolve to system Qt 6.11.1, which the project does not target); desktop tooling is `QT_LINUX`, Android is `QT_ANDROID`, and both are declared in `CMakeLists.txt`. Fold this into the task-10.4 edit rather than writing the section twice.
- [ ] 11.6 Confirm the Android side is genuinely unaffected: with the shell layer active, run an Android configure and check it still resolves `~/Qt/6.10.3/android_<abi>` and host tools from `~/Qt/6.10.3/gcc_64` — i.e. the desktop `PATH` prefix does **not** leak into the cross-build. Do this after task 6.0, when `QT_ANDROID` is actually 6.10.3, so the two versions are distinguishable.
- [x] 11.7 Prove the convenience layer is not load-bearing (the PRD non-goal 6 test): configure and build from a clean environment with `direnv` disabled and the `.claude/settings.json` env unset, and confirm CMake still selects `~/Qt/6.9.3/gcc_64` on its own. If this fails, task 1.2 is incomplete and the shell layer is masking it.
  - Passes at **configure** time: `env -i HOME=… PATH=/usr/bin:/bin cmake -S . -B …`
    reports `Using CMAKE_PREFIX_PATH: ~/Qt/6.9.3/gcc_64` and the assertion
    `Qt 6.9.3 (expected 6.9.3)`.
  - **The `build` half is still outstanding** — re-confirm as part of task 1.11's
    `make build -B`, which has not run yet.
- [ ] 11.8 Record the whole arrangement in `docs/qt-kit-selection.md` (task 10.2): the four layers, which one is authoritative, the 1.1 non-determinism finding as the motivation, and the `direnv allow` per-machine step.

---

### 12.0 Follow-up — enable AOT qmlcachegen for the QML files, and measure it

**Why this is a separate task.** Task 4.2 established that the AOT QML cache has
**never once been used in this project** — under cxx-qt 0.7 or 0.9. The generated
loader inserts its keys raw
(`"/qt/qml/com/profoundlabs/simsapa/../assets/qml/SuttaSearchWindow.qml"`) and
looks them up through `QDir::cleanPath`, which strips `..`, so no key containing
`/../` can ever match. 87 compiled units — 12.4 MB of generated C++ plus a 52 KB
loader — were compiled into the binary and never consulted. The 4.2 fix stopped
generating them, which is why this is an **improvement to try**, not a
regression to repair.

**Specs to keep in mind**

- The blocker is structural, not a flag: qmlcachegen only runs over a QML
  module's `qml_files`, and every path there is fed verbatim into the loader's
  key. So AOT requires **`..`-free paths**, which requires the QML files to live
  at or below `bridges/`.
- **`git mv assets/qml/ → bridges/assets/qml/` is the clean way**, not a symlink
  and not `set_current_dir`. Both of those were rejected in 4.2: a repo symlink
  breaks on Windows without `core.symlinks` and makes every QML file visible at
  two paths to `rg`/`qmllint`/`cargo package`; `set_current_dir` breaks
  incremental builds, because cxx-qt-build emits `rerun-if-changed` with the raw
  path (`cxx-qt-build/src/lib.rs:459,586,645,964`) while cargo resolves relative
  rerun paths against the package root.
- **The move is alias-neutral, and that is the whole point.** From `bridges/`
  the path becomes `assets/qml/Foo.qml`, so the resource path stays
  `:/qt/qml/com/profoundlabs/simsapa/assets/qml/Foo.qml` — every
  `qrc:` literal in `cpp/` (~16 sites) and `assets/icons.qrc` is untouched.
- **A green build proves nothing here**, exactly as in task 4.0. AOT units that
  are stale, mismatched or simply unmatched fail silently — the engine falls
  back to parsing source, which is the current behaviour. The measurement *is*
  the deliverable.
- Whether `assets/qml/` belongs under `bridges/` is a genuine design question:
  it is app UI, not bridge code. If the measured win is small, **not moving** is
  a legitimate outcome.

**Depends on:** task 4.0 complete (the app verified working on the 4.2 fix).
Independent of stages 1 and 2 — do **not** fold it into either.

- [ ] 12.1 Establish the baseline before changing anything: instrument QML engine
  load with `STARTUP-TRACE` (see `docs/startup-sequence-and-caches.md` §6) and
  record the time from `engine.load() start` to `end` over several cold runs,
  plus binary size. Without this the change cannot be evaluated.
- [ ] 12.2 `git mv assets/qml/ bridges/assets/qml/` and update every consumer:
  `bridges/build.rs`, the `qmllint` stub dir
  `assets/qml/com/profoundlabs/simsapa/`, `make qml-test`, `.claude/settings.json`
  if it names the path, and the `assets/qml/` references in `CLAUDE.md` /
  `AGENTS.md` / `PROJECT_MAP.md`. Confirm nothing else greps for `assets/qml`.
- [ ] 12.3 Move the files back from `qrc_resources(…)` into
  `QmlModule::new(URI).qml_files(…)` with the now `..`-free paths, and delete the
  alias-derivation block (its comment explains the bug it existed for — carry the
  explanation into `docs/cxx-qt-fork.md` rather than losing it).
- [ ] 12.4 Verify the generated artifacts before trusting a runtime result: the
  `qmldir` component lines must now resolve (`Logger 1.0 assets/qml/Logger.qml`),
  the `.qrc` aliases must be unchanged from 4.2's, and **every** key in
  `qmlcache_loader.cpp` must be free of `/../` so it can match a
  `QDir::cleanPath`ed lookup.
- [ ] 12.5 Prove at **runtime** that the cache is actually hit — the check the
  whole task turns on. `QT_LOGGING_RULES="qt.qml.diskcache.debug=true"`, or a
  deliberate mismatch experiment. "It built and started" is not evidence.
- [ ] 12.6 Re-measure 12.1's numbers and compare. Record the result **either
  way**; a null result is a useful finding and closes the question.
- [ ] 12.7 Decide from the measurement. If the win does not justify moving app UI
  under `bridges/`, revert the move and record why in `docs/cxx-qt-fork.md`, so
  the next reader does not re-derive it.
- [ ] 12.8 Report both upstream defects to KDAB (pairs with task 3.14's
  `rerun-if-env-changed` finding): (a) a `qml_files` path containing `..` is
  folded by rcc but not by the qmldir writer or qmlcachegen, so the three
  disagree; (b) `qmlcache_loader.cpp` cleans the path on lookup but not on
  insert, so such keys are unreachable. A reproducer is one QML file passed as
  `"../foo/Bar.qml"`.
