# PRD: Fix the macOS build — CoreAudio/AudioToolbox linking and deployment target

- **Date:** 2026-08-05
- **Status:** Draft (not yet implemented)
- **Source report:** `../feedback-and-bug-reports/macos-build-error.txt` — note this
  is **outside the git repository** (a sibling of `simsapa/` in the
  `simsapa-ng-project/` working tree), so it is not version-controlled and will
  not be present in a fresh clone.

## 1. Introduction / Overview

`make macos -B` currently fails. All C++ and Rust compilation succeeds; the build
dies at 100% while linking the app binary:

```
Undefined symbols for architecture arm64:
  "_AudioComponentFindNext", referenced from:
      coreaudio::audio_unit::AudioUnit::new_with_flags_uninitialized … in libsimsapa_bridges.a
  "_AudioObjectGetPropertyData", referenced from:
      cpal::host::coreaudio::macos::device::set_sample_rate … in libsimsapa_bridges.a
  … (19 symbols total)
ld: symbol(s) not found for architecture arm64
```

**Root cause.** Every undefined symbol belongs to a macOS system framework
(`CoreAudio` or `AudioToolbox`) and is referenced from the Rust static library,
by the `cpal` / `coreaudio-rs` crates of the pure-Rust audio backend (see
[docs/pure-rust-audio-backend.md](../docs/pure-rust-audio-backend.md)).

When a Rust crate is compiled to a **staticlib**, its `#[link(kind =
"framework")]` / `cargo:rustc-link-lib` directives are *not* baked into the
`.a` file — they are requirements that the **final** linker must satisfy. That
final link is performed by CMake, so `CMakeLists.txt` has to name the
frameworks explicitly.

`CMakeLists.txt:413-421` links `SystemConfiguration`, `Carbon`, `AppKit` and
`ApplicationServices` — but no audio framework. The Linux branch at
`CMakeLists.txt:432-444` does exactly the equivalent job for cpal's ALSA
backend, with a comment saying so. **macOS never received its counterpart when
Qt Multimedia was replaced by the pure-Rust audio stack.** Because the project's
day-to-day development happens on Linux, the omission stayed invisible until a
macOS build was attempted.

**Second, pre-existing defect surfaced by the same log.** Before the errors, the
linker emits 18 deployment-target warnings — **16** of the dylib form below, plus
**2** of an object-file form (`object file (…QDarwinMicrophonePermissionPlugin…)
was built for newer 'macOS' version (12.0) than being linked (11.0)`, log lines
7-8):

```
ld: warning: building for macOS-11.0, but linking with dylib
'@rpath/QtCore.framework/Versions/A/QtCore' which was built for newer version 12.0
```

The project declares a minimum of macOS **11.0** in two places
(`CMakeLists.txt:58`, `build-macos.sh:183-184`) while every Qt 6.9.3 framework
it links — and the `QDarwinMicrophonePermissionPlugin` it statically imports —
requires **12.0**. Worse, cpal 0.18 references symbols requiring macOS **14.2**
(§4, FR-4). The macOS 11 support the app advertises is therefore not real: an
old-macOS user would get a dyld failure at launch, not a working app. The
maintainer has confirmed the minimum may be raised as needed, so this PRD sets
the declared minimum to whatever the binary genuinely requires.

**Goal:** make the macOS build link, produce a working bundle whose declared
minimum macOS version is honest, and add a documented rule so the next Rust
dependency that needs a system library does not reproduce this failure on a
platform nobody develops on.

## 2. Goals

1. `make macos -B` completes and produces a working `.app` bundle.
2. Chanting-practice recording and playback work on macOS via the pure-Rust
   audio backend.
3. The linker emits no deployment-target mismatch warnings.
4. The declared minimum macOS version is **derived from the symbols the binary
   actually references**, and is stated identically in the build system and in
   the bundle's `Info.plist`.
5. A written rule and a dependency audit make this class of build break
   detectable without owning every platform.

## 3. User Stories

- **As a macOS user,** I want to download and run Simsapa, so that I can read
  suttas and use the Pāli tools on my Mac. Today no macOS build can be produced
  at all.
- **As a macOS user practising chanting,** I want to record and play back my
  recitation, so that I can compare it with the reference audio.
- **As a macOS user on an older OS release,** I want the app either to run or to
  be clearly labelled as requiring a newer macOS — not to install and then fail
  to launch with a dyld error.
- **As the maintainer,** I want a build break caused by a new Rust dependency to
  be caught by a documented checklist step, so that I do not discover it only
  when I next sit down at the Mac.

## 4. Functional Requirements

### Part A — Make macOS link

**FR-1.** The macOS link step must supply the audio system frameworks that the
Rust static library references. Add them to the existing
`if(APPLE AND NOT IOS)` block at `CMakeLists.txt:413-421`, alongside the
frameworks already there:

- `CoreAudio` — provides `AudioObjectGetPropertyData`,
  `AudioObjectGetPropertyDataSize`, `AudioObjectSetPropertyData`,
  `AudioObjectAddPropertyListener`, `AudioObjectRemovePropertyListener`,
  `AudioHardwareCreateAggregateDevice`, `AudioHardwareDestroyAggregateDevice`,
  `AudioHardwareCreateProcessTap`, `AudioHardwareDestroyProcessTap`.
- `AudioToolbox` — provides `AudioComponentFindNext`,
  `AudioComponentInstanceNew`, `AudioComponentInstanceDispose`,
  `AudioUnitInitialize`, `AudioUnitUninitialize`, `AudioUnitGetProperty`,
  `AudioUnitSetProperty`, `AudioUnitRender`, `AudioOutputUnitStart`,
  `AudioOutputUnitStop`. (On modern macOS SDKs the old `AudioUnit` framework is
  an umbrella re-exporting `AudioToolbox`; linking `AudioToolbox` is the correct
  modern form. If any `AudioUnit*` symbol remains unresolved, add
  `-framework AudioUnit` as well and record why in a comment.)

**FR-2.** The new entries must carry a comment explaining *why* they are needed
— that Rust staticlib framework directives are not transitive and the final
CMake link must name them — mirroring the wording of the Linux/ALSA block at
`CMakeLists.txt:430-431`. A future reader must not be able to mistake these for
redundant lines and delete them.

**FR-3.** The change must be confined to the macOS configuration
(`APPLE AND NOT IOS`). The `elseif (IOS)` branch is **not** touched — see §5.

### Part B — Set an honest minimum macOS version

**FR-4 — determine the floor from the binary, not from a guess.** Two separate
constraints push the minimum above the currently declared 11.0:

- **Qt 6.9.3** ships macOS frameworks built for **12.0** (this is what all 18
  linker warnings report).
- **cpal 0.18** hard-references `AudioHardwareCreateProcessTap` /
  `AudioHardwareDestroyProcessTap` from its macOS loopback module
  (`src/host/coreaudio/macos/loopback.rs`), introduced in macOS **14.2**, along
  with the `CATapDescription` / `kAudioAggregateDeviceTap*` API of the same
  vintage. Rust `extern` declarations carry no availability attributes, so —
  unlike a C caller compiled against the SDK headers — the compiler cannot emit
  these as *weak* references. They are expected to be **strong** dyld bindings,
  which means the app would fail to **launch** on any macOS older than 14.2,
  regardless of the fact that Simsapa never calls loopback capture.

The implementer must therefore establish the real floor from the built binary:

- **FR-4a.** Inspect the imported symbols and their binding strength, e.g.
  `nm -m simsapadhammareader.app/Contents/MacOS/simsapadhammareader | grep -iE 'processtap|CATap'`
  and `dyld_info -imports …`. Record whether the ProcessTap symbols are strong
  (`undefined external`) or weak (`weak external`).
- **FR-4b.** Set the deployment target to the highest requirement found.
  **Expected outcome: `14.2`** (strong bindings → the 14.2 API is mandatory).
  If — and only if — FR-4a shows the ProcessTap symbols are weakly bound, `12.0`
  is sufficient and is preferable, since it supports more users.
- **FR-4c.** Also check that no symbol *newer* than the chosen floor is imported
  (a later cpal/objc2 API would push the floor higher again). The check must be
  general, not limited to the two symbols named above.

**FR-5.** Apply the value chosen in FR-4b at **both** sites, which must always
hold the same number:

- `CMakeLists.txt:58` — `CMAKE_OSX_DEPLOYMENT_TARGET` (controls what the binary
  can actually run on).
- `build-macos.sh:183-184` — `LSMinimumSystemVersion` (controls what the Finder
  and the installer tell the user).

Add a short comment at each site pointing at the other, and stating that the
floor is driven by Qt 6.9.3 (12.0) and cpal's CoreAudio ProcessTap API (14.2),
so a future reader knows what to re-check rather than lowering it speculatively.

**FR-6.** After the change, the build log must be free of
`building for macOS-… but linking with dylib … built for newer version …`
warnings. Their absence is the acceptance check for FR-4/FR-5.

**FR-7.** Because the floor is being raised by three or four major releases, the
change must be user-visible where the requirement is stated: check `README.md`
and any release-notes or download-page text for a "macOS 11+" style claim and
correct it. If no such claim exists, note that it was checked.

### Part C — Verification on the macOS machine

**FR-8.** On the macOS Tahoe 26.5.2 build machine, verify in this order:

1. `make macos-rebuild` completes with exit status 0 and no linker warnings (a
   genuinely clean build — see §7).
2. The produced `.app` launches; the sutta reader and search work (confirms the
   framework additions did not disturb the Qt/WebEngine link).
3. **Chanting practice: record a short clip and play it back.** This is the
   functional test of the audio backend and the real point of the frameworks —
   a successful link alone does not prove the audio path works.
4. The microphone permission prompt appears on first recording (the
   `QDarwinMicrophonePermissionPlugin` import at `CMakeLists.txt:459-463` and
   `NSMicrophoneUsageDescription` at `build-macos.sh:198-200` are already in
   place; confirm the deployment-target change did not affect them).
5. The same build also produces the `.dmg`, and the app launches from a copy
   installed out of the `.dmg` (not just from the build tree).
6. `plutil -p …/Contents/Info.plist | grep -i minimum` reports the FR-4b value.

**FR-9.** The test machine runs macOS 26.5.2, so it **cannot** validate behaviour
on any older macOS — in particular it cannot detect the FR-4 launch failure,
because every symbol in question is present on 26.5.2. This limitation must be
stated explicitly in the task list and in the doc update, so it is not mistaken
for tested behaviour; FR-4a's binary inspection is the substitute evidence and is
required for exactly that reason. (This mirrors the existing note in
[docs/android-edge-to-edge-and-safe-areas.md](../docs/android-edge-to-edge-and-safe-areas.md)
that an Android 15 phone cannot validate the edge-to-edge work.)

### Part D — Prevention

**FR-10.** Audit the remaining *active* platform link configurations in
`CMakeLists.txt` for the same gap — a Rust dependency needing a system library
that the final link does not name:

- **Windows** (`CMakeLists.txt:447-452`): only `bcrypt` is linked. Confirm
  whether cpal's WASAPI backend needs anything further (`ole32`, `mmdevapi`). A
  successful Windows build is evidence enough; no change if it links.
- **Android**: cpal's AAudio backend goes through the `ndk` crate against system
  libraries; the app builds and runs today, so this is a read-only confirmation.

iOS is excluded from the audit — it is dormant and out of scope (§5). Record the
outcome of each check; an audit that finds nothing must say it looked.

**FR-11.** Add a rule to `CLAUDE.md`, as a new subsection under *Specific coding
procedures*, stating: **when adding a Rust dependency that binds to a native
system library, check every platform's link configuration in `CMakeLists.txt`
and add the matching `target_link_libraries` entry where one is needed — and
check whether the crate selects a *different* backend module per platform,
because the libraries then differ too.** Cite this incident and the ALSA block as
the worked examples, and note that the failure appears only on the platform you
are not building on.

The rule must state **why it is per-platform rather than universal**, or a reader
will apply it to Windows and be confused when it is unnecessary there: `#[link]`
/ `cargo:rustc-link-lib` directives are not recorded in a Mach-O or ELF `.a`, so
macOS and Linux need the explicit CMake entry — whereas on Windows/MSVC rustc
emits `/DEFAULTLIB:` linker directives **into the COFF object files**, which the
linker honours automatically. That asymmetry is the whole reason `bcrypt` is the
only manual Windows entry while Linux needs the ALSA block (and is what FR-10
should confirm).

**FR-12.** Update `docs/pure-rust-audio-backend.md` with a per-platform "what
the final link must supply" table (macOS: CoreAudio + AudioToolbox; Linux:
libasound; Android: nothing, AAudio is a system lib; Windows: as found by
FR-10), plus the FR-4 minimum-version finding and its reasoning.

**FR-13 — the human substitute for CI.** There is no hosted CI; every macOS
build is made by hand on the on-site Mac. So the only thing that can catch a
recurrence is a person remembering to build. Record, in the same place as the
FR-11 rule, that **a macOS build must be produced on the on-site Mac before any
release** — because the platform is not built during ordinary Linux development,
a link break there can sit undetected for months (as this one did, from the Qt
Multimedia removal until now). Keep this to a couple of sentences next to the
FR-11 rule; do not invent a broader release-checklist document as part of this
task.

## 5. Non-Goals (Out of Scope)

- **Anything to do with the iOS target.** iOS is dormant, not merely broken:
  commit `0783a93` (2025-07-26) is titled *"update cxx-qt to 0.7.2, not building
  for iOS for now"*, and the `elseif (IOS)` branch at `CMakeLists.txt:152-156`
  sets only `Rust_CARGO_TARGET`, a qmake path and `Qt::WebView` — there is no
  `Makefile` target, no `build-ios.sh`, no iOS deployment target and no
  Info.plist step. Its link configuration has the *same* missing-framework gap,
  but the fix differs from the macOS one and cannot be verified without a
  buildable target, so it is deliberately left alone here. The finding is
  recorded for whoever revives iOS: cpal selects a **separate** iOS module
  (`src/host/coreaudio/ios/`, via `#[cfg(not(target_os = "macos"))]` in
  `src/host/coreaudio/mod.rs`) built on RemoteIO + `AVAudioSession`, importing
  `objc2_audio_toolbox` and `objc2_avf_audio`, and never touching the macOS HAL
  — so iOS needs `AudioToolbox` + `AVFAudio` (or `AVFoundation`) and **not**
  `CoreAudio`. `backend/src/audio/` has no platform `cfg` gates, so those symbols
  really will be referenced from an iOS build.
- Upgrading Qt, cpal, or any other dependency.
- Replacing or redesigning the audio backend.
- Adding system-audio **loopback** capture as a feature. The loopback code is
  merely present in cpal and is the source of the 14.2 symbols; Simsapa does not
  call it and will not start doing so here.
- Removing the 14.2 requirement by patching or forking cpal to drop its loopback
  module. This would allow a lower floor, but it adds a per-upgrade maintenance
  burden; the maintainer has accepted raising the minimum instead. Worth
  revisiting only if supporting macOS 12/13 becomes important.
- Code signing, notarization, or Mac App Store distribution.
- Setting up online CI for macOS. The project has no hosted CI: macOS builds are
  always produced locally on the on-site Mac with `build-macos.sh` / `make
  macos`. Automated detection of this failure class is therefore not available,
  which is precisely why the documented rule in FR-11 and the pre-release build
  step in FR-13 carry the weight instead.
- Intel/x86_64 macOS or a universal binary. The `Rust_CARGO_TARGET` selection at
  `CMakeLists.txt:157-163` already picks per host architecture; the reported
  failure is arm64 and that is what gets fixed and tested.
- Changing the Linux, Windows, or Android **build outputs**. FR-10 is a
  read-only audit; if it finds a real gap, fixing it is a follow-up task.

## 6. Design Considerations

No UI changes. One user-visible consequence: the supported macOS range shrinks
substantially (from a claimed 11.0 to an expected 14.2), so FR-7 covers
correcting any stated requirement. Users on older macOS keep whatever build they
already have; nothing regresses for them, but they will not receive updates.

## 7. Technical Considerations

- **The two-site minimum-version invariant** (FR-5) is the kind of duplication
  that drifts. Prefer making one site the source of truth if it can be done
  cleanly — e.g. `build-macos.sh` reading the value rather than restating it —
  but a comment cross-reference at both sites is an acceptable minimum.
- **`-framework AudioUnit` vs `AudioToolbox`:** the `AudioUnit*` and
  `AudioComponent*` symbols historically lived in the `AudioUnit` framework,
  which current SDKs keep as an umbrella over `AudioToolbox`. Start with
  `AudioToolbox` alone and only add `AudioUnit` if symbols remain unresolved.
- **Why not fix this in Rust instead:** adding
  `println!("cargo:rustc-link-lib=framework=CoreAudio")` to a `build.rs` would
  not help — the directive is still not carried through a staticlib to the CMake
  link. The CMake side is the correct and only place.
- **The `QDarwinMicrophonePermissionPlugin` warnings** (log lines 7-8) are the
  same version mismatch as the Qt framework warnings and should disappear with
  FR-5. If they persist, that is a separate finding worth recording, not
  something to silence.
- **Build-tree staleness — `make macos -B` is *not* a clean build.** The
  deployment target at `CMakeLists.txt:58` is set with `CACHE … FORCE`, so it
  does take effect on reconfigure. But `-B` is GNU make's "always remake the
  named targets" flag: `make macos -B` re-runs `cmake -S . -B` (a reconfigure)
  and `cmake --build`, and **leaves every existing object file and the cached
  Rust staticlib in `./build/simsapadhammareader/`**. `make macos-clean` does not
  help either — it removes only `./dist` and `Simsapa-*.dmg`, not the build dir.
  C++ objects should rebuild when the `-mmacosx-version-min` flag changes, but
  **cargo does not reliably invalidate its cache on a `MACOSX_DEPLOYMENT_TARGET`
  change**, so `libsimsapa_bridges.a` can survive with the old target baked in —
  precisely the artifact whose symbols FR-4a inspects. Use **`make
  macos-rebuild`** (which `rm -rf`s the build dir via `build-macos.sh --clean`)
  for the FR-6 warning check and the FR-8 verification build.
- **Qt upgrade coupling:** both floors in play (Qt's 12.0 and cpal's 14.2) can
  move with a dependency upgrade. A line in the upgrade-considerations doc keeps
  this from being rediscovered — see §9.

## 8. Success Metrics

1. `make macos-rebuild` exits 0 on the macOS Tahoe machine, from a clean build
   directory (see §7 — `make macos -B` reconfigures but does **not** clean).
2. Zero `ld: warning:` lines in the macOS build log.
3. A chanting-practice recording is captured and played back successfully in the
   built `.app` (FR-8.3).
4. The `nm -m` / `dyld_info` output is recorded in the task notes, and the
   deployment target and `LSMinimumSystemVersion` both equal the floor that
   output implies.
5. `CLAUDE.md` and `docs/pure-rust-audio-backend.md` carry the new rule and the
   per-platform link table; `PROJECT_MAP.md` updated if any file's role changed.
6. The FR-10 audit result is written down for Windows and Android.

## 9. Open Questions

1. **Is 14.2 the final answer for the macOS floor?** FR-4a settles it from the
   binary. This must be done early in implementation, because it determines the
   value written at two sites and in the README.
2. Should the macOS minimum-version coupling be recorded alongside the existing
   Android upgrade notes — either in
   [docs/android-qt-upgrade-considerations.md](../docs/android-qt-upgrade-considerations.md)
   or in a new Apple-platform equivalent? The next Qt upgrade may raise Qt's own
   floor again, and this PRD's reasoning is exactly what a future reader needs.

*(A third question — whether to add macOS CI — was resolved rather than left
open: there is no hosted CI and macOS builds are always made locally on the
on-site Mac. FR-13 records the manual pre-release build step that takes its
place.)*
