# Android beta package, on-device debugging, and the Play update policy

How to get a locally built Android package onto a phone that already has the
released app, how to watch its log messages from the command line, and why the
in-app update notice behaves differently depending on where the copy came from.

Companion to
[android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md) (how the
release AAB is built) and
[app-packaging-and-identifiers.md](./app-packaging-and-identifiers.md) (the
application id vs. the QML module URI).

---

## 1. The problem: a Play install cannot be replaced by a local build

Everything here follows from one fact that is easy to discover the hard way.

A copy installed from Google Play is signed by **Play App Signing** — Google
re-signs every upload with its own key. Measured on a real device carrying the
closed-testing build:

| | certificate | SHA-256 |
|---|---|---|
| installed from Play | `CN=Android, OU=Android, O=Google Inc.` | `fdf35925…` |
| our upload keystore (`android/signing.env`) | `CN=Simsapa, O=Profound Labs, C=PT` | `fef4991a…` |

Android has no key-swap path, so **no locally built package can replace a Play
install** — not a debug APK, not `make android-apk`, not one re-signed with the
upload key. `adb install -r` fails with `INSTALL_FAILED_UPDATE_INCOMPATIBLE`.

Two things that are *not* the cause, checked so nobody re-checks them:

- **Play does not rename the package** for a closed-testing track. The installed
  id is plain `io.github.simsapa.app`. (What Play does do is deliver it as a
  split install — `base.apk` + `split_config.arm64_v8a` + `split_config.xxhdpi`
  — with `installerPackageName=com.android.vending`.)
- The signature mismatch is not about debuggable-vs-release. Signing and the
  `debuggable` flag are independent; the flag lives in the manifest and survives
  re-signing untouched.

Uninstalling the Play copy would work, but it wipes app-private storage —
including the multi-hundred-MB downloaded `appdata.sqlite3` — and destroys any
upgrade-path test state. Hence the beta package.

## 2. The beta package

The beta carries its own application id so it installs **alongside** the
released app:

- `io.github.simsapa.app.beta`, launcher label **Simsapa (beta)**
- its own launcher icon — the S mark with a **B** badge
- `versionName` suffixed `-beta` (dist) or `-beta-debug` (local)
- signed with the release keystore, like everything else we ship

Defined in `android/build.gradle` (`applicationIdSuffix`, `versionNameSuffix`,
`res.srcDirs`) plus `android/AndroidManifest.beta.xml` for the label and
`android/res-beta/` for the icon. Nothing is hardcoded in the `Makefile`.

### The beta icon set

Two installs with the same icon are as confusing as two with the same name, so
`android/res-beta/` overrides the launcher mipmaps for the beta variant only. It
is added **in addition to** the main res dirs (`res.srcDirs += ['res-beta']`):
a variant source set takes priority over `main` for same-named resources, so
`res-beta/` replaces `ic_launcher*` while `values/` and `xml/` still come from
`res/`.

Regenerate with `scripts/generate_beta_app_icons.sh` (idempotent, ImageMagick 7)
from `assets/icons/appicons/simsapa-beta_w512.png`. Do not hand-place the art:
the script derives its geometry from the shipped release icons so the two marks
sit identically on the launcher. Both source PNGs trim to the same 440×440 box at
+36+36 in a 512 canvas — the beta art was drawn on the release grid — so the
release transform applies unchanged:

| layer | canvas (xxxhdpi) | art | note |
|---|---|---|---|
| `ic_launcher_foreground` | 432 | 272 px (0.6296 of edge) | content lands at 234/432, inside the 66.7% safe zone, so no launcher mask clips the badge |
| `ic_launcher_monochrome` | 432 | — | byte-identical copy of the foreground, matching the release set's own convention |
| `ic_launcher_background` | 432 | — | solid `#FAE6B2`, copied verbatim from `res/` |
| `ic_launcher` (legacy) | 192 | 182 px (0.9455 of edge) | composed the way Android derives a legacy icon: central 72/108 cropped and masked round |

Verified in the finished packages by extracting `res/*.png` and pixel-comparing:
the beta APK's 432 px layers are a **0-pixel** match for `res-beta/`'s
foreground and differ from the release art by exactly the badge region; the
plain release APK is the inverse. The legacy `ic_launcher.png` is effectively
unused at `minSdkVersion 27` — the `mipmap-anydpi-v26` adaptive icon wins on
every supported device — but is generated to keep the set complete.

If the release monochrome layer is ever redrawn as a proper silhouette (today it
is a copy of the foreground, so themed icons render as a filled blob), redo both
sets together.

Two consequences worth knowing before reaching for it:

- The beta gets its **own data directory** (`/data/user/0/io.github.simsapa.app.beta/`),
  so it runs first-time asset setup of its own. Coexistence is not free.
- The FileProvider authority is declared `${applicationId}.qtprovider` in
  `android/AndroidManifest.xml`, so it follows the suffix automatically and the
  two installs do not collide on it. Same for `androidx-startup`. A future
  provider added with a *hardcoded* authority would break this.

### Two variants of the same package

| | build type | debuggable | distribute? |
|---|---|---|---|
| `make android-beta-debug` | Gradle `debug` | **yes** | **never** |
| `make android-beta-dist` | Gradle `release` + `simsapaBeta` | no | yes — GitHub Releases |

They share the id, so one replaces the other on a device and a developer never
ends up with two betas; `versionName` says which is installed.

**Never distribute the debuggable one.** `android:debuggable="true"` lets
anything with adb access `run-as` the package, read its private data and attach
a debugger. Play refuses debuggable uploads outright, which is a fair signal of
how it is regarded even where no policy applies.

### Why the dist beta is the *release* build type

Gradle build types cannot simply be added here: androiddeployqt invokes
`assembleDebug` or `assembleRelease` according to `CMAKE_BUILD_TYPE`, so a third
`beta` build type would never be built. The beta identity is instead applied to
the **release** type on demand, through the same `ORG_GRADLE_PROJECT_*`
environment mechanism already used by `simsapaReleaseOnly`:
`build-android.sh --beta` exports `ORG_GRADLE_PROJECT_simsapaBeta=true`, and
`build.gradle` keys `applicationIdSuffix` / the manifest overlay off
`project.hasProperty("simsapaBeta")`.

The same unset-not-`false` rule applies as for `simsapaReleaseOnly`:
`hasProperty()` is true for **any** value, including `"false"` and `""`.

### The package-identity guard (a real trap)

ninja's `apk` target is up to date as soon as
`android-build/simsapadhammareader.apk` exists and no *source* changed. A Gradle
project property is not one of its inputs. So building `--beta` and then plain
`--apk` in the same directory skipped androiddeployqt entirely and reported the
**previous** artifact: a plain release build printing
`package: name='io.github.simsapa.app.beta'`. Observed, not theorised.

`build-android.sh` therefore records the identity
(`<BuildType>-beta<0|1>-sign<0|1>`) in
`$ANDROID_BUILD_DIR/.simsapa-package-identity` after each successful package, and
when it changes — **or is unknown**, which covers a build directory predating the
guard or an interrupted run — deletes the packaging outputs to force
androiddeployqt and Gradle to run again.

The **signing state is part of the identity** for the same reason:
`make android-apk-debug` (unsigned) straight after `make android-beta-debug`
(release-signed) produces the same package id, so ninja would stay up to date
and the script would report the still-release-signed APK as an unsigned debug
build. The re-sign is applied to the artifact in place, so nothing else would
reveal it.

It deletes only *outputs* (`android-build/build/outputs`,
`android-build/simsapadhammareader.{apk,aab}`). It must never delete
`android-build/` itself: the per-ABI ExternalProject copy stamps live outside it
and would then consider themselves up to date, leaving the staging directory
unpopulated and the tree permanently wedged. Use `make android-clean` for that.

### Signing a debuggable build: `--sign`

`build-android.sh --apk --debug --sign` signs the debug variant with the release
keystore. Order-independent with `--debug`; `--no-sign` is the counterpart, and
whichever is given explicitly wins over the default `--debug` implies.

The signing happens **after** the build, with `apksigner`, rather than through
androiddeployqt's `--sign` (whose path is exercised for release variants only).
Gradle has already applied a debug-keystore signature by then; apksigner
replaces it. The script prints the resulting signer as proof rather than
trusting an exit status. 16 KB alignment survives re-signing (`zipalign -c -P 16`
still passes) and the `debuggable` flag is untouched.

`--aab --debug --sign` and `--aab --beta` are both rejected: an AAB is a Play
upload format, and the Play listing is the plain id.

## 3. Watching log messages from the command line

`adb logcat` *is* what Qt Creator's "Application Output" pane shows — it is a
filtered logcat and nothing more. There is no reason to deploy from Qt Creator
to read logs, and two reasons not to: its kits are single-ABI, and a Qt Creator
deploy triggers the spurious "This app isn't 16 KB compatible" dialog (see
`AGENTS.md`).

`make android-beta-debug-run` clears the log, launches the app and streams:

```sh
adb logcat -v brief simsapa:V Qt:V QtCore:V QtQml:V AndroidRuntime:E DEBUG:E '*:S'
```

- `simsapa` — the Rust backend. `backend/src/logger.rs` initialises
  `android_logger` with `.with_tag("simsapa")` at `LevelFilter::Debug` and routes
  `tracing` output through it, ANSI colour codes and all.
- `Qt` / `QtCore` / `QtQml` — Qt's own message handler, which is also where the
  QML `Logger` module's output arrives.
- `AndroidRuntime` / `DEBUG` — Java exceptions and native crash traces.

**Filtering by tag, not by pid, is deliberate**: a pid filter cannot be
established until the process exists, which loses exactly the startup messages
worth reading. Drop the trailing `'*:S'` if something expected is missing — an
unlisted tag is the usual reason.

Because the beta is debuggable, `adb shell run-as io.github.simsapa.app.beta`
also works for inspecting its data directory. That is refused for the Play
build, which is not debuggable.

## 4. The in-app update notice and Google Play policy

Distributing the app outside Play is fine — Play has no exclusivity requirement,
the beta is a different application id that simply is not listed there, and
Apache-2.0 puts no obstacle in the way. Signing GitHub builds with the upload key
is correct; Play App Signing's key stays on Google's side.

The constraint is narrower: **an app distributed through Play must update only
through Play** (Device and Network Abuse). Offering a Play-installed user a
download link to an APK hosted elsewhere is the pattern that policy targets.
Simsapa's app-update dialog did exactly that on Android, with no platform gate.

The fix is to discriminate on **how the copy was installed**, not on which
artifact it is:

```
SuttaBridge.is_installed_from_play_store()   // getInstallerPackageName() == "com.android.vending"
SuttaBridge.get_play_store_url()             // market://details?id=<running package>
```

backed by `get_installer_package_name()` / `get_android_package_name()` in
`cpp/utils.cpp` (same JNI pattern as `get_status_bar_height()`).

`UpdateNotificationDialog.qml` then branches:

| install source | shown |
|---|---|
| Google Play | "This copy was installed from Google Play. Open the Simsapa page in the Play Store and tap Update there." + **Open Google Play** button |
| anything else (sideloaded release, GitHub beta, desktop) | the release-page URL + **Open Link** button, as before |

**Hiding the button is not sufficient on its own.** The dialog also renders the
release notes — the GitHub release *description*, server-supplied, built in
`update_checker.rs` — as RichText with a live `onLinkActivated` handler. A link
written into that description would still carry a Play user to the download
page. Every link in the dialog therefore goes through
`open_release_link()`, which is inert on a Play install, leaving the
**Open Google Play** button as the only way out. Consequence when writing release
descriptions: on Play installs their links are not clickable, though the URL text
is still readable.

Why installer-based rather than a build-time flag: the same release APK
downloaded from GitHub and sideloaded is byte-identical to what Play serves, and
that copy is *not* covered by the policy — it should keep the link. Only the copy
that actually came from Play is restricted. `getInstallerPackageName()` returns
null for a sideloaded package (`QJniObject` reports it invalid, hence the empty
string), and the bridge returns false off Android, so desktop is unchanged.

The `market://` URL is built from the **running** package name, so a beta
resolves to a beta listing rather than being hardcoded to the release one, with
the `https://play.google.com/store/apps/details?id=…` form as fallback if
nothing claims the scheme. `open_visit_url()` routes through the same gate so a
future caller cannot reintroduce an off-Play link.

Deprecation note: `getInstallSourceInfo()` replaced `getInstallerPackageName()`
in API 30. The latter is deprecated but functional on every level the app
supports (minSdk 27), so it is called directly rather than branched on.

Two things this does **not** cover, deliberately:

- The **database/asset** update path is untouched. Those downloads are data, not
  executable code, and are outside the policy.
- Play's In-App Updates API would be the fully-blessed mechanism. It needs a
  Play Core dependency and was judged overkill; a link to our own listing is a
  standard, compliant pattern.

## 5. Testing checklist for a beta

```sh
make android-beta-dist          # -> dist/Simsapa-<version>-beta.apk, not debuggable
aapt2 dump badging dist/Simsapa-<version>-beta.apk | grep -E 'package|label|debuggable'
```

Expect `io.github.simsapa.app.beta`, `application-label:'Simsapa (beta)'`, and
**no** `application-debuggable` line. Then, for local work:

```sh
make android-beta-debug
make android-beta-debug-install
make android-beta-debug-run
```

After any change to the beta wiring, round-trip the identities in one build
directory (`--beta` → plain `--apk` → `--beta`) and check `aapt2 dump badging`
each time. That is the only check that catches the stale-artifact trap in §2.
