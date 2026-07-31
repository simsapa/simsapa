# Android edge-to-edge and safe areas

How Simsapa keeps its UI clear of the status bar, navigation bar and display
cutout on Android, and why the app's own margin setting is only *extra* space on
top of what Qt already provides.

Companion documents:

- [android-qt-upgrade-considerations.md](./android-qt-upgrade-considerations.md)
  — deferred work, including the predictive-back opt-out this document explains.
- [android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md) —
  packaging, permissions and the targetSdk 36 move.
- [android-soft-keyboard.md](./android-soft-keyboard.md) — the related
  input-method quirks.

---

## 1. Qt supplies the inset; the app supplies only extra

At targetSdk 36 Android 16 enforces edge-to-edge with **no opt-out**: the
activity is laid out underneath the system bars. Clearance is not something the
app arranges — **Qt already does it.**

`QQuickApplicationWindow` binds its four padding properties to the window's safe
area, in `qquickapplicationwindow.cpp`:

```cpp
installPropertyBinding(this, "leftPadding"_L1,   controlSafeArea, "margins.left"_L1);
installPropertyBinding(this, "topPadding"_L1,    controlSafeArea, "margins.top"_L1);
installPropertyBinding(this, "rightPadding"_L1,  controlSafeArea, "margins.right"_L1);
installPropertyBinding(this, "bottomPadding"_L1, controlSafeArea, "margins.bottom"_L1);
```

(Qt 6.9.3: the bindings at lines 802–805, installed by the
`installPropertyBinding` lambda defined at 785.) The safe area it reads reflects
the margins from the `QWindow`, any margins added to the content item, and the
margins Qt adds for the header, footer and menu bar.

So every `ApplicationWindow` in `assets/qml/` is padded correctly for free. The
app's `extra_top_margin` property is exactly what its name says: **additional**
space below Qt's inset, defaulting to `0`, for the rare device where the
automatic inset is not enough.

### Rule 1 — never assign `topPadding` or `padding` on an `ApplicationWindow` root

The binding above is installed with `binding.installOn(targetProperty)`
(`qquickapplicationwindow.cpp:793`). An explicit assignment in QML **replaces
that binding silently**, and the safe-area inset vanishes for that window with
no warning.

```qml
ApplicationWindow {
    topPadding: 24        // ❌ destroys Qt's safe-area binding for this window
}
```

Verified: no window does this today — every `padding:` in `assets/qml/` is on an
inner control. **This is the first thing to check if an inset ever goes
missing.** Apply extra space as an anchor margin on the window's root layout,
inside Qt's padding, which is what `extra_top_margin` does.

### Rule 1a — anchor the root content item; never size it from `root.width` / `root.height`

A direct child of an `ApplicationWindow` is reparented to its **`contentItem`**,
which Qt has already inset: its height is `root.height - topPadding -
bottomPadding`. Computing a size from the *window* dimensions therefore hands the
item more space than the content area has, and it overflows the bottom by
`topPadding + bottomPadding` minus whatever margin was subtracted — about **70 px**
on a phone with a ~34 dp status bar and a ~48 dp navigation bar. The top looks
correct (`y` is measured from the already-inset origin), so the symptom is
one-sided and easy to misread as a missing bottom margin.

```qml
ApplicationWindow {
    Item {
        x: 10
        y: 10 + root.extra_top_margin
        implicitWidth: root.width - 20                            // ❌
        implicitHeight: root.height - 20 - root.extra_top_margin  // ❌ overflows
    }
}
```

```qml
ApplicationWindow {
    Item {
        anchors.fill: parent                                 // ✅ the contentItem
        anchors.margins: 10
        anchors.topMargin: 10 + root.extra_top_margin
    }
}
```

`AnkiExportDialog.qml`, `DatabaseValidationDialog.qml`, `SystemPromptsDialog.qml`
and `ModelsDialog.qml` all had the broken form (fixed 2026-07-30). It went
unnoticed for a while because those windows also carried a mobile-only
`Layout.bottomMargin: 60` on their button rows — added back when the app produced
its own insets — which happened to cancel most of the overflow. Removing that 60
(now redundant, see §2) is what made the Database Validation window's lowest
buttons appear *under* the navigation bar. **The 60 was not the fix and its
removal was not the bug**; the anchoring was.

Every other window roots its content in a `Frame` or `StackLayout` with
`anchors.fill: parent` and was never affected.

---

## 2. Why the default had to become 0 — the doubled-gap diagnosis

The setting used to be `mobile_top_bar_margin`, a `SystemValue | CustomValue(u32)`
enum defaulting to **24 dp** on mobile. That default was written when Qt did
**not** apply safe-area padding for us; the app had to produce the whole inset
itself.

Qt gained the `ApplicationWindow` safe-area bindings in the 6.8.3 → 6.9.3 range.
From that point the app was adding its 24 dp *on top of* Qt's inset, so every
Android 15+ device showed a **doubled top gap** — Qt's real inset (~34 dp on a
typical phone) plus a hardcoded 24.

The fix was structural, not a number tweak:

- the setting became `mobile_extra_top_margin: u32`, defaulting to **0**;
- every QML owner's transient default became `0` too.

That second half matters for a reason easy to miss. Every owner's
`is_mobile ? 24 : 0` initializer was already overwritten on
`Component.onCompleted` by the bridge read, so it only affected the frames
before completion — which is exactly where the old default produced a **visible
jump at first paint**, from 24 to the resolved value. With both at `0`, first
paint matches the settled layout.

### Migration of persisted values

`AppSettings` is stored as one JSON blob, so old installs carry
`"mobile_top_bar_margin": "SystemValue"` or `{"CustomValue": 24}`. Serde ignores
unknown fields, so without a hook every user who had customised the value would
silently get `0`.

The carry-over (`SystemValue → 0`, `CustomValue(v) → v`) lives **inside
`Deserialize`**, not at a call site, because `AppSettings` is deserialized in
**three** places — `db/appdata.rs` (the in-app path that fills the settings
cache), `db/mod.rs` (a standalone pre-`QApplication` read used by `gui.cpp`), and
`app_data.rs` (the `import-me/app_settings.json` import). A hook at any one of
them would migrate only that copy. The legacy field is
`#[serde(skip_serializing)]`, so the old key is not written back out.

---

## 3. `status_bar_height` is not the safe area

`get_status_bar_height()` (`cpp/utils.cpp`) reads Android's
`android:dimen/status_bar_height` resource. It is **informational only** and must
not be used for layout:

- it excludes the **display cutout**, which can extend past the status bar;
- it says nothing about the **navigation bar** or rounded corners;
- it is a global system value, not per-window, so it is wrong for split-screen,
  freeform and desktop-mode windows.

Its one legitimate use is the read-only "System safe area: N dp" readout beside
the Extra Top Margin setting, so a user troubleshooting a covered toolbar can
see what the platform reports. Even there the QML prefers the window's own
`SafeArea.margins.top` and falls back to `get_status_bar_height()` only if the
attached property is unavailable — labelling itself differently in each case, so
the number is never misattributed.

---

## 4. What Qt does *not* pad

### Rule 2 — the `Popup` family gets no padding

`Popup`, `Dialog`, `Menu` and `Drawer` live in the window's **overlay**, not in
its content item, so the `ApplicationWindow` padding in §1 does not apply to
them. A popup that is top-anchored or tall enough to reach an edge must handle
its own inset.

`QQuickPopup` does implement `QQuickSafeAreaAttachable`
(`qquickpopup.cpp:3443`, returning `popupItem()`), so `SafeArea` attaches
correctly to a popup rather than warning.

**Worked example — `DrawerMenu.qml`.** It is a `Drawer` (a `Popup` subclass),
full-height (`height: control.window_height`), and its first child is a
top-anchored `Label { text: "Menu" }` — so on enforced edge-to-edge it landed
under the status bar and cutout, and it is the mobile main menu. The fix is on
the `Drawer` itself:

```qml
topPadding: control.SafeArea.margins.top
```

Not by plumbing `extra_top_margin` into it. The attached property is relative to
the item it is attached to, so this does not double-count.

**Sweep result (all 55 `Dialog`/`Popup`/`Drawer`/`Menu` roots in `assets/qml/`,
July 2026):** `DrawerMenu.qml` was the **only** top-anchored one. Every other
`Dialog` is centered (`anchors.centerIn: parent`, or `x/y: (parent.w|h - w|h) / 2`
in `TabListDialog.qml`), and the `Menu` popups in `SuttaSearchWindow.qml` open
from toolbar buttons that already sit below the inset.

Centered is not automatically safe, though: a centered dialog sized to nearly the
full window height leaves only a few px of clearance, which is less than a ~34 dp
inset. Watch-list, in descending order of exposure:

- `DocumentImportDialog.qml` — `height: Math.min(500, parent.height - 40)` → 20 px
  top clearance whenever the window is under ~540 px tall
- `DocumentMetadataEditDialog.qml` — same `parent.height - 40` form
- `TabListDialog.qml` — up to `parent.height * 0.9` when `!is_tall`
- `GlossTab.qml` — a fixed `height: 500` dialog
- `DatabaseValidationDialog.qml`, and `ChantingPracticeWindow.qml`'s
  `anchors.fill: parent` content dialogs

None were changed: the fix belongs on the edge a device actually shows a problem
on, and a phone screenshot decides it. The July 2026 on-device pass found no
problem with any of them.

### Other unpadded cases

- A mobile-visible `header` / `footer` / `menuBar` — Qt accounts for these in the
  safe area it computes for the content item, but content *inside* them is the
  app's business.
- `Flickable` / `ListView` content scrolling under an edge: the viewport is
  padded, the scrolled content is not clipped by it.

---

## 5. Predictive back (targetSdk 36)

targetSdk 36 also enables the **predictive back gesture** by default, which is
part of the same edge-to-edge enforcement package. It broke back navigation
outright, and the app currently opts out in `android/AndroidManifest.xml`:

```xml
android:enableOnBackInvokedCallback="false"
```

Predictive back stops the system dispatching legacy `KEYCODE_BACK` key events,
expecting an `OnBackInvokedCallback` instead. **Qt 6.9.3 registers none** (no
`onBackPressed` and no `OnBackInvokedCallback` anywhere in its Android Java), and
the app registers none either — Qt Quick Controls dismiss a
`Dialog`/`Popup`/`Window` off the `Qt::Key_Back` event the legacy path delivers,
so no handler was ever needed. With predictive back on, nothing handles back at
all and the system default finishes the activity.

Measured on an Android 16 phone (2026-07-28): back **closed the whole app** from
the sutta reader, the tab list dialog, the search help dialog, and the Chanting
Practice window. Confirmed fixed by the opt-out in versionCode 4.

The opt-out is temporary. Removing it, and what to re-test when Qt implements the
callback, is in
[android-qt-upgrade-considerations.md §2.1](./android-qt-upgrade-considerations.md).

---

## 6. Deprecated status/navigation bar APIs in Play's report

Google Play reports three deprecated APIs against the app. **All three are in
Qt's own Java**, are no-ops at API 36 (the system ignores bar-colour calls under
enforced edge-to-edge), and cannot be removed without patching Qt:

| API | File (`qtbase/src/android/jar/src/org/qtproject/qt/android/`) | Line |
|---|---|---|
| `Window.getStatusBarColor` | `QtActivityDelegateBase.java` | 108 |
| `Window.setStatusBarColor` | `QtDisplayManager.java` | 191, 200 |
| `Window.setNavigationBarColor` | `QtDisplayManager.java` | 192, 204 |

Expect the report to keep listing them until a Qt upgrade drops them. No action.

---

## 7. Large screens

targetSdk 36 also makes Android ignore orientation and resizability attributes on
large screens (sw ≥ 600 dp), so the app must tolerate arbitrary resize. Nothing in
Simsapa depends on a fixed orientation, and
`PROPERTY_COMPAT_ALLOW_RESTRICTED_RESIZABILITY` was deliberately **not** added —
it should only be considered if a concrete problem appears.

As of 2026-07-29 a Chromebook user has confirmed the app installs correctly,
which closes the original "not compatible on Chromebook" report. The
resize/rotate behaviour on a large screen has **not** been exercised.

---

## 8. How to test

Margins cannot be tested by `make qml-test` or `qmllint` — neither can see a
wrong gap. This needs a device.

1. **The top gap is ONE inset** — not zero, not doubled. Check this first; the
   whole design rests on it.
2. **Settings → Extra Top Margin.** At `0` the gap is a single inset. Raise it:
   space appears immediately, no restart. Restart: the value survives. Rotate:
   the "System safe area" readout updates.
3. **The drawer menu** in portrait *and* landscape — the `Popup`-family case from
   §4.
4. **Each secondary window separately.** Settings, Library, Dictionaries, Sutta
   Languages, Topic Index, Reference Search, Chanting Practice, About — each is
   its own `ApplicationWindow` with its own padding, so one being right proves
   nothing about the others.
5. **Landscape**: the cutout side and rounded corners; margins update without a
   restart, including while a dialog is open.
6. **Soft keyboard**: the focused input stays visible and the keyboard raises on
   the first tap (see [android-soft-keyboard.md](./android-soft-keyboard.md)).
7. **The sutta reader WebView**: scroll to top and bottom, use the find bar,
   switch display layouts — no HTML content under a system bar.
8. **Back navigation** from a dialog, a secondary window and the main window
   (§5).
9. **Upgrade path**: install over an existing install **with** a custom margin
   (layout unchanged — this is what exercises the `CustomValue(v) → v` migration)
   and over one **without** (the doubled gap must be gone).

Edge-to-edge enforcement does not apply below Android 16 at targetSdk 36, so a
**physical Android 16 device** is required — an Android 15 phone cannot validate
any of this. Build with `make android-apk` and **sideload**; a Qt Creator deploy
triggers the spurious "isn't 16 KB compatible" dialog and its kits are
single-ABI.
