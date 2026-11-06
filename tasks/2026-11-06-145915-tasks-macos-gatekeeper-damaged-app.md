# Tasks: macOS "damaged and can't be opened" (Gatekeeper, unsigned DMG)

PRD: `tasks/2026-11-06-145915-prd---macos-gatekeeper-damaged-app.md` — read
§4 (what is already changed) and §5 (research, including the corrected
assumption that `macdeployqt` already ad-hoc signs by default) before starting.

**Run these tasks on the Mac** (macOS 26, Apple silicon,
`/Users/gambhiro/prods/simsapa-ng-project/simsapa/`). The `build-macos.sh`
change was written on Linux and has never run. It is committed on `main` on
the Linux machine; make sure the Mac's checkout has that commit before starting
(ask the maintainer how they sync the two).

## Relevant Files

- `build-macos.sh` - Packaging script. `code_sign()` (ad-hoc fallback + strict
  verify) is now called from `main` after `create_app_bundle`, before
  `create_dmg`. `update_info_plist` runs before `deploy_qt_frameworks`
  (`macdeployqt`, which ad-hoc signs by default).
- `entitlements.plist` - allow-jit, allow-unsigned-executable-memory,
  disable-library-validation, network, audio-input. Used by the Developer ID
  branch, and by the ad-hoc "runtime" variant if Task 3 chooses it.
- `Makefile` - `make macos -B` (build + DMG), `make macos-app` (no DMG),
  `make macos-clean`.
- `CMakeLists.txt` - `CMAKE_OSX_DEPLOYMENT_TARGET` 14.2; `QT_MACOS` Qt version.
- `docs/macos-unsigned-distribution.md` - new; signing rules + user steps.
- `CLAUDE.md` - add the new doc to "Notable feature docs".
- `PROJECT_MAP.md` - its `build-macos.sh` line (around line 537) needs the signing step.

### Notes

- **Never judge Gatekeeper from a local build as is.** Files built or copied
  locally carry no `com.apple.quarantine`, so Gatekeeper never checks them.
  Quick test: set the attribute by hand and `open` the app (PRD §8, "Testing
  quarantine"); the measured macOS 26 case in PRD §5.3 was done only this way.
  Faithful test: a browser download of an uploaded DMG in a **second** macOS
  account.
- **Test hygiene:** `/Applications` is shared between accounts; use
  `~/Applications` in the test account, delete the app between variants, and
  use a fresh quarantined copy per variant (an Open Anyway approval is
  remembered).
- Make sure `APPLE_SIGNING_IDENTITY` is unset, or `code_sign()` takes the
  Developer ID branch.
- Useful commands:
  - `codesign -dvv <app> 2>&1 | grep -E "flags|Info.plist|Sealed|Signature"`
  - `codesign --verify --deep --strict --verbose=2 <app>`
  - `spctl -a -vv -t exec <app>` (expect "rejected" for an un-notarized app;
    the *source* line and any error text are what matter)
  - `xattr -l <file>` (does it carry `com.apple.quarantine`?)
  - `hdiutil attach -nobrowse -readonly <dmg>` / `hdiutil detach <mountpoint>`
- Reading `codesign -dvv` (from the measured macOS 26 case, PRD §5.3):
  `flags=…(adhoc,linker-signed)` with `Info.plist=not bound` = linker signature
  only, the "damaged" case. `Sealed Resources version=2` = the bundle is sealed.
- Ask the maintainer before any `git commit`. Never bump versions.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

- [x] 1.0 Diagnose the released v1.2.0-alpha.2 bundle
  - [x] 1.1 Download `https://github.com/simsapa/simsapa/releases/download/v1.2.0-alpha.2/Simsapa-v1.2.0-alpha.2-arm64.dmg`
        (or use the old `dist/Simsapa.app` if it is still from that build), mount
        it read-only with `hdiutil attach -nobrowse -readonly`.
  - [x] 1.2 Run `codesign -dvv` and `codesign --verify --deep --strict --verbose=2`
        on the `.app` inside the mount; also on
        `Contents/Frameworks/QtWebEngineCore.framework/Helpers/QtWebEngineProcess.app`.
        Record the output in the PRD's Open Question 1.
  - [x] 1.3 Conclude: verify fails / `linker-signed` / `Info.plist=not bound` →
        the signature was the cause. Verify passes → it was not; record the
        alternative candidates (PRD §10.1) and ask the maintainer for the other
        users' macOS version, browser, and the exact steps they took.
- [x] 2.0 Build with the new script and verify
  - [x] 2.1 `make macos -B`; confirm the log shows the ad-hoc signing and
        "Code signing completed and verified", and that the build does not stop.
  - [x] 2.2 Run `codesign -dvv` and `--verify --deep --strict` on
        `dist/Simsapa.app`; expect `Sealed Resources version=2` and
        `valid on disk`.
  - [x] 2.3 Launch `dist/Simsapa.app` locally and open a sutta, to confirm the
        re-sign did not break QtWebEngine (no quarantine involved here; this only
        checks that the app runs).
- [ ] 3.0 A/B test the ad-hoc variants
  - [x] 3.1 Variant A = current code (ad-hoc, no hardened runtime). Copy the app
        out of the built DMG into `~/Applications` of a second account, set the
        quarantine attribute by hand (PRD §8) and `open` it. Record the exact
        dialog text and buttons, whether Privacy & Security offers Open Anyway,
        and whether the app then renders a sutta.
  - [x] 3.2 Variant B = ad-hoc with `--options runtime --entitlements entitlements.plist`
        (the YUI case). Repeat 3.1 with a fresh copy. Also check QtWebEngine
        works under the hardened runtime (`codesign -d --entitlements - ` on
        `QtWebEngineProcess.app` shows the entitlements reached the helper).
  - [ ] 3.3 Confirm the better variant with a real download: ask the maintainer
        where to upload the DMG (a GitHub pre-release is public; a draft release
        is visible only to logged-in repo members), download it with Safari in
        the second account, confirm `xattr -l` shows `com.apple.quarantine`, and
        repeat the 3.1 observations.
  - [ ] 3.4 If time allows, repeat 3.3 with a Chrome download.
  - [x] 3.5 Keep the variant that gives Open Anyway **and** runs QtWebEngine;
        update `code_sign()` and its comment accordingly. If both give "damaged",
        stop and report back with the `codesign -dvv` output of each.
- [x] 4.0 Verify the app inside the finished DMG
  - [x] 4.1 In `build-macos.sh`, after the DMG is created (both the create-dmg
        and the hdiutil fallback paths), mount it read-only, run
        `codesign --verify --deep --strict` on the `.app` inside, detach, and
        exit 1 on failure.
  - [x] 4.2 `make macos -B` again and confirm the check runs and passes.
- [x] 5.0 Documentation
  - [x] 5.1 Write `docs/macos-unsigned-distribution.md`: signing order rules
        (sign after the last change to the bundle; verify bundle and DMG copy),
        "damaged" vs "could not verify", the user's first-launch steps (Open
        Anyway in Privacy & Security), the fallback
        `xattr -dr com.apple.quarantine /Applications/Simsapa.app` (on the app,
        not the DMG), the Task 3 result, and the sources from PRD §12. Describe
        current behaviour; keep the traps; no history narration.
  - [x] 5.2 Add a "Notable feature docs" entry for it in `CLAUDE.md`, and
        extend the `build-macos.sh` line in `PROJECT_MAP.md` (around line 537)
        to mention the signing step and the new doc.
- [ ] 6.0 Wrap up
  - [x] 6.1 Update the PRD's Open Questions with the answers found.
  - [ ] 6.2 Show `git status` / `git diff --stat` and ask the maintainer before
        committing.
