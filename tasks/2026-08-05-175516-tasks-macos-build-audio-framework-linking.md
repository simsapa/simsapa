# Tasks: Fix the macOS build — CoreAudio/AudioToolbox linking and deployment target

Source PRD: [2026-08-05-175516-prd---macos-build-audio-framework-linking.md](./2026-08-05-175516-prd---macos-build-audio-framework-linking.md)

## Review findings (2026-08-05)

The PRD and this task list were reviewed against the codebase, the build log and
the cpal 0.18.1 source. Six corrections were applied to the PRD; the rest of it
verified clean.

| # | Finding | Where fixed |
|---|---|---|
| R1 | The source report path is `../feedback-and-bug-reports/…` — the file is **outside the git repo** and untracked. The PRD's path did not resolve from the repo root. | PRD header, Relevant Files, task 1.1 |
| R2 | **19** undefined symbols, not 17. The PRD's own FR-1 lists 9 + 10 = 19, contradicting its prose. | PRD §1, task 1.1, task 1.9 |
| R3 | The 18 warnings are **16 dylib-form + 2 object-file-form**, not 18 of one form. The two object-file ones are the mic-permission plugin — the pair FR-6 most needs to see disappear. | PRD §1, task 3.6 |
| R4 | **`make macos -B` is not a clean build** — it reconfigures but deletes nothing, and `make macos-clean` removes only `dist/` and the dmg. Since cargo does not reliably invalidate on a `MACOSX_DEPLOYMENT_TARGET` change, a stale `libsimsapa_bridges.a` could make the FR-6 check pass for the wrong reason. **`make macos-rebuild` is the correct command.** | PRD §7 / §8.1 / FR-8.1, stage 3 spec, tasks 3.5, 5.1 |
| R5 | FR-11's rule as worded would have a reader add CMake entries on Windows too, where they are unnecessary: rustc emits `/DEFAULTLIB:` into COFF objects, while Mach-O/ELF `.a` archives carry nothing. That asymmetry *is* the rule's substance. | PRD FR-11, stage 4 spec, task 4.2, stage 6 spec, task 6.1 |
| R6 | Stage 1 could in principle block stage 2 if the link fails on symbol *availability* rather than absence. Unlikely (`.tbd` files carry no per-symbol introduction version) but it inverts the stage order, so it is called out rather than left to be misdiagnosed as a missing framework. | task 1.8, stage 2 spec |

Verified and **confirmed correct** in the PRD: every `CMakeLists.txt` line
reference (58, 152-156, 410-421, 430-431, 447-452, 459-463), both
`build-macos.sh` references (183-184, 198-200), the symbol→framework split, the
claim that cpal's loopback module is unconditional with no feature to disable it
(so §5's "don't fork cpal" non-goal is the right call), and that
`backend/src/audio/` has no platform `cfg` gates.

**One prediction to hold loosely:** the PRD expects the ProcessTap symbols to be
strongly bound (→ 14.2). Review supports that expectation but it remains
unverified until task 2.2 runs on the Mac — that is the whole point of stage 2
being its own stage.

## Component analysis

Technical components implied by the PRD, and what blocks what:

| # | Component | PRD requirements | Depends on |
|---|---|---|---|
| C1 | macOS link config in `CMakeLists.txt` (`APPLE AND NOT IOS` block) | FR-1, FR-2, FR-3 | — |
| C2 | Binary symbol inspection on the Mac (`nm -m`, `dyld_info -imports`) | FR-4a, FR-4c | C1 (needs a binary that links) |
| C3 | Deployment target `CMAKE_OSX_DEPLOYMENT_TARGET` | FR-4b, FR-5 | C2 |
| C4 | `LSMinimumSystemVersion` in `build-macos.sh` Info.plist step | FR-5 | C2, C3 (must equal C3) |
| C5 | User-facing version claims (README / release notes / download text) | FR-7 | C3 |
| C6 | On-machine functional verification (launch, search, chanting record/playback, mic permission, DMG install, plist check) | FR-6, FR-8, FR-9 | C1, C3, C4 |
| C7 | Cross-platform link audit (Windows, Android; iOS excluded) | FR-10 | — (read-only; independent) |
| C8 | Prevention rule in `CLAUDE.md` + pre-release macOS build note | FR-11, FR-13 | C1 (worked example), C7 |
| C9 | `docs/pure-rust-audio-backend.md` per-platform link table + minimum-version finding | FR-12, FR-9 | C2, C3, C7 |
| C10 | `PROJECT_MAP.md` / Qt-upgrade-coupling note | Success metric 5, §9 Q2 | C9 |

Requirement coverage check: FR-1→C1, FR-2→C1, FR-3→C1, FR-4→C2/C3, FR-5→C3+C4,
FR-6→C6, FR-7→C5, FR-8→C6, FR-9→C6+C9, FR-10→C7, FR-11→C8, FR-12→C9, FR-13→C8.
Every functional requirement is covered.

**Ordering constraint that shapes the stages:** FR-4a cannot be answered without
a binary, and the binary cannot be produced without FR-1. So the link fix ships
first (stage 1), the measurement and the version change follow (stage 2), and
only then can the build log be checked warning-free (stage 3).

## Current state (assessed)

- `CMakeLists.txt:410-421` — the comment + `if(APPLE AND NOT IOS)` block linking
  `SystemConfiguration`, `Carbon`, `AppKit`, `ApplicationServices`. No audio
  framework. This is the file to edit for FR-1.
- `CMakeLists.txt:430-444` — the Linux ALSA block is the pattern to mirror,
  including its "must be resolved at the final link" comment wording (FR-2).
- `CMakeLists.txt:447-452` — the Windows block links only `bcrypt` (FR-10).
- `CMakeLists.txt:58` — `set(CMAKE_OSX_DEPLOYMENT_TARGET "11.0" CACHE STRING …
  FORCE)`, preceded by a comment claiming it is set "to match the SDK being used
  … prevents version mismatch warnings" — a claim the build log disproves; that
  comment needs replacing too (FR-5).
- `CMakeLists.txt:152-156` — the dormant `elseif (IOS)` branch. Out of scope.
- `CMakeLists.txt:459-463` — `QDarwinMicrophonePermissionPlugin` import, already
  present; only verified, not changed (FR-8.4).
- `build-macos.sh:183-184` — the two `PlistBuddy` lines setting
  `LSMinimumSystemVersion 11.0`; `NSMicrophoneUsageDescription` is at
  `build-macos.sh:198-200`, already present.
- `Makefile:79-89` — `macos` / `macos-app` / `macos-clean` / `macos-rebuild`;
  `make macos` depends on the `build` target and then runs `./build-macos.sh`.
- `README.md` — mentions macOS only in the build-commands section (lines 47-69);
  a grep found **no** "macOS 11+" style requirement claim, so FR-7 is likely a
  "checked, nothing to change" outcome that still needs recording.
- `backend/Cargo.toml:48` — `cpal = "0.18"`; the resolved version in the
  registry is **0.18.1**.
- **cpal 0.18.1 verified during review** (`~/.cargo/registry/src/*/cpal-0.18.1`):
  - `src/host/coreaudio/macos/loopback.rs` exists and really does reference
    `AudioHardwareCreateProcessTap` (line 115) / `AudioHardwareDestroyProcessTap`
    (line 149) / `CATapDescription` (line 96), imported from **`objc2-core-audio`
    0.3**.
  - `src/host/coreaudio/macos/mod.rs:23` declares `mod loopback;`
    **unconditionally** — there is no `cfg` and **no cargo feature** that
    disables it (`[features]` has only `asio`, `audioworklet`, `custom`,
    `default`, `jack`, `pipewire`, `pulseaudio`, `realtime*`, `wasm-bindgen`).
    This confirms the PRD §5 finding that patching/forking cpal is the *only*
    alternative to raising the floor — and that it is correctly a non-goal.
  - The `AudioUnit*` / `AudioComponent*` symbols come from a different crate:
    **`coreaudio-rs` 0.14.2**, matching the `libsimsapa_bridges.a[1200](coreaudio-…)`
    object names in the error log. Two crates, two frameworks — which is exactly
    why both `CoreAudio` and `AudioToolbox` are needed and neither alone suffices.
- **The build log contains 19 undefined symbols, not 17** (counted:
  `grep -cE '^ *"_' ../feedback-and-bug-reports/macos-build-error.txt` → 19). The
  PRD's prose said 17 while its own FR-1 lists 9 CoreAudio + 10 AudioToolbox = 19;
  the PRD has been corrected. Use **19** as the completeness check in task 1.1.
- **`make macos -B` is not a clean build.** `Makefile:79-80` — `macos: build` then
  `./build-macos.sh`; `-B` forces the recipes to re-run (so `cmake -S . -B` does
  reconfigure) but **deletes nothing**. `make macos-clean` (`Makefile:85-86`)
  removes only `./dist` and `Simsapa-*.dmg`. Only `build-macos.sh --clean`
  (line 443) does `rm -rf "$BUILD_DIR"`, reached via **`make macos-rebuild`**.
  This matters because cargo does not reliably invalidate on a
  `MACOSX_DEPLOYMENT_TARGET` change, so a stale `libsimsapa_bridges.a` could
  survive — the very artifact task 2.0 inspects.
- `docs/pure-rust-audio-backend.md` — has an "Android link: NDK system
  libraries" section but no macOS/Linux/Windows equivalent; the per-platform
  table (FR-12) slots in beside it.
- `CLAUDE.md` — *Specific coding procedures* already hosts sibling rules of
  exactly this shape ("Adding a Qt module that needs an Android permission");
  the FR-11 rule follows that template.

## Relevant Files

- `CMakeLists.txt` — the only place the fix belongs: the audio frameworks
  (line ~413 block) and `CMAKE_OSX_DEPLOYMENT_TARGET` (line 58).
- `build-macos.sh` — `LSMinimumSystemVersion` in the Info.plist step
  (lines 183-184); must always equal the CMake deployment target.
- `README.md` — checked for a stated macOS version requirement (FR-7).
- `CLAUDE.md` — new *Specific coding procedures* subsection: the
  Rust-dependency/native-library rule and the pre-release macOS build note.
- `docs/pure-rust-audio-backend.md` — per-platform "what the final link must
  supply" table, the minimum-version finding, and the FR-9 limitation note.
- `PROJECT_MAP.md` — updated only if a file's role changed (likely just the doc
  cross-reference).
- `../feedback-and-bug-reports/macos-build-error.txt` — the source log; the
  reference for which symbols must resolve. **Outside the git repository** (a
  sibling of `simsapa/`), 103 lines, not version-controlled.
- `backend/src/audio/` — read-only. **Verified during review:** `format.rs`,
  `mod.rs`, `player.rs`, `recorder.rs` contain **zero** `cfg(target_os …)` gates,
  so the audio code really is compiled into every target and this is genuinely a
  cross-platform question (task 4.1 is confirmation, not discovery).

### Notes

- There is no automated test for this work: the deliverable is a build that
  links and an `.app` that records audio. Verification is manual, on the on-site
  macOS Tahoe 26.5.2 machine. `cargo test` and `make qml-test` are unaffected by
  every change here — none of it touches Rust, QML or C++ source.
- Tasks 1.0, 4.0 and 6.0 are editable on Linux; tasks 2.0, 3.0 and 5.0
  **require the Mac**.
- The test machine cannot validate the older-macOS launch behaviour (FR-9) —
  binary symbol inspection is the substitute evidence, which is why task 2.0
  exists as its own stage rather than being folded into "pick a number".
- **Record command output verbatim** in the task notes as you go (task 2.0 and
  4.0 especially). Success metric 4 requires the `nm -m` / `dyld_info` output to
  be written down, and the FR-10 audit must show it looked.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown
file by changing `- [ ]` to `- [x]`. Update the file after completing each
sub-task, not just after completing an entire parent task.

## Tasks

---

### Stage 1 — make it link

**Spec.** Edit exactly one block: `CMakeLists.txt:413-421`, `if(APPLE AND NOT
IOS)`. Add two `PUBLIC` entries in the same `"-framework X"` string style as the
existing lines:

```cmake
"-framework CoreAudio"
"-framework AudioToolbox"
```

The `elseif (IOS)` branch and the Linux/Windows/Android blocks are **not**
touched (FR-3). Order within the list does not matter — CMake passes them to the
linker and framework link order is not significant here.

The comment above the block (lines 410-412) currently explains only
SystemConfiguration/Carbon/AppKit; extend it to explain the audio frameworks
using the *reason*, not just the name: a Rust **staticlib** does not carry its
`#[link(kind = "framework")]` directives into the `.a`, so the final CMake link
must name them. Mirror the phrasing of the ALSA block at `CMakeLists.txt:430-431`
("references libasound symbols that must be resolved at the final link").

**Depends on:** nothing. **Blocks:** everything else.

- [ ] 1.0 Link the CoreAudio / AudioToolbox frameworks on macOS so the app binary links (FR-1, FR-2, FR-3)
  - [ ] 1.1 Read `../feedback-and-bug-reports/macos-build-error.txt` (**note the `../`** — it lives outside the repo) and list the **19** undefined symbols, grouping each under CoreAudio or AudioToolbox, so the fix can be checked against the actual failure rather than against the PRD's summary. `grep -oE '"_[A-Za-z0-9]+", referenced' … | sort -u` gives the list directly.
  - [ ] 1.2 Read `CMakeLists.txt:405-455` to see the macOS block, the Linux ALSA block and the Windows block together; note the exact comment wording of the ALSA block to mirror.
  - [ ] 1.3 Add `"-framework CoreAudio"` and `"-framework AudioToolbox"` to the `PUBLIC` list in the `if(APPLE AND NOT IOS)` block.
  - [ ] 1.4 Extend the comment above that block to state *why* the frameworks are listed — Rust staticlib framework directives are not transitive, so the final CMake link must name them — and that CoreAudio/AudioToolbox are required by cpal / coreaudio-rs. Make it explicit enough that a future reader cannot mistake the lines for redundancy (FR-2).
  - [ ] 1.5 Confirm the diff touches nothing outside that block: no `elseif (IOS)` change, no Linux/Windows/Android change (FR-3). `git diff CMakeLists.txt` should show one hunk.
  - [ ] 1.6 Verify the change is inert on Linux: `make build -B` still succeeds (the `APPLE` block is not evaluated there). This is the only compile check available off the Mac.
  - [ ] 1.7 **On the Mac:** run `make macos -B` and confirm the link now completes. If any `AudioUnit*` or `AudioComponent*` symbol is *still* unresolved, add `"-framework AudioUnit"` as well and record in the comment why it was needed (FR-1's fallback clause). If nothing remains unresolved, do **not** add it.
  - [ ] 1.8 **Contingency — if the link instead fails with an *availability* error** (e.g. "symbol … not available in macOS 11.0" for the ProcessTap symbols), the stage ordering has to invert: raise `CMAKE_OSX_DEPLOYMENT_TARGET` to 14.2 first (task 3.1), link, and then run stage 2 to confirm the number from the binary. This is expected to be *unlikely* — `.tbd` stub files carry no per-symbol introduction version, so ld normally resolves them without complaint — but it is the one way stage 1 can block stage 2, so recognise it rather than debugging it as a missing framework.
  - [ ] 1.9 Record in this file: the build now links (or the exact remaining symbols and what fixed them), and all 19 symbols from 1.1 are accounted for. Do not proceed to stage 2 until the binary exists.

---

### Stage 2 — measure the real floor

**Spec.** This stage produces a **number**, backed by recorded evidence. It
changes no files.

The binary to inspect is
`build/simsapadhammareader/simsapadhammareader.app/Contents/MacOS/simsapadhammareader`
(confirm the actual path from the build output — `build-macos.sh` may stage it
elsewhere).

Two questions must be answered:

1. **Are the ProcessTap symbols bound strongly or weakly?** `nm -m` prints
   `(undefined [lazy bound]) external` vs `weak external`. A **strong** binding
   means dyld must resolve it at launch on every macOS, so the API's
   introduction version (14.2) becomes a hard floor even though Simsapa never
   calls loopback capture. A **weak** binding means the symbol resolves to null
   on older systems and only matters if called — 12.0 (Qt's floor) suffices.
2. **Is anything else newer than the candidate floor imported?** (FR-4c) The
   check must be general — a later cpal/objc2/Qt API could push the floor higher
   than 14.2. `vtool -show` / `otool -l` on the linked Qt frameworks gives their
   `LC_BUILD_VERSION` minos; `dyld_info -imports` gives the full import list.

Expected outcome per the PRD: **14.2**. Prefer 12.0 **only** if the evidence
shows weak binding.

**Calibration, so the result is not over-read.** `.tbd` stub libraries do not
record a per-symbol introduction version, so ld has nothing to auto-weak-import
from and a plain Rust `extern` reference will almost certainly come out
**strong**. Task 2.2 is therefore expected to *confirm* 14.2 rather than to
discover 12.0 — but it must still be run and its output recorded, because that
recorded output is the substitute for the older-macOS testing this machine
cannot do (FR-9), and because a *weak* result would be a genuinely better
outcome worth catching.

**Depends on:** task 1.0 (a linked binary). **Blocks:** 3.0, 5.0, 6.0.

- [ ] 2.0 Determine the real minimum macOS version from the built binary's imported symbols (FR-4a, FR-4c)
  - [ ] 2.1 **On the Mac:** locate the linked binary inside the `.app` produced by task 1.7 and note its path. Confirm it is *fresh* (`ls -l`, or check it is newer than the `CMakeLists.txt` edit) — a stale binary from before task 1.3 would silently answer the wrong question.
  - [ ] 2.2 Run `nm -m <binary> | grep -iE 'processtap|CATap|AggregateDeviceTap'` and paste the **verbatim** output into this task file. Note for each hit whether it reads `external` (strong) or `weak external`.
  - [ ] 2.3 Run `dyld_info -imports <binary>` and save the full output to a scratch file; grep it for the CoreAudio/AudioToolbox imports and record the count.
  - [ ] 2.4 Determine the introduction version of every imported CoreAudio/AudioToolbox symbol that is not obviously ancient — at minimum the ProcessTap/CATap set. Check the SDK headers (`xcrun --show-sdk-path` → `.../Frameworks/CoreAudio.framework/Headers/`) for `API_AVAILABLE(macos(...))` annotations rather than relying on memory (FR-4c: the check must be general, not limited to the two named symbols).
  - [ ] 2.5 Record the Qt floor independently: run `vtool -show-build-version` (or `otool -l | grep -A4 LC_BUILD_VERSION`) on one of the linked Qt 6.9.3 frameworks and confirm it reports `minos 12.0`, matching the 18 linker warnings.
  - [ ] 2.6 State the conclusion in this file as one line: **the floor is X.Y, because Z** — the highest of (Qt's minos, the newest imported-symbol introduction version). Note explicitly whether the ProcessTap binding was strong or weak, since that is the whole basis for choosing 14.2 over 12.0.
  - [ ] 2.7 If the conclusion is **not** 14.2, stop and flag it: the PRD, the README wording and the doc updates all assume 14.2, and a different number changes what gets written in stage 3 and stage 6.

---

### Stage 3 — apply the floor, honestly, at every site

**Spec.** The chosen value must appear identically at **two** sites, which have
different jobs and drift apart silently if not cross-referenced:

| Site | What it controls |
|---|---|
| `CMakeLists.txt:58` `CMAKE_OSX_DEPLOYMENT_TARGET` | what the binary can actually run on (baked into `LC_BUILD_VERSION`) |
| `build-macos.sh:183-184` `LSMinimumSystemVersion` | what Finder / the installer tell the user before they launch |

PRD §7 prefers a single source of truth if it can be done cleanly — e.g.
`build-macos.sh` extracting the value from `CMakeLists.txt` (a `grep`/`sed` on
the `set(CMAKE_OSX_DEPLOYMENT_TARGET "…"` line) instead of restating it. Attempt
that; a comment cross-reference at both sites is the acceptable fallback if the
extraction is fragile. **Do not** invent a new config file for one value.

Note the existing comment at `CMakeLists.txt:56-57` is *wrong* — it says the
target is set "to match the SDK being used / This prevents version mismatch
warnings", which is exactly what the 18 warnings disprove. Replace it; do not
leave a stale rationale next to a corrected number.

**Acceptance:** FR-6 — a clean build log with **zero** `ld: warning:` lines.

**Use `make macos-rebuild`, not `make macos -B`.** `-B` re-runs the recipes (so
`cmake -S . -B` *does* reconfigure and the `CACHE … FORCE` value *does* take
effect) but it deletes nothing, and `make macos-clean` removes only `./dist` and
the `.dmg`. Only `build-macos.sh --clean`, reached via `make macos-rebuild`,
`rm -rf`s the build directory. This is not pedantry: **cargo does not reliably
invalidate its cache when `MACOSX_DEPLOYMENT_TARGET` changes**, so
`libsimsapa_bridges.a` — the exact artifact whose symbols stage 2 inspected —
can survive a `-B` build with the old target baked in, and the warning check
would then pass for the wrong reason.

**Depends on:** task 2.0. **Blocks:** 5.0.

- [ ] 3.0 Apply the determined minimum at both sites and confirm the linker warnings are gone (FR-4b, FR-5, FR-6, FR-7)
  - [ ] 3.1 Set `CMAKE_OSX_DEPLOYMENT_TARGET` at `CMakeLists.txt:58` to the value from task 2.6.
  - [ ] 3.2 Replace the misleading comment above it. The new comment must state: the floor is driven by Qt 6.9.3's frameworks (12.0) **and** cpal's CoreAudio ProcessTap API (14.2, strongly bound); it must match `LSMinimumSystemVersion` in `build-macos.sh`; and it must not be lowered speculatively — re-derive it with the task 2.0 procedure instead.
  - [ ] 3.3 Update `build-macos.sh:183-184` to the same value. Attempt the single-source-of-truth approach first: have the script read the value out of `CMakeLists.txt` and fail loudly if the extraction returns empty (a silent empty string would write a broken plist).
  - [ ] 3.4 If the extraction proves fragile, fall back to a literal value plus a comment pointing at `CMakeLists.txt:58` — and add the same pointer in the other direction. Record which approach was taken and why.
  - [ ] 3.5 **On the Mac:** run `make macos-rebuild` (a genuinely clean build — see the spec above) and capture the full log.
  - [ ] 3.6 Grep the log for `ld: warning:` — there must be **zero** hits (FR-6). The baseline is 18: 16 of the `building for macOS-11.0, but linking with dylib …` form and 2 of the `object file (…QDarwinMicrophonePermissionPlugin…) was built for newer 'macOS' version` form (source log lines 7-8). Confirm **both** forms are gone; if the plugin pair persists, record it as a separate finding rather than silencing it (PRD §7).
  - [ ] 3.7 Check `README.md` for any stated macOS version requirement. The initial grep found only build-command references and no "macOS 11+" claim — confirm that, and **record in this file that it was checked** even if nothing changed (FR-7 requires the negative result to be stated).
  - [ ] 3.8 Check the release-notes / download-page text the project uses for releases for the same claim, and correct it if present. If there is no such text under version control, say so.

---

### Stage 4 — audit the other platforms

**Spec.** Read-only (PRD §5: "if it finds a real gap, fixing it is a follow-up
task"). The question for each platform: does a Rust dependency bind to a system
library that the final CMake link does not name?

- **Windows** (`CMakeLists.txt:447-452`): only `bcrypt`. cpal's WASAPI backend
  may want `ole32` / `mmdevapi`. **Expect to find no gap, for a structural
  reason:** on Windows/MSVC, rustc emits `/DEFAULTLIB:` **linker directives into
  the COFF object files** for `#[link]` / `cargo:rustc-link-lib`, and the linker
  honours them automatically. Mach-O and ELF `.a` archives have no equivalent
  mechanism — which is precisely why macOS and Linux need explicit CMake entries
  and Windows mostly does not. (That asymmetry is also why `bcrypt` is the lone
  manual Windows entry, and it is the nuance the FR-11 rule must carry — see the
  stage 6 spec.) A successful Windows build is sufficient evidence.
- **Android**: cpal's AAudio backend goes through the `ndk` crate against system
  libraries, and the app builds and runs today — a read-only confirmation
  against the existing "Android link: NDK system libraries" section in
  `docs/pure-rust-audio-backend.md`.
- **Linux**: already correct (the ALSA block); note it as the worked example.
- **iOS**: **excluded** (PRD §5). Do not touch it. The PRD already records the
  finding for whoever revives it (needs `AudioToolbox` + `AVFAudio`, **not**
  `CoreAudio`) — carry that into the doc in stage 6 rather than acting on it.

An audit that finds nothing must still say it looked.

**Depends on:** nothing (independent of 1.0-3.0). **Blocks:** 6.0.

- [ ] 4.0 Audit the other active platforms' link configurations for the same gap (FR-10)
  - [ ] 4.1 Confirm `backend/src/audio/` has no platform `cfg` gates — i.e. the audio code really is compiled into every target, which is what makes this a cross-platform question at all.
  - [ ] 4.2 Windows: read `CMakeLists.txt:447-452`, then determine what cpal 0.18.1's WASAPI backend links against (check the crate source / its `windows-sys` feature list in `~/.cargo/registry/src/*/cpal-0.18.1`). Record whether `ole32`/`mmdevapi` arrive automatically via rustc's `/DEFAULTLIB:` COFF directives or genuinely need naming in CMake.
  - [ ] 4.3 Windows: if a Windows build can be produced, that is the evidence — record the result. If not, record the static analysis from 4.2 and mark it as unverified-by-build.
  - [ ] 4.4 Android: confirm the current `ANDROID` link block against the "Android link: NDK system libraries" doc section, and note that the shipping app builds and runs (so AAudio resolves). No change expected.
  - [ ] 4.5 Write the audit outcome for each platform into this file — including the ones where nothing was wrong. This is a deliverable (success metric 6), not a formality.

---

### Stage 5 — verify on the Mac

**Spec.** FR-8's order is deliberate: link → app runs at all → audio actually
works → permissions → distribution artifact. A successful link does **not**
prove the audio path works, which is why 5.3 is the real test.

**FR-9 limitation — state it, do not paper over it.** The build machine runs
macOS 26.5.2. Every symbol in question exists there, so this machine **cannot**
detect the launch failure that the deployment-target work is about. The binary
inspection from task 2.0 is the substitute evidence. This mirrors the existing
note in `docs/android-edge-to-edge-and-safe-areas.md` that an Android 15 phone
cannot validate the edge-to-edge work.

**Depends on:** 1.0, 3.0. **Blocks:** the FR-9 wording in 6.0.

- [ ] 5.0 Verify the bundle on the macOS machine: launch, search, chanting record/playback, mic permission, DMG install (FR-8, FR-9)
  - [ ] 5.1 `make macos-rebuild` completes with exit status 0 and no linker warnings (re-confirming 3.5/3.6 from a genuinely clean build directory — **not** `make macos -B`, which cleans nothing).
  - [ ] 5.2 The produced `.app` launches; the sutta reader renders and search returns results — this confirms the framework additions did not disturb the Qt/WebEngine link.
  - [ ] 5.3 **Chanting practice: record a short clip and play it back.** This is the functional test of the pure-Rust audio backend and the real point of the whole change.
  - [ ] 5.4 The microphone permission prompt appears on first recording, confirming `QDarwinMicrophonePermissionPlugin` (`CMakeLists.txt:459-463`) and `NSMicrophoneUsageDescription` (`build-macos.sh:198-200`) still work after the deployment-target change. If the prompt does not appear, check whether permission was already granted from an earlier run before treating it as a regression.
  - [ ] 5.5 `make macos -B` also produces the `.dmg`; install the app from a copy mounted out of the `.dmg` (not the build tree) and launch it from `/Applications`.
  - [ ] 5.6 `plutil -p …/Contents/Info.plist | grep -i minimum` reports the task 2.6 value. Do this on the **installed** copy, since `macdeployqt` and the DMG step both rewrite the bundle.
  - [ ] 5.7 Record every result above in this file, including the FR-9 limitation: this machine cannot validate behaviour on older macOS, and task 2.0's symbol inspection is the substitute evidence.

---

### Stage 6 — write it down so it does not recur

**Spec.** Three documentation deliverables. Keep each tight — the PRD explicitly
warns against inventing a broader release-checklist document (FR-13).

1. **`CLAUDE.md`**, a new subsection under *Specific coding procedures*,
   following the shape of the existing "Adding a Qt module that needs an Android
   permission". The rule: *when adding a Rust dependency that binds to a native
   system library, add the matching `target_link_libraries` entry in
   `CMakeLists.txt` for every platform that library exists on — and check whether
   the crate selects a **different backend module** per platform, because the
   frameworks then differ too.* Worked examples: this incident and the ALSA
   block. The sting in the tail: **the failure appears only on the platform you
   are not building on.** The rule must also say **why it is per-platform rather
   than universal** — Mach-O/ELF `.a` archives carry no link directives, while
   rustc emits `/DEFAULTLIB:` into COFF objects on Windows/MSVC — or a reader
   will apply it to Windows and be puzzled that nothing is needed there. Plus
   FR-13, a couple of sentences in the same place: a
   macOS build must be produced on the on-site Mac before any release, because
   this break sat undetected from the Qt Multimedia removal until now.
2. **`docs/pure-rust-audio-backend.md`**: a per-platform "what the final link
   must supply" table — macOS: CoreAudio + AudioToolbox; Linux: libasound;
   Android: nothing (AAudio is a system lib); Windows: whatever task 4.0 found —
   plus the minimum-version finding and its reasoning, and the FR-9 limitation.
   Also record the iOS finding from PRD §5 as a note for whoever revives that
   target (`AudioToolbox` + `AVFAudio`, **not** `CoreAudio`; cpal selects a
   separate `src/host/coreaudio/ios/` module).
3. **CLAUDE.md's doc index** — the "Notable feature docs" bullet for
   `pure-rust-audio-backend.md` already exists; extend its summary to mention the
   per-platform link table and the macOS floor, since that index is what a future
   session reads first.

PRD §9 Q2 is open: whether the macOS/Qt version coupling belongs in
`docs/android-qt-upgrade-considerations.md` or a new Apple-platform equivalent.
Resolve it in 6.5 — a cross-reference from the existing doc is the cheap answer;
a new doc is only warranted if there is more Apple-platform upgrade content to
carry.

**Depends on:** 2.0 (the number), 4.0 (the audit results), 5.0 (the FR-9
wording).

- [ ] 6.0 Write down the prevention rule and the per-platform link table, and update the docs (FR-11, FR-12, FR-13)
  - [ ] 6.1 Add the new *Specific coding procedures* subsection to `CLAUDE.md` with the FR-11 rule, both worked examples (this incident, the ALSA block), and the "different backend module per platform" warning.
  - [ ] 6.2 Add the FR-13 pre-release note in the same place: a macOS build on the on-site Mac before any release, with the reason (no hosted CI; this break went undetected for months). Two sentences — do **not** start a release-checklist document.
  - [ ] 6.3 Add the per-platform link table to `docs/pure-rust-audio-backend.md`, filling the Windows row from task 4.0's actual finding rather than a guess.
  - [ ] 6.4 Add the minimum-version section to the same doc: the number, the two constraints (Qt 12.0, cpal ProcessTap 14.2), the strong-vs-weak binding evidence from task 2.2, and the FR-9 limitation that the 26.5.2 machine cannot test it.
  - [ ] 6.5 Add the iOS note from PRD §5 to that doc, clearly marked as *not implemented, for whoever revives iOS*.
  - [ ] 6.6 Resolve PRD §9 Q2: add the Qt-upgrade coupling either as a cross-reference in `docs/android-qt-upgrade-considerations.md` or as a new Apple-platform doc. Prefer the cross-reference unless there is more Apple upgrade content to carry; record the decision.
  - [ ] 6.7 Extend the `CLAUDE.md` "Notable feature docs" bullet for `pure-rust-audio-backend.md` to mention the per-platform link table and the macOS floor.
  - [ ] 6.8 Update `PROJECT_MAP.md` if any file's role changed. If nothing changed, say so — success metric 5 asks for the check, not necessarily an edit.
  - [ ] 6.9 Final pass: re-read PRD §8 Success Metrics and confirm each of the six is satisfied and its evidence is recorded in this file.
