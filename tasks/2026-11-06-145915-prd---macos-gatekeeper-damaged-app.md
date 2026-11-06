# PRD: macOS "Simsapa is damaged and can't be opened" (Gatekeeper, unsigned DMG)

## 1. Introduction / Overview

The macOS release `Simsapa-v1.2.0-alpha.2-arm64.dmg` is not signed with an
Apple Developer ID (there is no developer account) and is not notarized.

Observed:

- The DMG was built under the `gambhiro` account on a macOS 26 (Tahoe) Mac mini
  (Apple silicon). Under a second account (`sumedharama`) on the **same
  machine**, the DMG was downloaded from GitHub Releases. Opening the app gave
  **"Simsapa" is damaged and can't be opened.**
- After `xattr -d com.apple.quarantine ~/Downloads/Simsapa-v1.2.0-alpha.2-arm64.dmg`,
  the DMG opened, the app was copied to Applications, and it ran.
- **Other users keep getting "damaged" even after removing the quarantine
  attribute.**

Goal: a downloaded, quarantined Simsapa app should get macOS's normal
"Apple could not verify 'Simsapa' is free of malware…" prompt, which a user can
allow in **System Settings → Privacy & Security → Open Anyway**, instead of
"damaged", which offers no way through. Users must also have correct written
instructions for the first launch.

**The root cause is not yet established.** This PRD records what was changed,
what the research supports, which earlier assumption was wrong, and the
measurements that must be taken on a Mac to settle it.

## 2. Goals

1. Find out, by measurement on the Mac, whether the v1.2.0-alpha.2 bundle's
   signature is valid (`codesign --verify --deep --strict`).
2. Make every DMG the build produces carry a complete, verified signature
   (ad-hoc when there is no developer identity), and fail the build otherwise.
3. Confirm with a real quarantined download, in a second account, that a new
   DMG shows "could not verify" + Open Anyway, not "damaged", and that the app
   then runs, including the QtWebEngine reader views.
4. Decide, by an A/B test, whether the ad-hoc signature needs the hardened
   runtime to get the Open Anyway path.
5. Document the signing rules and the user's first-launch steps in a `docs/`
   page.

## 3. User Stories

- As a Mac user who downloaded Simsapa from GitHub, I want macOS to tell me how
  to allow the app, so that I can open it without Terminal commands.
- As a Mac user who already got "damaged", I want one correct command that
  fixes it, so that I am not stuck after following half-correct advice.
- As the maintainer, I want `make macos -B` to fail when the bundle's signature
  is broken, so that I never upload a DMG every user will see as "damaged".

## 4. What has already been done (on Linux, untested)

`build-macos.sh` (committed on `main`, written on Linux):

- The script had a `code_sign()` function that `main` **never called**. It is
  now called right after `create_app_bundle` and before `create_dmg`:
  `code_sign "./dist/$APP_NAME.app"`.
- `code_sign()` now:
  - with `APPLE_SIGNING_IDENTITY` set: unchanged Developer ID signing
    (`codesign --deep --force --options runtime --entitlements entitlements.plist`),
    but now exits on failure;
  - without it: **ad-hoc signs** the bundle, without the hardened runtime:
    `codesign --deep --force --verbose --sign - "$app_bundle"`;
  - then always runs `codesign --verify --deep --strict --verbose=2` and exits
    1 if verification fails.
- The end-of-build note no longer says "right-click → Open" (removed in macOS
  Sequoia). It now names Privacy & Security → Open Anyway and
  `xattr -dr com.apple.quarantine /Applications/Simsapa.app`.

Only `bash -n build-macos.sh` was run. Nothing was run on macOS.

Decision (maintainer): **keep this step always**, whatever the diagnosis shows.
It costs seconds and turns a broken seal into a build failure.

## 5. Research findings

### 5.1 Corrected assumption: `macdeployqt` already ad-hoc signs

The first explanation given was that the bundle shipped unsigned because
`macdeployqt` and the `Info.plist` edits ran after the linker signed the
binaries. **That was wrong on two counts:**

- Qt's docs: `-codesign=<ident>` — "Run codesign with the given identity on all
  executables. By default, ad-hoc signing (`-codesign=-`) is used."
  `-no-codesign` disables it. The qtbase source agrees
  (`codesignIdentity = QStringLiteral("-")`, help text "(default: ad-hoc sign)").
- In `build-macos.sh`, `update_info_plist` runs **before** `deploy_qt_frameworks`
  (`macdeployqt`), so the plist edits come before Qt's signing, not after.

So the released bundle **may already have had a valid ad-hoc signature.** Not
documented by Qt: whether `macdeployqt` seals the bundle as a whole
(`Contents/_CodeSignature/CodeResources`) or signs only the executables. The
Mac measurement in Task 1 settles it.

> **Measured on the Mac (Task 1): this correction is itself wrong for the Qt
> 6.9.3 kit.** Its `macdeployqt` has no ad-hoc default and signed nothing; the
> released bundle was linker-signed only and fails `codesign --verify`. See
> Open Question 1.

### 5.2 Gatekeeper behaviour

- An Apple engineer on the Apple Developer Forums (thread 740680):
  - all code on Apple silicon must be signed, and code that looks unsigned is
    typically ad-hoc signed by the linker;
  - "for something simple like an app, Gatekeeper typically only kicks in if the
    app is quarantined";
  - the rules are "complicated, not officially documented, and evolve over time".
- This explains the second account on the build Mac: quarantine was removed
  from the DMG **before** it was opened, so the copied app never carried the
  attribute and Gatekeeper never checked it.
- Apple Developer News: "In macOS Sequoia, users will no longer be able to
  Control-click to override Gatekeeper when opening software that isn't signed
  correctly or notarized. They'll need to visit System Settings > Privacy &
  Security…"
- "damaged" vs "could not verify" (third-party guides, read only through
  search summaries, **not Apple documentation**): "damaged" is shown when a
  quarantined app's signature does not validate, and Open Anyway is not
  offered. "Apple could not verify…" is shown for an intact but un-notarized
  app, and Open Anyway is offered.

### 5.3 The one measured macOS 26 case (YUI issue #1691)

A Tauri app on macOS 26 (Darwin 25.2.0), Apple silicon. **Neither dialog came
from a real download**: in both cases the author set the quarantine attribute
by hand (`xattr -rw com.apple.quarantine …`, then `open YUI.app`) on local
builds. The "after" app was copied out of the built `.dmg`. The issue lists as
not measured: a real Safari download of either build, the Developer ID path,
and any macOS version other than 26.

| | Before | After |
|---|---|---|
| `codesign -dvv` | `flags=0x20002(adhoc,linker-signed)`, `Info.plist=not bound` | `flags=0x10002(adhoc,runtime)`, `Sealed Resources version=2` |
| `codesign --verify` | `code has no resources but signature indicates they must be present` (exit 1) | `valid on disk`, `satisfies its Designated Requirement` |
| Dialog | "is damaged and can't be opened", Move to Trash / Cancel | "Apple could not verify…", Move to Trash / Done, then Open Anyway in Settings |

The fix was a full ad-hoc bundle signature **with the hardened runtime**. Our
current change signs ad-hoc **without** it. Whether that difference matters is
unknown, hence the A/B test in Goal 4. This is one report, not Apple
documentation.

### 5.4 `codesign --deep`

Apple DTS, "`--deep` Considered Harmful" (forum thread 129980): recommends
signing nested code separately, inside out. `--deep` applies the same options
and entitlements to every nested item, and misses code placed where data is
expected. For our ad-hoc step, which passes no entitlements, the first problem
does not apply. It does apply to the Developer ID branch: it would give
`entitlements.plist` to `QtWebEngineProcess.app` and every framework.

### 5.5 Related Qt notes

- `macdeployqt` and `install_name_tool` edits can break existing signatures on
  Apple silicon (QTBUG-101695); SuperCollider re-signs ad-hoc after
  `macdeployqt` for that reason (PR #5650).
- `macdeployqt` options `-hardened-runtime`, `-timestamp`,
  `-sign-for-notarization=<ident>`; when signing, it uses the first
  `.entitlements` file it finds in the bundle's `Resources` folder.

## 6. Functional Requirements

1. `build-macos.sh` must sign the finished bundle after `macdeployqt` and before
   the DMG is built: with `APPLE_SIGNING_IDENTITY` when set, otherwise ad-hoc.
2. The build must run `codesign --verify --deep --strict` on the bundle and stop
   with a non-zero exit if it fails.
3. The build must also verify the app **inside the finished DMG** (mount it,
   verify the copy, detach), because the DMG step copies the bundle.
4. The ad-hoc variant (with or without the hardened runtime) must be chosen by
   the A/B test in Task 3, and the choice recorded in the docs page with the
   measured result.
5. If the hardened runtime is chosen, the app must still run QtWebEngine
   (reader views render; `QtWebEngineProcess` starts), with entitlements applied
   where QtWebEngine needs them.
6. A new `docs/macos-unsigned-distribution.md` must state: the signing rules
   (sign after the last change to the bundle, verify, never ship an unverified
   bundle), what "damaged" vs "could not verify" mean, the user's first-launch
   steps, the `xattr -dr` fallback **on the app in Applications** (not on the
   DMG), and the sources. It must be linked from `CLAUDE.md`'s notable docs.

## 7. Non-Goals

- Getting an Apple Developer account, Developer ID signing, or notarization.
- Intel (x86_64) builds or a universal binary.
- Mac App Store distribution (see `docs/apple-app-store-readiness.md`).
- Changing the app's code; this is packaging only.
- Release-notes or website text (the maintainer chose a `docs/` page only).

## 8. Technical Considerations

- **The Mac:** `gambhiro@192.168.1.143`, repo at
  `/Users/gambhiro/prods/simsapa-ng-project/simsapa/` (the `media-mac` git
  remote). It was unreachable from Linux during this session ("No route to
  host"), so all Mac work happens in a new session on the Mac.
- **Build:** `make macos -B` (runs `build` then `./build-macos.sh`). Outputs
  `./dist/Simsapa.app` and `./Simsapa-<version>-arm64.dmg`. Deployment target
  14.2 from `CMakeLists.txt`.
- **Releases:** `https://github.com/simsapa/simsapa/releases`; the affected
  asset is
  `https://github.com/simsapa/simsapa/releases/download/v1.2.0-alpha.2/Simsapa-v1.2.0-alpha.2-arm64.dmg`.
- **Hardened runtime and QtWebEngine (unverified, check in Task 3.3):** with
  `--options runtime`, JIT code needs the `com.apple.security.cs.allow-jit`
  entitlement. `entitlements.plist` already carries it, together with
  `allow-unsigned-executable-memory` and `disable-library-validation`; which of
  these QtWebEngine actually needs was not researched. The JIT runs in the
  **helper** `QtWebEngineProcess.app` (inside
  `QtWebEngineCore.framework/Helpers/`), so the entitlements must reach it, not
  only the main executable. `--deep --entitlements` does that, bluntly.
- **Testing quarantine, two levels:**
  - *Quick:* set the attribute by hand on a local copy and open it, as the YUI
    report did: `xattr -w com.apple.quarantine "0081;$(printf %x $(date +%s));Safari;" Simsapa.app`
    then `open Simsapa.app`. The value's format is undocumented; this is a
    simulation.
  - *Faithful:* a browser download of an uploaded DMG in a second macOS account.
    Needed at least once for the chosen variant, because that is what users do.
  - Never conclude anything from opening `dist/` directly: locally built files
    carry no quarantine, so Gatekeeper is never involved.
- **Test hygiene:** `/Applications` is shared by every account on the Mac, so a
  test copy there overwrites the maintainer's install. Use `~/Applications` in
  the test account, and delete the app between variants. An Open Anyway
  approval is remembered, so each variant needs its own freshly downloaded or
  freshly quarantined copy.
- **Uploading test DMGs:** assets of a GitHub **draft** release are visible only
  to logged-in repository members; a **pre-release** is public. Either way it is
  an outward-facing step: ask the maintainer first.
- **`APPLE_SIGNING_IDENTITY`:** before this change `code_sign()` was never
  called, so the variable had no effect. It now selects the Developer ID branch.
  Make sure it is unset on the Mac unless that is intended.
- **zsh on the Mac:** `noclobber`/`nomatch` rules from `CLAUDE.md` apply if the
  Mac shell is configured the same way.

## 9. Success Metrics

- `codesign --verify --deep --strict` passes on `dist/Simsapa.app` and on the
  copy inside the DMG; the build fails when it does not.
- A browser-downloaded new DMG, opened in a second account with quarantine
  intact, shows "Apple could not verify…", and after Open Anyway the app starts
  and renders a sutta.
- No further "damaged" reports for releases built with the new script.

## 10. Open Questions

1. Did the v1.2.0-alpha.2 bundle verify? **Answered (Task 1, measured on the
   Mac, macOS 26.6.2): no — the signature was the cause.** The released DMG,
   mounted read-only, is byte-for-byte the YUI "before" case:

   ```
   Simsapa.app (and Contents/MacOS/simsapadhammareader):
     Identifier=simsapadhammareader
     CodeDirectory v=20400 … flags=0x20002(adhoc,linker-signed) hashes=53740+0
     Signature=adhoc
     Info.plist=not bound
     Sealed Resources=none
   codesign --verify --deep --strict:
     code has no resources but signature indicates they must be present   (exit 1)
   No Contents/_CodeSignature/ folder.

   QtWebEngineProcess.app, QtCore/QtWebEngineCore/QtQuick.framework, plugins:
     flags=0x10000(runtime)  TeamIdentifier=A5GTH44LYL   (The Qt Company's own
     Developer ID signature, as shipped in the Qt kit)
     QtWebEngineProcess.app: valid on disk, satisfies its Designated Requirement
   ```

   So `macdeployqt` signed **nothing**: the frameworks still carry Qt's
   original signatures and the main executable only the linker's. §5.1's
   "macdeployqt ad-hoc signs by default" does not hold for the Qt 6.9.3 kit
   used here: its `macdeployqt` help lists `-codesign=<ident>` with no
   "default: ad-hoc sign" and has no `-no-codesign` option. The default cited
   in §5.1 belongs to a newer qtbase than the one the project builds with.
   The "damaged" reports from other users are consistent with this (any
   quarantined copy of this bundle fails verification); their macOS versions
   and steps (Open Question 4) are no longer needed to explain them.
2. Is the hardened runtime required on ad-hoc builds for Open Anyway on macOS
   26? **Answered (Task 3.1/3.2, macOS 26.6.2, second account, hand-set
   quarantine on a copy in `~/Applications`): no.** Variant A (ad-hoc, no
   runtime, the current code) and Variant B (ad-hoc + `--options runtime
   --entitlements entitlements.plist`) behaved identically: "Apple could not
   verify 'Simsapa'" → Done → Privacy & Security listed Simsapa with
   **Open Anyway** → the app opened and ran. Variant A is kept. In B the
   entitlements did reach `QtWebEngineProcess.app` (`flags=adhoc,runtime`).
   Not yet done: the same check on a real browser download (Task 3.3).
3. Should the Developer ID branch stop using `--deep` and sign inside out? Only
   relevant once there is an identity.
4. Which macOS versions and browsers were the other users on? No longer
   needed to explain the reports (see 1). The users' remaining "damaged" after
   removing quarantine is consistent with removing it from the **DMG** after
   the app had already been copied out; the docs page gives the command on the
   installed app instead.

## 11. Follow-up tasks (summary)

The task list is `tasks/2026-11-06-145915-tasks-macos-gatekeeper-damaged-app.md`.

1. Diagnose the old v1.2.0-alpha.2 bundle on the Mac.
2. Build with the new script and verify both the bundle and the DMG copy.
3. A/B test the ad-hoc variants (no runtime vs runtime + entitlements) with a
   hand-set quarantine attribute, then confirm the better one with a real
   download.
4. Add the DMG-copy verification to the script.
5. Write `docs/macos-unsigned-distribution.md`, link it from `CLAUDE.md`, update
   `PROJECT_MAP.md` if it lists `build-macos.sh`.
6. Ask the maintainer before committing.

## 12. Sources

- [Apple Developer Forums: Gatekeeper and code signing (thread 740680)](https://developer.apple.com/forums/thread/740680)
- [Apple Developer News: Gatekeeper change in macOS Sequoia](https://developer.apple.com/news/?id=saqachfa)
- [Apple Developer Forums: `--deep` Considered Harmful (thread 129980)](https://developer.apple.com/forums/thread/129980)
- [Qt 6: Qt for macOS – Deployment (macdeployqt options)](https://doc.qt.io/qt-6/macos-deployment.html)
- [qtbase macdeployqt `main.cpp` source](https://contribute.qt-project.org/doc/da/da9/qtbase_2src_2tools_2macdeployqt_2macdeployqt_2main_8cpp_source.html)
- [QTBUG-101695: macdeployqt breaks signatures on Apple silicon](https://bugreports-test.qt.io/browse/QTBUG-101695)
- [SuperCollider PR #5650: ad-hoc sign after macdeployqt](https://github.com/supercollider/supercollider/pull/5650)
- [YUI issue #1691: ad-hoc bundle signature on macOS 26](https://github.com/yw0nam/YUI/issues/1691)
- [swissmacuser: "damaged" on macOS Tahoe](https://swissmacuser.ch/fix-macos-tahoe-app-is-damaged-and-cant-be-opened-move-trash/)
- [swissmacuser: "Apple could not verify…" and Allow Anyway](https://swissmacuser.ch/how-to-allow-blocked-macos-apps-couldnt-verify-malware-privacy/)
- [AppleInsider: Control-click removed in Sequoia](https://appleinsider.com/articles/24/08/06/apple-removes-control-click-option-for-skipping-gatekeeper-in-macos-sequoia)
- [Corey Daley: "App is damaged and can't be opened"](https://coreydaley.dev/apps/purrchiver/app-is-damaged-cant-be-opened-mac/)
