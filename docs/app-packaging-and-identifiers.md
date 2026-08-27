# App packaging and identifiers

This document covers the identifiers used when packaging Simsapa for the
different platforms (Linux, Windows, macOS, Android), and — importantly — the
distinction between the **application identifier** (the store/OS package id) and
the **QML module name** (an internal Qt namespace). The two are unrelated even
though they have historically used the same `com.profoundlabs.simsapa` string.

## TL;DR

There are **two different identifiers** that look alike but mean completely
different things:

| | Application identifier | QML module URI |
|---|---|---|
| Example value | `io.github.simsapa.app` | `com.profoundlabs.simsapa` |
| Audience | Google Play, App Store, the OS | internal to the Qt/cxx-qt build |
| Purpose | uniquely names the installed app | namespaces QML imports & registered types |
| Must be globally unique? | Yes (store requirement) | No (only unique within the app) |
| Safe to change? | Yes, in a few files | High-risk refactor across ~70 sites; no benefit |

**They do not need to match.** Changing one does not require changing the other.

## 1. Application identifier (the "app id" / package name)

This is the reverse-DNS string that uniquely identifies the *installed
application* to an app store or the operating system. For Google Play this is
the **applicationId / package name**; on macOS it is the **bundle identifier**.

Current value: **`io.github.simsapa.app`**
(previously `com.profoundlabs.simsapa`; changed for the Google Play release).

### Where it is set, per platform

| Platform | File | Field |
|---|---|---|
| **Android** | `android/AndroidManifest.xml` | `package="io.github.simsapa.app"` |
| **macOS** | `CMakeLists.txt` | `MACOSX_BUNDLE_GUI_IDENTIFIER` |
| **macOS** | `build-macos.sh` | `BUNDLE_ID` (overwrites `CFBundleIdentifier` in the bundled `Info.plist`) |
| **Windows** | `simsapa-installer.iss` | `AppId` — a **GUID**, *not* a reverse-DNS string |
| **Linux** | `simsapa.desktop` | no reverse-DNS id; uses `Name=Simsapa`, `Exec=simsapadhammareader` |

Notes:

- **Android is the source of truth on Android.** `androiddeployqt` reads the
  `package` attribute from `AndroidManifest.xml` and injects it into Gradle as
  the `androidPackageName` property, which `android/build.gradle` consumes as
  `namespace`. Things that derive **automatically** from the package and need no
  separate edit:
  - the FileProvider authority `android:authorities="${applicationId}.qtprovider"`
  - the app's private data directory `/data/user/0/<package>/files/…` and the
    external `…/Android/data/<package>/files` path (these come from the OS at
    runtime; Rust code reads them via the platform APIs, it does not hardcode
    the package — see the path comments in `backend/src/lib.rs` and
    `bridges/src/storage_manager.rs`).
- **macOS** keeps the id in two places — keep `CMakeLists.txt` and
  `build-macos.sh` in sync. If you code-sign / notarize, the provisioning
  profile must match the bundle id.
- **Windows** has no reverse-DNS application id. The Inno Setup `AppId` is a
  generated GUID and is unrelated to the Android/macOS identifier — do **not**
  change it to a reverse-DNS string.
- **Linux** (AppImage + `.desktop`) has no reverse-DNS id either. (If a Flatpak
  is ever shipped it would want an `io.github.simsapa.app` appstream id and a
  matching `.desktop` filename, but no Flatpak packaging exists today.)

### Consequence of changing the Android package

Changing the Android `package` makes Android treat it as a **different app**: a
new private data directory, and a fresh install rather than an upgrade-in-place
over any previously-sideloaded `com.profoundlabs.simsapa` build. This is the
expected/correct behaviour for a first Google Play publication.

## 2. QML module URI (internal Qt namespace — leave it alone)

`com.profoundlabs.simsapa` is **also** the URI of the app's QML module — the
namespace under which QML components and the Rust/cxx-qt bridge types are
registered and imported. This is **purely internal**: it never appears to end
users and is invisible to any app store. It is **not** an application
identifier, and it does not need to equal the package name.

It appears in roughly 70 places that all must agree with each other:

- `import com.profoundlabs.simsapa` at the top of ~50 QML files in `bridges/assets/qml/`
- the type-stub / `qmldir` directory `bridges/assets/qml/com/profoundlabs/simsapa/`
  (the qmllint stubs for the Rust bridges, e.g. `SuttaBridge.qml`)
- the cxx-qt module registration:
  - `bridges/build.rs` → `QmlModule { uri: "com.profoundlabs.simsapa", … }`
  - `CMakeLists.txt` → `cxx_qt_import_qml_module(… URI "com.profoundlabs.simsapa" …)`
- the generated Qt resource paths that embed the URI, e.g.
  `:/qt/qml/com/profoundlabs/simsapa/assets/qml/…` referenced from `cpp/*.cpp`,
  `cpp/gui.cpp`, and `assets/icons.qrc`

**Recommendation: do not rename the QML module URI.** It is load-bearing across
the C++/Rust/QML/qrc resource-path machinery, renaming it touches ~70 files plus
a directory move, and it yields zero benefit for store publication. Keeping the
QML URI as `com.profoundlabs.simsapa` while the application id is
`io.github.simsapa.app` is perfectly valid and intentional.

## 3. Checklist: changing the application identifier

When the store package id needs to change (e.g. the Play release):

1. `android/AndroidManifest.xml` — `package=`
2. `CMakeLists.txt` — `MACOSX_BUNDLE_GUI_IDENTIFIER`
3. `build-macos.sh` — `BUNDLE_ID` (and the `--bundle-id` help text)
4. Update the explanatory path comments in `backend/src/lib.rs` and
   `bridges/src/storage_manager.rs` for accuracy (runtime paths come from the OS;
   these are comments only).
5. Do **not** touch the QML module URI, the Inno Setup `AppId` GUID, or the
   Linux `.desktop` file.
6. Rebuild: Android via Qt Creator (regenerates the Gradle namespace from the
   manifest); macOS via `make macos`. Linux/Windows builds are unaffected.
