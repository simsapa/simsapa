# Android API levels and feature dependencies

What Android API level each part of Simsapa actually requires, measured rather
than assumed. Two uses:

1. **Deciding `minSdkVersion`** — what a raise costs, and what the current floor
   is really buying.
2. **Triaging a user crash report** — a user says the app crashes or a feature
   does nothing; §7 turns their Android version into a list of candidate causes.

Everything in §2 and §3 is reproducible with the commands given; re-run them
after any Qt upgrade, NDK change, or new native crate.

Companion documents:

- [android-qt-upgrade-considerations.md](./android-qt-upgrade-considerations.md)
  §2.2 — the `minSdkVersion` decision history.
- [android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md) — the
  manifest, permissions and `<uses-feature>` rules.
- [pure-rust-audio-backend.md](./pure-rust-audio-backend.md) — the audio stack
  and the NDK r28 exclusion (a *compile-time* constraint, unrelated to the
  runtime floors here).
- [android-file-saving-saf.md](./android-file-saving-saf.md),
  [relocated-storage-recovery.md](./relocated-storage-recovery.md),
  [file-selection-test.md](./file-selection-test.md) — the storage and picker
  code whose API calls are catalogued in §5.
- [android-edge-to-edge-and-safe-areas.md](./android-edge-to-edge-and-safe-areas.md)
  — the `targetSdkVersion`-driven behaviours in §6.

---

## 1. Current declared levels

| Setting | Value | Where it is declared |
|---|---|---|
| `minSdkVersion` | **28** (Android 9) | `android/build.gradle` `defaultConfig` — **the only source**. Raised from 27 on 2026-08-26 because of §2 |
| `targetSdkVersion` | 36 (Android 16) | `android/build.gradle` `defaultConfig` |
| `compileSdk` | android-36 | written by androiddeployqt; it picks the newest installed platform |
| NDK | r27 (27.3.13750724) | `build-android.sh`; **not r28** |
| Qt | 6.9.3 | `CMakeLists.txt` `QT_ANDROID` |

There is **no `<uses-sdk>` element** in `android/AndroidManifest.xml` and no
`QT_ANDROID_MIN_SDK_VERSION` in `CMakeLists.txt`. Verify the built artifact with
`aapt2 dump badging`, never by reading the generated
`android-build/gradle.properties` — androiddeployqt writes values there that
`build.gradle` never reads.

---

## 2. The measured native floor: **the app cannot load below API 28**

This is the headline finding, and it was not known when `minSdkVersion 27` was
last discussed.

Every `.so` in the APK was scanned for **undefined** (imported) symbols and
compared against the NDK's per-API-level stub libraries. Exactly one hard
requirement above API 27 exists in the whole package:

| Library | Symbol | Binding | First available |
|---|---|---|---|
| `libQt6Core_arm64-v8a.so` | `getentropy@LIBC_P` | **GLOBAL** | **API 28** |

`GLOBAL` undefined means the dynamic linker *must* resolve it. On an API 27
device `dlopen("libQt6Core…so")` fails with `cannot locate symbol "getentropy"`,
and the app dies during library loading — before any Qt or app code runs. The
`@LIBC_P` version tag is bionic's own marker for "Android P", i.e. API 28.

**Consequence: `minSdkVersion 27` was not merely one level below what Qt
declares — it was a promise the binary could not keep.** Google Play was
offering the app to Android 8.1 devices on which it could not start. Raising the
floor to 28 dropped no working users; it stopped advertising to users for whom
it was never going to work.

> **Resolved 2026-08-26.** `android/build.gradle` now declares
> `minSdkVersion 28`, verified on the artifact with `aapt2 dump badging`
> (`minSdkVersion:'28'`, `targetSdkVersion:'36'`). This finding is what
> decoupled the raise from the Qt upgrade it had been deferred to since
> 2026-07-29 — reframing it from a distribution trade-off into a correctness
> fix. See
> [android-qt-upgrade-considerations.md §2.2](./android-qt-upgrade-considerations.md).

### 2.1 Symbols above the floor that are safe (weak-linked)

Weak undefined symbols resolve to null when absent, and the calling code takes a
fallback path. These are **not** a floor:

(The table is as measured against the old floor of 27, which is why `getrandom`
appears in it. At today's floor of 28 that one is no longer above the floor at
all; the rest still are.)

| Library | Symbol | Binding | First available | Fallback |
|---|---|---|---|---|
| `libsimsapadhammareader` | `getrandom` | WEAK | API 28 | Rust's `getrandom` crate reads `/dev/urandom` (its `use_file` code is compiled into our binary) |
| `libsimsapadhammareader` | `memfd_create` | WEAK | API 30 | Rust std falls back to a temp file |
| `libsimsapadhammareader` | `copy_file_range` | WEAK | API 34 | Rust std falls back to read/write |
| `libsimsapadhammareader` | `ZSTD_trace_*` (4) | WEAK | never in bionic | zstd's optional tracing hooks; absent by design |

**Everything else the app imports from the NDK is API 26 or lower** — the whole
AAudio surface (`AAudio_createStreamBuilder`, `AAudioStream_*`,
`AAudioStreamBuilder_*`), the asset manager (`AAsset*`), and `ANativeWindow_*` /
`ANativeActivity_*`. `libaaudio.so` is a hard `NEEDED` entry, which sets a
**native audio floor of API 26**; it is below the Qt floor and therefore not the
binding constraint.

### 2.2 How to re-run this scan

**Use `scripts/android-api-scan.sh`** — it regenerates every measurement in this
document, and `--markdown` prints §2's two tables in the form they appear above.

```sh
./scripts/android-api-scan.sh                      # everything, newest APK under build/
./scripts/android-api-scan.sh --floor 28           # test a proposed minSdk
./scripts/android-api-scan.sh --sections symbols --markdown
./scripts/android-api-scan.sh --abi x86_64 --apk dist/Simsapa-x.y.z.apk
./scripts/android-api-scan.sh --help
```

Sections: `declared` (build.gradle's levels), `qt` (§3's three pieces of
evidence, read from the installed kit), `symbols` (§2), `jni` (§5's call-site
inventory), `manifest` (§5.8). The symbol scan takes about a minute; it reads
only, and changes nothing.

**Two flags are effectively mandatory for a verdict you can trust.** The script
scans **one ABI at a time** and defaults to `arm64-v8a`, and with no `--apk` it
picks the *newest* APK anywhere under `build/` — which may be an unrelated
arm64-only build. Always pass both, and loop the ABIs:

```sh
for abi in arm64-v8a x86_64 armeabi-v7a; do
    ./scripts/android-api-scan.sh --apk "$apk" --floor 28 --abi "$abi"
done
```

`minSdkVersion` binds **every** ABI, so a floor confirmed on arm64 alone is not
confirmed.

#### The confirming run for `minSdkVersion 28` (2026-08-26)

Against a multi-ABI beta APK, NDK 27.3.13750724, 98 libraries per ABI:

```
arm64-v8a     OK  no hard (GLOBAL) undefined symbol requires more than API 28
x86_64        OK  no hard (GLOBAL) undefined symbol requires more than API 28
armeabi-v7a   OK  no hard (GLOBAL) undefined symbol requires more than API 28
```

This is the check that **28 is a *sufficient* floor, not merely a higher one**.
Note the `getentropy` finding above was measured on `libQt6Core_arm64-v8a.so`;
before this run the x86_64 and armeabi-v7a slices had **never been scanned at
all**, and neither turned out to require a higher floor.

The WEAK references above 28 differ slightly per ABI, which is expected and is
not a floor — arm64 carries `copy_file_range` (34), `memfd_create` (30) and the
four `ZSTD_trace_*`; x86_64 the same minus `memfd_create`; armeabi-v7a only
`copy_file_range`. All three link the same nine system libraries, `libaaudio.so`
among them (API 26, well under the floor).

The rest of this section is what the script does, kept because the mechanism is
the point and a one-off check may be quicker by hand.

```sh
NDK=~/Android/Sdk/ndk/27.3.13750724/toolchains/llvm/prebuilt/linux-x86_64
S=$NDK/sysroot/usr/lib/aarch64-linux-android

# Exported symbols per API level (union of every stub library at that level)
for lvl in 27 28 30 35; do
  for l in $S/$lvl/*.so; do
    llvm-readelf --dyn-syms --wide "$l" | awk '$4=="FUNC"||$4=="OBJECT"{print $8}' | sed 's/@.*//'
  done | sort -u > /tmp/plat-$lvl.txt
done
comm -13 /tmp/plat-27.txt /tmp/plat-28.txt > /tmp/api28only.txt   # added in 28

unzip -q -o <app>.apk 'lib/arm64-v8a/*' -d /tmp/apkx && cd /tmp/apkx

# Which libraries import a symbol that does not exist before API 28?
for f in lib/arm64-v8a/*.so; do
  h=$(llvm-readelf --dyn-syms --wide "$f" | awk '$7=="UND"{print $8}' | sed 's/@.*//' \
      | sort -u | comm -12 - /tmp/api28only.txt | tr '\n' ' ')
  [ -n "$h" ] && echo "$(basename $f): $h"
done

# Binding matters: GLOBAL is a hard requirement, WEAK is not.
llvm-readelf --dyn-syms --wide lib/arm64-v8a/libQt6Core_arm64-v8a.so | grep -w getentropy
```

Columns in `llvm-readelf --dyn-syms --wide` output are
`Num: Value Size Type Bind Vis Ndx Name`, so `$5` is the binding and `$7` is
`UND` for imports.

> **Scope of the measurement.** Run against the arm64-v8a debug APK from
> `build/android-arm64/`, Qt 6.9.3, NDK r27. The x86_64 and armeabi-v7a slices
> were **not** scanned — Qt builds them from the same sources with the same
> `ANDROID_PLATFORM`, so the same floor is expected, but that is an inference,
> not a measurement. Re-run per ABI if it ever matters.

---

## 3. Qt's own floor

### 3.1 Qt 6.9.3 — API 28, and it says so three ways

All three verified in the installed kit and sources:

| Evidence | Location |
|---|---|
| androiddeployqt's default `minSdkVersion` is `"28"` | `~/Qt/6.9.3/Src/qtbase/src/tools/androiddeployqt/main.cpp:175` |
| androiddeployqt **rejects** a manifest `<uses-sdk android:minSdkVersion>` below 28 — *"Invalid minSdkVersion version, minSdkVersion must be >= 28"* | `main.cpp:1996-1997` |
| Qt's own libraries are compiled against the API 28 platform (`ANDROID_PLATFORM "android-28"`) | `lib/cmake/Qt6/qt.toolchain.cmake:44`, `QtAutoDetectHelpers.cmake:141` |

The third is the mechanism behind §2: Qt's libraries are linked against API 28
headers, so they may reference bionic symbols added in 28 — and `libQt6Core`
does exactly that with `getentropy`.

**Why our override slips past the check.** androiddeployqt validates the
`<uses-sdk>` element in `android/AndroidManifest.xml`. Our manifest has no
`<uses-sdk>` element; the 27 lives in `android/build.gradle`'s `defaultConfig`,
which androiddeployqt never inspects. It writes `qtMinSdkVersion=28` into the
generated `gradle.properties`, and Gradle then overrides it. So the override is
not sanctioned by Qt — it is invisible to Qt's only guard.

### 3.2 Qt 6.11 — still API 28

Qt 6.11's documentation states the supported distribution range as
**Android 9 (API 28) to Android 16 (API 36)** — the same floor as 6.9.3.

**So an upgrade to 6.11 does not by itself force the floor up.** But Qt's
[supported-versions guidelines](https://doc.qt.io/qt-6/android-supported-versions-selection-guidelines.html)
say the minimum is re-evaluated **once a year, for the autumn release**, against
"at least 90% of cumulative usage" on apilevels.com. The floor therefore moves
on a schedule, not with any particular version — expect a rise at some future
autumn release, and check the target version's own documentation rather than
assuming it inherits 28.

Note the guideline's other half: Qt's binaries are *built* at the declared
minimum, so targeting below it needs a Qt rebuild. That is exactly the situation
§2 measures — we are shipping Qt binaries below the level they were built for,
and it does not work.

---

## 4. Summary of floors

| Constraint | Level | Kind |
|---|---|---|
| `libQt6Core` `getentropy` | **28** | **Hard — app fails to load below this** |
| Qt 6.9.3 declared / build platform | 28 | Qt's own support statement |
| Qt 6.11 declared | 28 | unchanged from 6.9.3 |
| AAudio (`libaaudio.so`, hard `NEEDED`) | 26 | Hard, but below the Qt floor |
| Storage volume enumeration (`StorageManager.getStorageVolumes`) | 24 | Degrades: fewer candidates listed |
| SAF document tree writes (`DocumentsContract` tree helpers) | 21 | Degrades: file save fails |
| Everything else in §5 | ≤ 21 | Not a constraint at any supported level |
| `minSdkVersion` as declared | **28** | **Matches the hard floor.** Was 27 — below it, the defect this document was written to surface — until it was raised on 2026-08-26 |

---

## 5. Feature → API inventory

Java-side levels are from the AOSP documentation for each class; native levels in
§2 are measured. "If missing" describes what a user would actually see on a
device below that level.

### 5.1 Startup and process-level

| Area | Call / dependency | API | Source | If missing |
|---|---|---|---|---|
| Qt initialisation | `getentropy` (bionic) | **28** | `libQt6Core` (Qt's code, not ours) | **App does not start** — `dlopen` fails |
| Random numbers | `getrandom` (weak) | 28 | Rust `getrandom` crate | Nothing; `/dev/urandom` fallback |
| Device API level probe | `__system_property_get("ro.build.version.sdk")` | any | `backend/src/storage_diagnostics.rs:1892` | Reports `null` in diagnostics |
| Logging | `liblog` `__android_log_*` | any | `android_logger` crate | — |

### 5.2 Audio (chanting practice: record and playback)

| Area | Call / dependency | API | Source | If missing |
|---|---|---|---|---|
| Recording + playback | AAudio (`libaaudio.so`, `AAudio_createStreamBuilder`, `AAudioStream_*`) | **26** | `cpal` 0.18 Android backend, via `ndk-context` | Library load failure; no audio at all |
| JavaVM/Context handoff | `ndk_context::initialize_android_context` | n/a | `backend/src/lib.rs:187-200` (called from `cpp/gui.cpp` after `QApplication`) | First audio stream build panics |
| Microphone | `RECORD_AUDIO` permission (runtime-granted since API 23) | 1 / 23 | `android/AndroidManifest.xml` | Recording denied |
| Routing | `MODIFY_AUDIO_SETTINGS` | 1 | manifest | — |

No audio *library* is bundled — AAudio is a system library. See
[pure-rust-audio-backend.md](./pure-rust-audio-backend.md).

### 5.3 Window and activity

| Area | Call | API | Source | If missing |
|---|---|---|---|---|
| Keep screen on during downloads / index rebuilds | `Activity.getWindow()` → `Window.addFlags` / `clearFlags` with `FLAG_KEEP_SCREEN_ON` (0x80) | 1 | `cpp/screen.cpp:69-87` | Screen sleeps mid-operation |
| Minimise instead of exit | `Activity.moveTaskToBack(boolean)` | 1 | `cpp/app_minimize.cpp:49` | Back would exit |
| Open system display settings | `Intent(Settings.ACTION_DISPLAY_SETTINGS)` + `Activity.startActivity` | 1 | `cpp/android_helpers.cpp:30-41` | Button does nothing |
| Status bar height (informational only) | `Resources.getIdentifier` / `getDimensionPixelSize` / `getDisplayMetrics` | 1 | `cpp/utils.cpp:42-90` | Reports 0; **not** the safe area — see [android-edge-to-edge-and-safe-areas.md](./android-edge-to-edge-and-safe-areas.md) |

### 5.4 Storage location and recovery

Covered by [relocated-storage-recovery.md](./relocated-storage-recovery.md).
`cpp/utils.cpp` deliberately confines itself to **API 24 and earlier** calls.

| Area | Call | API | Source | If missing |
|---|---|---|---|---|
| Enumerate app-writable external dirs | `Context.getExternalFilesDirs(null)` | 19 | `cpp/utils.cpp:474-495` | No SD-card candidate offered |
| Volume mounted state | `Environment.getExternalStorageState(File)` | 21 | `cpp/utils.cpp:184-200` | Candidate cannot be classified |
| Emulated / removable classification | `Environment.isExternalStorageEmulated(File)` / `isExternalStorageRemovable(File)` | 21 | `cpp/utils.cpp:212, 232` | Emulated duplicates not de-duplicated |
| Volume for a path | `StorageManager.getStorageVolume(File)` | 24 | `cpp/utils.cpp:283-300` | Falls through to the description/`isPrimary` fallbacks |
| Volumes the app cannot write to | `StorageManager.getStorageVolumes()`, `StorageVolume.isPrimary/getUuid/getDescription(Context)` | 24 | `cpp/utils.cpp:304-420` | SAF-only volumes not reported in the scan |
| *(deliberately unused)* | `StorageVolume.getDirectory()` | 30 | — | Would be simpler; above the floor, so not used |

### 5.5 File saving and reading via SAF

Covered by [android-file-saving-saf.md](./android-file-saving-saf.md).

| Area | Call | API | Source | If missing |
|---|---|---|---|---|
| Folder picker | `ACTION_OPEN_DOCUMENT_TREE` (via Qt `FolderDialog`) | 21 | QML `FolderDialog` | Cannot choose a save folder |
| Tree → document id | `DocumentsContract.getTreeDocumentId` | 21 | `backend/src/android_saf.rs:132` | Save fails |
| Enumerate children | `DocumentsContract.buildChildDocumentsUriUsingTree` + `ContentResolver.query` | 21 | `android_saf.rs:156, 183` | Overwrite detection fails |
| Address a child | `DocumentsContract.buildDocumentUriUsingTree` | 21 | `android_saf.rs:231, 576` | Save fails |
| Create a file | `DocumentsContract.createDocument` | 21 | `android_saf.rs:594` | Save fails |
| Write / read bytes | `ContentResolver.openOutputStream` / `openInputStream` | 1 | `android_saf.rs:620, 427` | Save/read fails |
| Parse the URI | `Uri.parse` | 1 | `android_saf.rs:121, 396` | — |
| File picker (imports) | `ACTION_OPEN_DOCUMENT`, `Intent.getData` / `getClipData` | 19 / 16 | `cpp/android_raw_pick.cpp:152-193` | Import picker returns nothing |
| Copy a picked file to temp | `ContentResolver.query` for `_display_name`, then stream copy | 1 | `cpp/utils.cpp:593-645` | Imported file gets an opaque name |

### 5.6 Packaging and update policy

| Area | Call | API | Source | If missing |
|---|---|---|---|---|
| Was this installed from Play? | `PackageManager.getInstallerPackageName(String)` | 5 (deprecated at 30, still functional) | `cpp/utils.cpp:751-786` | Update dialog shows the GitHub link instead of `market://` |
| *(deliberately unused)* | `PackageManager.getInstallSourceInfo` | 30 | — | The replacement API; above the floor |
| Package name | `Context.getPackageName` | 1 | `cpp/utils.cpp:710` | — |

### 5.7 Web content

| Area | Dependency | API | Notes |
|---|---|---|---|
| Sutta / dictionary reader panels | `android.webkit.WebView` via Qt WebView (`libQt6WebView`, `libplugins_webview_qtwebview_android`) | 1 | **The system WebView updates through Play independently of the Android version.** A rendering bug is far more likely to track the *WebView* version than the API level — capture both when triaging. See [mobile-webview-visibility-management.md](./mobile-webview-visibility-management.md) |

### 5.8 Permissions declared

All are `normal`- or runtime-permission entries with no API-level constraint of
their own; the manifest declares them explicitly because the androiddeployqt
injection markers were removed
([android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md)).

`INTERNET`, `ACCESS_NETWORK_STATE`, `RECORD_AUDIO`, `MODIFY_AUDIO_SETTINGS`.

`WRITE_EXTERNAL_STORAGE` is **deliberately absent**. It is genuinely required on
API 27–28 to write *shared* external storage, and the app never does: all
user-chosen I/O goes through SAF `content://` URIs, everything else to the
app-private directory. Raising the floor to 28 keeps the app inside that same
band and changes nothing here. Re-run the
`getExternalStorage` / `EXTERNAL_STORAGE` / `/sdcard` grep across `backend/`,
`bridges/`, `cpp/`, `bridges/assets/qml/` and `android/` if a direct filesystem path is
ever reintroduced.

---

## 6. `targetSdkVersion`-driven behaviours (not API floors)

These are triggered by `targetSdkVersion 36` and apply on **every** device
running that level or higher, regardless of `minSdkVersion`. They are a common
source of "it only misbehaves on new phones" reports.

| Behaviour | Enforced from | How the app handles it |
|---|---|---|
| Edge-to-edge layout | 35 | Qt binds `ApplicationWindow`'s padding to the safe area; the app's top-margin setting is *extra* space only. [android-edge-to-edge-and-safe-areas.md](./android-edge-to-edge-and-safe-areas.md) |
| Predictive back gesture | 36 | **Opted out** with `android:enableOnBackInvokedCallback="false"` — Qt 6.9.3 registers no `OnBackInvokedCallback`, so without the opt-out Back closes the whole app from any dialog |
| Scoped storage | 29–30 | Nothing to do: SAF only |

---

## 7. Crash-triage playbook

When a user reports a crash or a dead feature:

**Step 1 — get the device API level.** Ask for the Storage Diagnostics report
(**About → Run Storage Diagnostics**, or Database Validation): it carries
`android_api_level` and `storage_path` among other things. Failing that, the
Android version name maps to a level (8.1 → 27, 9 → 28, 10 → 29, 11 → 30,
12 → 31, 13 → 33, 14 → 34, 15 → 35, 16 → 36).

**Step 2 — is it below 28?** If so, stop: the app cannot load (§2). The logcat
signature is a linker error naming `getentropy`, before any `simsapa` log line
exists. Nothing else needs investigating.

**Step 3 — match the symptom to §5.** Find the feature area; the "If missing"
column says what the level-related failure looks like. A mismatch there means
the API level is probably *not* the cause.

**Step 4 — read the failure shape, which tells you which side failed.**

| Shape | Meaning |
|---|---|
| `dlopen failed: cannot locate symbol "…"`, no app log lines | A **native** symbol above the device's level — §2's category. Fatal, at load time |
| `java.lang.NoSuchMethodError` / `NoSuchFieldError` in logcat | A **Java** method above the device's level. `QJniObject` returns an *invalid* object and leaves a pending JNI exception |
| A feature silently does nothing, no crash | Most likely the same Java case, already handled: our call sites check `isValid()` and call `QJniEnvironment::checkAndClearExceptions()` (10 sites in `cpp/utils.cpp`, 4 in `cpp/android_raw_pick.cpp`, 1 in `cpp/android_helpers.cpp`; the Rust SAF path uses `env.exception_clear()` at 8 sites) |
| A crash *later*, unrelated to the call | A pending JNI exception that was never cleared. `cpp/screen.cpp` and `cpp/app_minimize.cpp` do not clear exceptions — acceptable today because every call they make is API 1, but that is the pattern to check if either grows |

**Step 5 — if it is a rendering or in-page problem**, get the **Android System
WebView** version (Settings → Apps → Android System WebView), not just the OS
version. It updates independently through Play (§5.7).

**Step 6 — if you suspect a native symbol but cannot get logcat**, re-run the
§2.2 scan against the exact APK the user installed, using their API level as the
comparison target. That answers "could this build have loaded on their device?"
without the device.

---

## 8. Consequences of the raise to `minSdkVersion 28` — **done 2026-08-26**

What was predicted here before the change, and what the change actually did.
Every prediction held; nothing needed revisiting.

- **In-app code:** none, as expected. Every call in §5 is at API 26 or below
  except the storage helpers at 24, and the only >27 native requirement was Qt's
  own.
- **Correctness:** the app stopped being offered to devices where it cannot load
  (§2). This was the actual benefit, and it is larger than the "align with Qt's
  declared floor" framing that preceded this measurement — that framing is what
  had coupled the change to a Qt upgrade for a year.
- **NDK:** unchanged, and **not to be re-litigated**. The r28 exclusion is about
  `pthread_cond_clockwait`, which bionic declares at **API 30** — still above
  28. `build-android.sh` already said so at both its NDK-pin sites and needed no
  edit; `CMakeLists.txt` and `android/build.gradle`'s comments were reworded to
  match.
- **`WRITE_EXTERNAL_STORAGE`:** unchanged (§5.8). Re-audited on the day: 28 is
  still inside the API 27–28 band where the permission would be required, and
  the app still never touches shared external storage, so the rationale is
  re-affirmed rather than retired.
- **Distribution:** Google Play stops offering *updates* to devices below API 28.
  Existing installs keep the last compatible version and are **not**
  uninstalled. Sideloaded APKs will refuse to install on API 27.
- **Verification:** confirmed on the artifact —
  `aapt2 dump badging <apk> | grep -i sdkversion` reports `minSdkVersion:'28'`
  and `targetSdkVersion:'36'`. The per-ABI confirming scan is in §2.2.
- **Build system:** the multi-ABI build produced all three slices with no
  complaint about the floor, and the script's own artifact checks (cross-ABI
  contamination, ChromeOS required-features) passed unchanged.

---

## 9. Standing rules: keeping this document true

**These two are rules, not suggestions.** The `getentropy` defect existed for a
year because nobody scanned the binaries, and §5's inventory is only worth
consulting during a crash triage if it is complete:

> **Rule 1 — every new JNI call site records its API level in §5.** Adding a
> `QJniObject` call without adding its row silently degrades §7's triage
> playbook into guesswork, and the omission is invisible until a user on an old
> device reports something that "does nothing".
>
> **Rule 2 — the §2.2 symbol scan is re-run on any Qt or NDK change**, once per
> ABI, and its verdict recorded. A new floor arrives silently: nothing in the
> build fails, and the first symptom is a device that will not start the app.

`./scripts/android-api-scan.sh` regenerates all of it. Re-run it whenever:

- **Qt is upgraded** — Qt's `ANDROID_PLATFORM` and its libraries' symbol
  requirements move together, and a new floor arrives silently.
- **The NDK changes** — the stub libraries and the default platform level both
  change.
- **A new native crate is added** (audio, compression, crypto, filesystem).
  Rust crates commonly weak-link newer syscalls, which is safe, but not always.
- **A new `QJniObject` / JNI call site is added** — `--sections jni` lists every
  call site with its class and method, so nothing is missed; look each new one
  up in the AOSP documentation, record its level in §5, and prefer the oldest
  API that does the job (as `cpp/utils.cpp` does by refusing
  `StorageVolume.getDirectory`). The script cannot supply Java-side API levels —
  they are not recoverable from our sources — so that lookup stays manual.

Sources for the Qt claims:
[Qt for Android](https://doc.qt.io/qt-6/android.html) (6.11 supported
configurations) and
[Qt for Android Supported Versions Selection Guidelines](https://doc.qt.io/qt-6/android-supported-versions-selection-guidelines.html).
