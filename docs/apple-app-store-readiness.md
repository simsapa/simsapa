# Apple App Store readiness (iOS and Mac App Store)

An assessment of whether Simsapa would pass App Review if submitted to Apple's
App Store, on **iOS/iPadOS** and on the **Mac App Store**, with the rules
behind each finding and what would have to change. Researched 2026-11-05
against the App Review Guidelines and Apple's "Upcoming requirements" page.

**Short answer: no, not as the app stands, on either store.** Neither store
has a build pipeline (there is no iOS project, and the macOS build is a
Developer-ID DMG). On top of that, each store has one hard blocker that a
packaging change cannot fix:

| Store | Hard blocker | Why it cannot be argued past |
|---|---|---|
| Mac App Store | **Qt WebEngine (Chromium)** renders every reader panel on desktop | Qt's own docs say WebEngine apps are *not compatible* with the Mac App Store: Chromium calls private APIs (Guideline 2.5.1), and Chromium's sandbox does not initialise under the App Sandbox the store requires (2.4.5(i)) |
| iOS | **Toolchain**: Qt 6.9.3 is not a release with documented Xcode 26 / iOS 26 SDK support, and Apple requires both for every upload since 2026-04-28 | An upload built with an older SDK is refused or warned at upload, before review |

Everything else listed below is fixable work: entitlements, Info.plist keys,
a privacy manifest, the update notice, consent wording and the store metadata.
None of it touches the content or the purpose of the app, which are
unproblematic: a free, offline, ad-free reader of religious texts with
dictionaries is a clear fit for Guideline 4.2 (minimum functionality).

---

## 1. What exists today

| Area | State in the repo |
|---|---|
| iOS build | `CMakeLists.txt` has an `IOS` branch (`QT_IOS`, Rust target `aarch64-apple-ios`, `Qt::WebView`), and the Rust/QML code has `target_os = "ios"` / `Qt.platform.os === "ios"` cases. There is **no** Xcode project, Info.plist, launch screen, asset catalog, signing setup or build script. It has never been built for a device as far as the repo shows. |
| macOS build | `build-macos.sh` makes a `.app` + DMG with `macdeployqt` and `create-dmg`, optionally signs with `codesign --options runtime --entitlements entitlements.plist` (Developer ID, for notarised direct download). |
| `entitlements.plist` | Hardened-runtime exceptions for WebEngine (`cs.allow-jit`, `cs.allow-unsigned-executable-memory`, `cs.disable-library-validation`), network client + server, user-selected files, print, audio input. **No `com.apple.security.app-sandbox`.** |
| Web rendering | Desktop: `SuttaHtmlView_Desktop.qml` / `DictionaryHtmlView_Desktop.qml` use **QtWebEngine**. Mobile: `*_Mobile.qml` use **QtWebView** (WKWebView on Apple platforms). |
| Licence | Apache-2.0 (`LICENSE.txt`). |
| Bundle id | `io.github.simsapa.app` (see [app-packaging-and-identifiers.md](./app-packaging-and-identifiers.md)). |

---

## 2. Requirements that apply to both stores

### 2.1 Apple Developer Program and store metadata

- A paid Apple Developer Program membership (individual, or organisation with
  a D-U-N-S number). The app should be submitted by whoever holds the rights to
  the code (5.2.1).
- **Privacy policy URL** in App Store Connect *and* reachable inside the app
  (5.1.1(i)). One exists (`https://simsapa.github.io/privacy-policy/`, linked
  from Settings next to "Don't send stats"); it must also cover AI providers
  (§2.4) and say how a user can request deletion of the stats.
- **App Privacy "nutrition label"** answers. Data the app collects: the update
  check sends app version, OS, architecture, CPU frequency and cores, memory
  and screen resolution unless "Don't send stats" is ticked
  (`backend/src/update_checker.rs`, `ReleasesRequestParams`). Declare it as
  diagnostics/usage data, not linked to identity, not used for tracking.
- **Age rating questionnaire** — the new system's answers were due
  2026-01-31 for existing apps and are mandatory for a new submission. The
  app has no user-to-user content; the AI features (§2.4) are the part to
  answer carefully.
- **EU trader status** (Digital Services Act) is required to be listed on the
  EU storefronts; a non-trader declaration is possible for a non-commercial
  developer.
- Export compliance: the app uses only standard HTTPS/TLS, which is exempt —
  set `ITSAppUsesNonExemptEncryption = NO` in Info.plist to skip the question
  on every upload.

### 2.2 Privacy manifest (`PrivacyInfo.xcprivacy`)

Required since 2024-05-01 for any app whose code — including third-party code
— calls a "required reason" API. Simsapa does, through Rust `std::fs::metadata`
(file timestamps), `fs4::statvfs` in `import_staging.rs` / `picker_url.rs`
(disk space), and whatever Qt and SQLite call (Qt has been seen to trigger
`SystemBootTime`). Missing reasons produce `ITMS-91053` at upload. Add a
manifest declaring `FileTimestamp`, `DiskSpace`, `SystemBootTime` and
`UserDefaults` (Qt's `QSettings` on Apple platforms) with approved reason
codes, and **no** tracking domains. Location: bundle root on iOS,
`Contents/Resources/` on macOS. Verify the final list against the upload
report rather than guessing — the warning email names each missing category.

The stats payload also deserves one look against Apple's fingerprinting rule:
combining CPU, memory and screen values is a classic fingerprinting signal.
It is opt-out, not linked to an identifier, and used for aggregate stats, so
it is defensible, but it should be declared, and the simplest posture for a
store build is to default "Don't send stats" to on, or send only version + OS.

### 2.3 The in-app update notice

`UpdateNotificationDialog.qml` points users at the GitHub release page, and is
already switched to a `market://` button on Android when
`SuttaBridge.is_installed_from_play_store()` is true (see
[android-beta-distribution-and-play-policy.md](./android-beta-distribution-and-play-policy.md)).
A store build needs the same switch:

- Mac App Store 2.4.5(vii): *"They must use the Mac App Store to distribute
  updates; other update mechanisms are not allowed."*
- iOS: directing users to download the app elsewhere is a rejection.

On Apple platforms the build type decides it (a store build is a separate
signing configuration), so a compile-time flag is enough; an App Store link
(`itms-apps://` / `macappstore://`) or no update notice at all.

The **databases** downloaded from GitHub Releases at first run, the language
packs, dictionaries and the Topic Index update are *content*, not code, and
are allowed (2.5.2 forbids downloading "code which introduces or changes
features or functionality"). Keep it that way: no remotely fetched JS or
templates that change behaviour.

### 2.4 AI features — consent before sending text to a provider

Guideline 5.1.2(i) as updated in November 2025: *"You must clearly disclose
where personal data will be shared with third parties, including with
third-party AI, and obtain explicit permission before doing so."*

The Gloss "AI translation", AI word selection and the Prompts tab send the
user's text to the provider they configured (Gemini, OpenRouter, Anthropic,
OpenAI, …, `assets/providers.json`). The user supplies their own key, which
helps, but there is **no disclosure or consent prompt** in the app today. Add a
one-time dialog when a provider is first enabled (or first used) naming the
provider and what is sent, and mention it in the privacy policy. Expect the
age-rating and review notes to ask about AI-generated content too; say that the
AI features are off until the user enters their own key.

### 2.5 Third-party content rights (5.2.1)

Reviewers may ask for proof that bundled or downloaded texts may be
distributed. The app already credits its sources (`DhammaTextSourcesDialog.qml`).
Before submitting, collect the licence of each corpus and dictionary
(SuttaCentral CC0, DPD, the translations imported from suttas-core and
dhammatalks.org, the curated StarDict list) into a page that can be linked in
the review notes. A non-commercial licence (e.g. CC BY-NC-SA) is compatible
with a free app with no in-app purchases; keep it free.

### 2.6 Microphone purpose string

Chanting practice records audio, so `NSMicrophoneUsageDescription` is required
on both platforms (macOS already patches one in). The current text, *"Simsapa
needs microphone access for voice recording features."*, is the kind of
generic string reviewers reject. Name the feature: *"Simsapa records your
chanting so you can play it back and compare it with the reference
recording."*

### 2.7 Qt licensing (not a review issue, a distribution one)

The app is Apache-2.0 and the Qt modules used are LGPLv3. On iOS Qt is linked
**statically**, and LGPL distribution of a statically linked binary through
the App Store is a long-debated grey area: the usual reading is that it is
compliant if users can relink against a modified Qt, which an open-source app
satisfies by publishing its source and build instructions. App Review does not
check this; it is a legal question for the maintainer, and a commercial Qt
licence is the only way to make it moot.

---

## 3. Mac App Store

### 3.1 Blocker: Qt WebEngine

Qt WebEngine Platform Notes, "Mac App Store Compatibility": *"Applications
using Qt WebEngine are not compatible with the Mac App Store, because: The
Chromium part of the code uses several private API methods, which are
prohibited by the App Store. Applications submitted to the App Store must be
code-signed with the App Sandbox feature enabled. The App Sandbox feature
interferes with Chromium's own sandbox initialization …"*. No configure option
is offered.

**The realistic way through is the mobile code path.** The reader already
renders through `QtWebView` on Android and (by the same QML) on iOS; on macOS
`QtWebView` uses WKWebView, which is allowed. A Mac App Store build would be a
third variant: desktop UI, but `*_Mobile.qml`-style web views. Expect gaps the
desktop views currently get from WebEngine (`runJavaScript` round trips, the
`WebEngineRepaintNudge`, find-in-page, web channel, context menus, PDF
viewer); these must be inventoried before deciding. WKWebView loading
`http://127.0.0.1:<port>` needs the ATS key `NSAllowsLocalNetworking` (or
`NSAllowsArbitraryLoadsInWebContent`) in Info.plist.

`macdeployqt -appstore-compliant` skips plugins that use private API and should
be used for the store build.

### 3.2 App Sandbox and entitlements (2.4.5(i))

Required entitlements for a store build:

| Entitlement | Why |
|---|---|
| `com.apple.security.app-sandbox` | mandatory |
| `com.apple.security.network.client` | database / dictionary downloads, update check, AI providers |
| `com.apple.security.network.server` | the Rocket localhost API (bound to `127.0.0.1`, `api/src/lib.rs`) |
| `com.apple.security.files.user-selected.read-write` | importing StarDict, EPUB/PDF, saving exports |
| `com.apple.security.device.audio-input` | chanting recordings |
| `com.apple.security.print` | printing |

Remove the three `com.apple.security.cs.*` WebEngine exceptions: they exist only
for Chromium and invite questions in review. The store build also needs an
embedded provisioning profile and to be signed with the "Apple Distribution"
and "3rd Party Mac Developer Installer" identities, packaged as a `.pkg`
(`productbuild`) and uploaded with Xcode/Transporter — 2.4.5(ii) forbids the
DMG/third-party installer route. Since 2025-02-18 no file in the bundle may
carry `com.apple.quarantine`.

Consequences of the sandbox for the app's design:

- **Data location.** `app_dirs2` resolves `~/Library/Application Support/simsapa`;
  under the sandbox `HOME` is the container, so the store build reads and
  writes `~/Library/Containers/io.github.simsapa.app/Data/Library/Application
  Support/simsapa`. Existing DMG users will not see their databases or user
  data; the first-run download starts afresh. A one-time import of the old
  folder (chosen by the user through an open panel) is the sandbox-legal way to
  carry bookmarks and history over.
- **The `simsapa` CLI** (npm) and the sandboxed app no longer share a
  `SIMSAPA_DIR`, so the CLI's in-use detection (`api-port.txt` + `/health`) and
  read-only mode will not see the store app. The CLI is a separate product and
  is unaffected otherwise.
- **Choosing a storage folder** outside the container, if offered on desktop,
  needs a security-scoped bookmark to survive a restart.
- A store build and a DMG build must not both be installed with the same
  bundle id; give one a different id or document that the DMG is replaced.

### 3.3 Global hotkey (`cpp/global_hotkey_mac.mm`)

Registering the chord with Carbon `RegisterEventHotKey` works in the sandbox.
**Synthesising ⌘C with `CGEventPost`** to copy the selection in another app,
gated on the Accessibility permission (`AXIsProcessTrusted`), is the risky
part: Apple's sandbox documentation lists Accessibility-gated APIs as
incompatible with the sandbox, and in 2026 a sandboxed clipboard manager that
posts ⌘V with `CGEvent.post` was rejected twice under 2.4.5 as "Accessibility
features should not be used for non-accessibility purposes". `CGRequestPostEventAccess`
is the documented sandbox-compatible request for posting events, but App
Review's acceptance of this use is not established. For a store build, ship
the hotkey **without** the synthetic copy (look up whatever is already on the
clipboard) or leave the feature out.

### 3.4 macOS SDK

Apple's minimum-SDK rule (2026-04-28) names iOS/iPadOS/tvOS/visionOS/watchOS
SDKs, not macOS. Building against the macOS 26 SDK nonetheless turns on the
Liquid Glass appearance by default; Qt fixed its rendering in 6.5.11, 6.8.5 and
6.10.0 — **not in 6.9.x**. With Qt 6.9.3, set `UIDesignRequiresCompatibility`
in Info.plist or build on a Qt with the fix.

---

## 4. iOS / iPadOS

### 4.1 Blocker: toolchain

- Since **2026-04-28** every upload must be built with **Xcode 26** and the
  **iOS 26 SDK**. From **2026-09-09** the deployment target must be iOS 13 or
  later (no constraint in practice).
- Qt's iOS docs for 6.10 onward cover Xcode 26 (including the step of
  replacing the default iOS platform component with the universal one). No
  official statement covers **6.9.3**, which is out of support. `QT_IOS` will
  need to move to a release with documented Xcode 26 support. Because the
  Qt version is per platform in `CMakeLists.txt`, iOS can move alone
  (see [qt-kit-selection.md](./qt-kit-selection.md)); Android's reasons for
  staying on 6.9.3 do not apply to iOS.

### 4.2 Build and bundle work (none exists yet)

- An Xcode project from `cmake -G Xcode` for the iOS kit, with signing team,
  provisioning, `CFBundleShortVersionString` / `CFBundleVersion` from
  `scripts/set-version.py` (add the iOS places to its `PLACES`).
- An **asset catalog** with the 1024 px App Store icon, and a **launch screen**
  storyboard (Qt generates defaults; replace with branded ones).
- Info.plist keys: `NSMicrophoneUsageDescription` (§2.6),
  `NSAppTransportSecurity` → `NSAllowsLocalNetworking` for the localhost reader
  pages, `ITSAppUsesNonExemptEncryption`, `UIFileSharingEnabled` /
  `LSSupportsOpeningDocumentsInPlace` only if the Files app should see exports.
- The privacy manifest (§2.2).
- Supported orientations for iPad (all four are expected unless the app
  requires full screen), and screenshots for 6.9" iPhone and 13" iPad.
- Rust: `aarch64-apple-ios` for device; `aarch64-apple-ios-sim` for the
  simulator. The staticlib's frameworks (CoreAudio/AudioToolbox/AVFoundation
  for cpal, Security/SystemConfiguration for reqwest) must be named in the
  final link, as on macOS (see
  [pure-rust-audio-backend.md](./pure-rust-audio-backend.md)).

### 4.3 Data storage and iCloud backup

The first-run download is several gigabytes (the full `app-assets` folder
measures 6.1 GB on the bootstrap machine). Apple's data storage guidance, and
a long history of rejections under it, require re-downloadable content to be
kept out of the device backup: put the databases and indexes in
`Library/Application Support` (or `Library/Caches`) and set
`NSURLIsExcludedFromBackupKey` on them. User data that cannot be re-downloaded
— bookmarks, gloss/prompts history, recordings, imported books, all of which
live in `appdata.sqlite3` — **should** be backed up, which argues for
excluding `dpd.sqlite3`, `dictionaries.sqlite3` and the Tantivy indexes but
not `appdata.sqlite3`.

Also verify where `app_dirs2` resolves on iOS before anything else:
`get_simsapa_internal_app_root_path()` strips a `.local/share/simsapa` suffix
on mobile, which is an Android/XDG layout; the iOS app container root is not
writable, only `Documents/`, `Library/` and `tmp/` are.

### 4.4 Behaviour that differs on iOS

- **The localhost server** (Rocket) runs only while the app is in the
  foreground — iOS suspends it in the background. That is fine for the reader;
  the agent-facing API (`/health`, the CLI's companion use) is effectively
  desktop-only on iOS. Binding to `127.0.0.1` (not `0.0.0.0`) is what keeps the
  Local Network permission prompt away; keep it.
- **Imports**: a document picked through the Files picker is a
  security-scoped URL. The Android SAF code is not used on iOS; the
  `file://` route in the import pipeline must call
  `startAccessingSecurityScopedResource` (or Qt must copy the file into the
  sandbox) — to verify on device.
- **Storage relocation** ([relocated-storage-recovery.md](./relocated-storage-recovery.md))
  is Android-specific (SD cards); on iOS `storage_path_state()` should never
  leave `absent`.
- Android-only code (`android_saf.rs`, raw picker, keep-screen-on JNI) needs iOS
  counterparts where the feature matters: keep-screen-on is
  `UIApplication.idleTimerDisabled`.

### 4.5 Rules that are already satisfied by design

- **2.5.6** (web content must use WebKit): `QtWebView` on iOS *is* WKWebView.
- **2.5.2** (no downloaded code): downloads are SQLite databases, Tantivy
  indexes and JSON.
- **3.1 / 3.2.2(iv)** (payments, donations): there are no in-app purchases,
  ads or donation prompts. Keep any future donation link outside the app
  (Safari), per 3.2.2(iv), unless registered as an approved nonprofit.
- **5.1.1(v)** (account deletion): the app has no accounts.

---

## 5. Checklist, in order

1. Decide which store is worth it. iOS needs a Qt upgrade for iOS only plus a
   whole new build pipeline; the Mac App Store needs a WebView-based desktop
   reader. Both reuse the existing mobile QML views.
2. iOS: move `QT_IOS` to a Qt with Xcode 26 support; build for a device;
   fix data location and backup exclusion (§4.3).
3. macOS: inventory what the desktop reader uses from WebEngine; prototype the
   QtWebView variant under the App Sandbox.
4. Both: privacy manifest, ATS key, microphone string, AI consent dialog,
   store-build gating of the update notice, stats default, export-compliance
   key.
5. Store metadata: privacy policy update (AI + stats deletion), nutrition
   label, age rating, content-rights page for the review notes, screenshots,
   support URL, EU trader status.
6. TestFlight before submission; read the upload email for `ITMS-` warnings.

---

## Sources

- [App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/) — 2.4.5, 2.5.1, 2.5.2, 2.5.6, 3.2.2(iv), 4.2, 5.1.1(i), 5.1.2(i), 5.2.1
- [Upcoming requirements](https://developer.apple.com/news/upcoming-requirements/) — Xcode 26 / iOS 26 SDK (2026-04-28), iOS 13 floor (2026-09-09), age ratings (2026-01-31), required-reason APIs (2024-05-01), quarantine attribute (2025-02-18)
- [Adding a privacy manifest](https://developer.apple.com/documentation/bundleresources/adding-a-privacy-manifest-to-your-app-or-third-party-sdk) and [Qt on iOS: ITMS-91053](https://blog.felgo.com/qt-on-ios-itms-91053-nsprivacyaccessedapitypes)
- [Qt WebEngine Platform Notes — Mac App Store Compatibility](https://doc.qt.io/qt-6/qtwebengine-platform-notes.html)
- [Qt for iOS (6.10)](https://doc.qt.io/qt-6.10/ios.html), [Qt forum on Xcode 26](https://forum.qt.io/post/832060)
- [Qt wiki: licensing on mobile platforms](https://wiki.qt.io/Licensing-talk-about-mobile-platforms)
- Apple Developer Forums: [clipboard manager rejected under 2.4.5 for CGEvent.post](https://developer.apple.com/forums/thread/820594), [Accessibility permission in the sandbox](https://developer.apple.com/forums/thread/789896)
- iCloud backup rejections: [example 2.23 rejection](https://community.gamesalad.com/discussion/comment/514276/), [isExcludedFromBackup and migration](https://developer.apple.com/forums/thread/749467)
