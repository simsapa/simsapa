# macOS: distributing without a Developer ID

Simsapa's macOS DMG is not signed with an Apple Developer ID and is not
notarized: there is no Apple developer account. The bundle is signed **ad-hoc**
(`codesign --sign -`) by `build-macos.sh`. This page covers what that signature
must look like for Gatekeeper to let users in, how the build enforces it, and
what users do on first launch.

## 1. "Damaged" vs. "could not verify"

Gatekeeper checks an app only when it carries the `com.apple.quarantine`
attribute, which a browser sets on a download (and which a copy out of a
quarantined DMG inherits). For a quarantined app it shows one of two dialogs:

| Dialog | Cause | Way through |
|---|---|---|
| **"Simsapa" is damaged and can't be opened.** (Move to Trash / Cancel) | The bundle's signature **does not verify** | None in the UI. Only removing the quarantine attribute (§4) |
| **Apple could not verify "Simsapa" is free of malware…** (Move to Trash / Done) | The signature verifies, but the app is not notarized | System Settings → Privacy & Security → **Open Anyway** |

"Damaged" therefore means a broken or incomplete signature, not a corrupt
download. Every DMG we ship must get the second dialog.

### What a broken bundle looks like

The `codesign` output of a bundle that gets "damaged":

```
$ codesign -dvv Simsapa.app
CodeDirectory v=20400 … flags=0x20002(adhoc,linker-signed) …
Signature=adhoc
Info.plist=not bound
Sealed Resources=none

$ codesign --verify --deep --strict --verbose=2 Simsapa.app
Simsapa.app: code has no resources but signature indicates they must be present
```

`linker-signed` means the executable carries only the signature the linker
adds on Apple silicon; `Sealed Resources=none` means the bundle as a whole
(`Contents/_CodeSignature/CodeResources`) was never signed.

A good bundle:

```
$ codesign -dvv dist/Simsapa.app
CodeDirectory v=20400 … flags=0x2(adhoc) …
Signature=adhoc
Info.plist entries=18
Sealed Resources version=2 rules=13 files=1845

$ codesign --verify --deep --strict dist/Simsapa.app && echo ok
ok
```

`spctl -a -vv -t exec` answers `rejected` for **both**: it reports
notarization, not signature validity. Do not use it to tell the two apart.

## 2. Signing rules (`build-macos.sh`)

1. **Sign after the last change to the bundle.** `code_sign()` runs after
   `create_app_bundle` (which edits `Info.plist` and runs `macdeployqt`) and
   before `create_dmg`. Any later edit to a file inside the bundle breaks the
   seal.
2. **`macdeployqt` does not sign.** The Qt 6.9.3 kit's `macdeployqt` has
   `-codesign=<ident>` but no ad-hoc default (that default exists only in newer
   qtbase). Without `code_sign()` the bundled Qt frameworks keep The Qt
   Company's own Developer ID signatures (Team `A5GTH44LYL`) and the main
   executable keeps only the linker's — the "damaged" bundle above.
3. **Verify, and fail the build.** `code_sign()` runs
   `codesign --verify --deep --strict` on `dist/Simsapa.app` and exits 1 if it
   fails.
4. **Verify the copy inside the DMG too.** The DMG step copies the bundle, so
   `verify_dmg_app()` mounts the finished DMG read-only, verifies the app in
   it, detaches, and exits 1 on failure. It covers both the `create-dmg` path
   and the `hdiutil` fallback.

   Trap: the DMG carries a **license agreement**, and `hdiutil attach` stops at
   an "Agree?" prompt. Without `Y` on stdin the attach is *cancelled*
   (`hdiutil: attach canceled`), so the function pipes `echo Y` into it. Do the
   same when mounting a release DMG by hand.
5. **Ad-hoc signing uses no hardened runtime and no entitlements**:
   `codesign --deep --force --sign - Simsapa.app`. Measured on macOS 26.6
   (Apple silicon), a quarantined copy got "Apple could not verify…" plus Open
   Anyway, and then ran, both **without** the hardened runtime and **with**
   `--options runtime --entitlements entitlements.plist`. The runtime buys
   nothing here, and without it QtWebEngine's JIT needs no entitlements, so
   the simpler signature is kept.
6. **`APPLE_SIGNING_IDENTITY` selects the Developer ID branch** (hardened
   runtime + `entitlements.plist`, for notarization). Leave it unset for the
   ad-hoc build. That branch still uses `--deep`, which gives
   `entitlements.plist` to every nested item including `QtWebEngineProcess.app`;
   Apple DTS recommends signing nested code inside out instead ("`--deep`
   Considered Harmful"). Revisit it if a developer identity is ever used.

## 3. Testing Gatekeeper

**Never judge Gatekeeper from a local build.** Files built or copied locally
carry no quarantine attribute, so `open dist/Simsapa.app` opens without any
dialog whatever the signature.

- **Quick test** — set the attribute by hand (the value's format is
  undocumented; this is a simulation):

  ```sh
  mkdir -p ~/Applications && rm -rf ~/Applications/Simsapa.app
  ditto dist/Simsapa.app ~/Applications/Simsapa.app
  xattr -w com.apple.quarantine "0081;$(printf %x $(date +%s));Safari;" ~/Applications/Simsapa.app
  open ~/Applications/Simsapa.app
  ```

- **Faithful test** — download an uploaded DMG with a browser, in a second
  macOS account, and check `xattr -l` shows `com.apple.quarantine`.

Test hygiene: use a **second account** and its `~/Applications`
(`/Applications` is shared by every account, so a test copy there overwrites a
real install); delete the app between attempts; use a **fresh** quarantined
copy each time, because an Open Anyway approval is remembered.

## 4. User instructions: first launch

1. Open the DMG and drag **Simsapa** to **Applications**.
2. Open Simsapa. macOS says *Apple could not verify "Simsapa" is free of
   malware…*. Click **Done**.
3. Open **System Settings → Privacy & Security**, scroll to the Security
   section, and click **Open Anyway** next to the Simsapa message. Confirm
   with your password.
4. Simsapa opens; later launches show no dialog.

Control-click → Open no longer bypasses Gatekeeper (removed in macOS Sequoia);
do not give that advice.

**Fallback**, if the dialog says "damaged" (an older release) or Open Anyway
does not appear — remove the attribute from the **installed app**:

```sh
xattr -dr com.apple.quarantine /Applications/Simsapa.app
```

Run it on the app in Applications, not on the DMG: removing it from the DMG
after the app has already been copied out leaves the copy quarantined.

## 5. Sources

- [Apple Developer Forums: Gatekeeper and code signing (thread 740680)](https://developer.apple.com/forums/thread/740680)
- [Apple Developer News: Gatekeeper change in macOS Sequoia](https://developer.apple.com/news/?id=saqachfa)
- [Apple Developer Forums: `--deep` Considered Harmful (thread 129980)](https://developer.apple.com/forums/thread/129980)
- [Qt 6: Qt for macOS – Deployment (macdeployqt options)](https://doc.qt.io/qt-6/macos-deployment.html)
- [QTBUG-101695: macdeployqt breaks signatures on Apple silicon](https://bugreports-test.qt.io/browse/QTBUG-101695)
- [SuperCollider PR #5650: ad-hoc sign after macdeployqt](https://github.com/supercollider/supercollider/pull/5650)
- [YUI issue #1691: ad-hoc bundle signature on macOS 26](https://github.com/yw0nam/YUI/issues/1691)
- [AppleInsider: Control-click removed in Sequoia](https://appleinsider.com/articles/24/08/06/apple-removes-control-click-option-for-skipping-gatekeeper-in-macos-sequoia)
- PRD: `tasks/2026-11-06-145915-prd---macos-gatekeeper-damaged-app.md`
