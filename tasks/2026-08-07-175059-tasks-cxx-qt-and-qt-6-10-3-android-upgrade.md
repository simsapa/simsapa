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
- `build-macos.sh` / `build-windows.ps1` — hardcoded `6.9.3` paths to derive from `CMakeLists.txt`.
- `bridges/Cargo.toml` — the four cxx-qt crate pins (fork → upstream `2180c12`).
- `bridges/build.rs` — the `QmlModule` literal, the `rust_files` list and the `cc_builder` closure; all three change under the 0.9 API.
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
- `Makefile` — also gains the `qt-env-check` target (11.4).
- `~/.config/fish/conf.d/direnv.fish` — **new, landed** (11.2), outside the repo.
  The direnv shell hook. Note PRD non-goal 6 bars the *build* from depending on
  shell config; this is a convenience layer only, added at the user's request.
- `.envrc` — **new** (task 11.2). direnv hook so an interactive shell entering the
  project gets the desktop Qt on `PATH`.
- `.claude/settings.json` — **new/edited** (task 11.4). `env` block giving agent
  `Bash` calls the project's Qt; the one place a literal path is unavoidable,
  hence the drift check.
- `docs/qt-kit-selection.md` — **new** (FR-31).
- `docs/cxx-qt-fork.md` — **new** (FR-7 / FR-31).
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

### 1.0 Part C, first half — `CMakeLists.txt` as the single source of the Qt kit

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
- [ ] 1.14 Commit Part C's CMake half on its own, with a commit message stating the developer-visible change: a Linux machine without `~/Qt/6.9.3/gcc_64` now gets a `FATAL_ERROR` instead of silently using system Qt.

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

- [ ] 2.1 Write the bash version-reading helper (e.g. `qt_version_for()` reading `set(QT_ANDROID "…")` / `set(QT_LINUX "…")` from `CMakeLists.txt` with `sed -n`), and place it where both `build-android.sh` and `build-appimage.sh` can use it — either a small shared `scripts/qt-version.sh` sourced by both, or duplicated with a cross-reference comment. Choose one and state the reason in the file (FR-30).
- [ ] 2.2 `build-android.sh:25` — `QT_ANDROID_VERSION` default derived from `QT_ANDROID` instead of the hardcoded `6.9.3`, keeping the `${QT_ANDROID_VERSION:-…}` override (FR-12, FR-30). Verify by printing the resolved value at the top of a run.
- [ ] 2.3 `build-appimage.sh` — extract the Qt resolution (lines 187-221) into a `resolve_qt()` helper that sets `qt6_path` and exports `QT_BASE_DIR` / `PATH` / `QMAKE` / `LD_LIBRARY_PATH` / the WebEngine paths, and call it from `main()` **before** `build_app()` (FR-30b). `create_appimage()` then consumes the already-resolved `qt6_path`.
- [ ] 2.4 In the same script, base the search on `QT_LINUX` read from `CMakeLists.txt` rather than the hardcoded `6.9.3`, and remove the `elif command -v qmake6` ambient-`PATH` fallback — replace it with an error naming `QT_BASE_DIR` (FR-30).
- [ ] 2.5 Add the compile-Qt vs bundle-Qt agreement check: after configuring, compare the Qt prefix CMake reports against `qt6_path` and fail loudly on mismatch (FR-30b). FR-27 covers the CMake half; this covers the bundling half, which is a different script's decision.
- [ ] 2.6 Fix `build_app()`'s "already built" short-circuit — for a release build, either drop it entirely or emit a prominent warning that the existing binary's Qt was not verified (FR-30b). Prefer dropping it; `make build -B` is already incremental at the compiler level.
- [ ] 2.7 `build-macos.sh:103-110` — derive the `6.9.3` in the `macdeployqt` search from `QT_MACOS` (FR-30). No version change; this platform is out of scope for behaviour.
- [ ] 2.8 `build-windows.ps1:8,27,107,116,178` — derive the default `-QtPath` and the message strings from `QT_WINDOWS` via a PowerShell one-liner (FR-30). Untestable here; keep the change minimal and mechanical.
- [ ] 2.9 `Makefile:5` — derive the Darwin `QT_PATH` default from `QT_MACOS` (FR-30). Leave the non-Darwin `BUILD_CMD` **without** `-DCMAKE_PREFIX_PATH`: after task 1.2 CMake resolves Linux itself, and passing it here would bypass the new branch.
- [ ] 2.10 Run `make appimage -B` end to end. Verify with `ldd` inside the AppDir that every `libQt6*.so.6` resolves under the bundled Qt (the `strings` half of this check is dropped — see 1.1) (Success Metric 3b).
- [ ] 2.11 Commit Part C's script half separately from 1.14.

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

- [ ] 3.1 Confirm FR-5 still holds: `rustup show` reports ≥ 1.85.0 (currently 1.96.1) with `aarch64-linux-android`, `armv7-linux-androideabi`, `thumbv7neon-linux-androideabi`, `x86_64-linux-android` installed. Note in passing that nothing pins this (no `rust-toolchain.toml`).
- [ ] 3.2 Read the cumulative fork diff `git -C src-lib/cxx-qt-simsapa diff c6710b71 8a597414` (5 files, +69/−7) — **not** commit by commit, since `8a597414` reverts callers added by `73b13685`. Confirm the five changes A–E against the PRD's FR-1 table and note anything that has drifted since the PRD was written.
- [ ] 3.3 Verify upstream's `CXX_QT_AUTORCC_OPTIONS` support in the local checkout: `cxx-qt-build/src/lib.rs:1253-1258` (colon split), `qt-build-utils/src/lib.rs:240,461` (`autorcc_options`), `tool/rcc.rs:42` (`custom_args`). This is what makes patch **A** droppable.
- [ ] 3.4 Point the four crates in `bridges/Cargo.toml:21-23,51` at `https://github.com/KDAB/cxx-qt.git` rev `2180c12`, preserving both feature lists and the "pin a rev, not a branch" comment at line 20 (FR-3).
- [ ] 3.5 Bump `CMakeLists.txt`'s `cxx-qt-cmake` `GIT_TAG` from the **branch** `0.7` to the **tag** `0.9.1` (commit `06a121e`) — not the branch `0.9`, even though they are equal today (FR-4).
- [ ] 3.6 Migrate `bridges/build.rs` to the 0.9 builder API per the table above: `new_qml_module` + `QmlModule::new("com.profoundlabs.simsapa").qml_files(qml_files)`, and the nine bridge files moved to `CxxQtBuilder::files([...])` (FR-3b). Keep the `mobile_build` branch on `CXX_QT_QT_MODULES` unchanged.
- [ ] 3.7 Replace the `cc_builder` closure with the **safe** equivalents rather than wrapping it in `unsafe`: `cc.include("../cpp/")` → `.include_dir("../cpp/")` (`lib.rs:517`), and the three `cc.file(...)` calls → `.cpp_files(["../cpp/utils.cpp", "../cpp/system_palette.cpp", "../cpp/gui.cpp"])` (`lib.rs:641`). No `unsafe` block is needed; only fall back to `unsafe { cc_builder(…) }` if something in the build genuinely requires raw `cc::Build` access, with a comment saying what.
- [ ] 3.8 Build the Rust crate alone first (`cd bridges && cargo build`) to isolate codegen errors from CMake/Qt errors, before any full `make build`.
- [ ] 3.9 Check the resource-path derivation, the one runtime-only risk the compiler cannot catch: how 0.9 turns `"../assets/qml/Foo.qml"` into a resource path. The `:/qt/qml/com/profoundlabs/simsapa/…` paths are load-bearing across the codebase. Inspect the generated `qmldir` and the qrc contents under `build/simsapadhammareader/cxxqt/qml_modules/com/profoundlabs/simsapa/` rather than inferring. (The competing-`qmldir` worry is already resolved: the generated files go to the build dir, never the source tree.)
- [ ] 3.10 Add `list(APPEND CMAKE_AUTORCC_OPTIONS --no-zstd)` at `CMakeLists.txt:80-81` — this is the whole of FR-2. cxx-qt-cmake joins the list with `:` and passes it to the bridge crate's cargo run. **Do not export `CXX_QT_AUTORCC_OPTIONS` from `build-android.sh`**; corrosion's `cmake -E env` assignment would override it.
- [ ] 3.11 Prove it took effect: force a rebuild (`touch bridges/build.rs`), then confirm with `cargo build -vv` (or by comparing resource sizes against a build without the flag) that `rcc` really received `--no-zstd`. Without the forced rebuild, "it propagated" and "cargo reused stale output" are indistinguishable — the one way this gets answered wrongly.
- [ ] 3.12 Verify the same on an Android per-ABI sub-build, since that is the platform the flag exists for, and confirm the value is picked up per-ABI without any environment plumbing.
- [ ] 3.13 Comment `CMakeLists.txt:80-81` to record that this single list now feeds **both** `rcc` invocations — CMake's AUTORCC for `assets/icons.qrc` and, via cxx-qt-cmake, the bridge crate's — that `--no-zstd` replaces fork patch A, and that a shell-exported `CXX_QT_AUTORCC_OPTIONS` would be overridden (FR-2).
- [ ] 3.14 Consider filing the missing `cargo::rerun-if-env-changed=CXX_QT_AUTORCC_OPTIONS` upstream — a one-line `println!` in `cxx-qt-build`. Optional, but cheap; record the decision either way.
- [ ] 3.15 Land FR-3 + FR-3b + FR-4 + FR-2 as **one commit**, separate from any Qt change (FR-10). If stage 1 cannot be made to work, stop and reassess rather than stacking the Qt bump on a broken bridge layer.

---

### 4.0 Stage 1 verification — desktop Linux against Qt 6.9.3

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

- [ ] 4.1 Confirm the configure log names `~/Qt/6.9.3/gcc_64` and the FR-27 assertion passed, before running anything else (FR-8's ordering note).
- [ ] 4.2 `make build -B` clean, then `make test` (Rust + QML + JS). Record any failure and classify it as drift or regression.
- [ ] 4.3 Launch the app and open **every** window and dialog in the `qml_files` list — a mis-registered QML file fails only when its screen is first shown. Work down `bridges/build.rs`'s list systematically: the search window, dictionary, gloss, prompts, bookmarks, chanting practice + review, library, storage dialogs/recovery, dictionaries window and its import/edit dialogs, settings, models/system-prompts dialogs, about, database validation, storage diagnostics, search help, update notification, keybinding capture (FR-9).
- [ ] 4.4 Verify the resource layer explicitly: `:/qt/qml/com/profoundlabs/simsapa/…` paths still resolve, the `Logger` works, and the bridge singletons (`SuttaBridge`, `AssetManager`, `StorageManager`, `PromptManager`, `ClipboardManager`, `DictionaryManager`, `AudioManager`, `GlobalHotkeyManager`, `api`) are reachable from QML (FR-9).
- [ ] 4.5 Run `qmllint` (or `make qml-test`) and confirm the hand-maintained stub `qmldir` + type stubs still resolve. 0.9 also exports a generated `qmldir` / `plugin.qmltypes` under `build/…/cxxqt/qml_modules/` for qmllint/qmlls; both may now be visible, so check that `qmllint` is not reporting a duplicate or conflicting module definition (follows 3.9).
- [ ] 4.6 Exercise a representative slice of `#[qinvokable]` surface at runtime rather than only opening windows: run a search in each area, open a sutta, run a dictionary lookup, gloss a paragraph, save a file, start and stop a recording.
- [ ] 4.7 If a desktop platform breaks and cannot be fixed against upstream, fall back to the **rebased `simsapa` branch** from task 5.0 — not to the old 0.7.2 pin (FR-3's stated fallback).

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

- [ ] 6.1 Confirm the FR-13b preconditions: `~/Qt/6.9.3` and `~/Qt/6.10.3` both present with their `gcc_64` kits, and 6.10.3's `android_arm64_v8a`, `android_x86_64`, `android_armv7` kits installed. Confirm FR-13c is still true (no `~/Qt/6.10.1`, no `~/Qt/6.8.3`).
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

- [ ] 10.1 Write `docs/cxx-qt-fork.md` (FR-7): what each of patches A–E was for, which upstream release absorbed or obsoleted it, the `CXX_QT_AUTORCC_OPTIONS` mechanism that replaced the Android patch (including the missing `rerun-if-env-changed` trap), what remains on the rebased `simsapa` branch and its head revision, and the answer to "can we go back to upstream?".
- [ ] 10.2 Write `docs/qt-kit-selection.md` (FR-31): `CMakeLists.txt` as the single source; scripts deriving from it; the FR-27 assertion; system `qt6-base` vs `~/Qt/<version>`; the **Android-on-6.10.3 / desktop-on-6.9.3 split and its reason**; the pre-fix state from PRD §2.1 (Linux silently on system 6.11.1 while `QT_LINUX` said 6.9.3, and the AppImage compiled against one Qt and bundled with another); that `~/Qt/6.9.3/gcc_64` is now **required** on a Linux dev machine; the `Qt::qmake` fallback trap; and that both `gcc_64` kits are live and neither is redundant.
- [ ] 10.3 Rewrite the **"New Rust bridges"** and **"New QML components"** sections in `CLAUDE.md` and `AGENTS.md` for the 0.9 builder API — `CxxQtBuilder::new_qml_module`, `QmlModule::new(...).qml_files([...])`, `CxxQtBuilder::files([...])` for bridge sources, and whichever of `cpp_files()` / `unsafe { cc_builder }` task 3.7 settled on (FR-3b, FR-31).
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
