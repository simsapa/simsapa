# QML `Dialog` sizing and headers

## `Dialog` width — clamp it to the window overlay

**Every `Dialog` must state its own width, clamped against the overlay it lives
in**, rather than letting it size itself from its content:

``` qml
Dialog {
    id: my_dialog

    modal: true
    // Clamped to the window overlay: a fixed 480 is wider than a phone screen.
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(parent.width - 40, 480)

    contentItem: Label {
        text: "…a sentence long enough to need wrapping…"
        wrapMode: Text.WordWrap
    }
}
```

A content-sized dialog with a long unwrapped string grows past the window edge
on a narrow desktop window and off-screen on a phone, where there is no window
manager to pull it back. The `Math.min(parent.width - 40, <max>)` form gives the
comfortable desktop width *and* the phone fit from one binding; `parent:
Overlay.overlay` is what makes `parent.width` the window's width regardless of
where the dialog is declared, and it is also what keeps `anchors.centerIn`
meaningful.

The clamp only works if the content wraps to it, so pair it with
`wrapMode: Text.WordWrap` on every text item inside — and that combination is
exactly what the next section is about.

`DictionaryEditDialog.qml` and `DictionariesWindow.qml`'s
`confirm_delete_all_dialog` are the worked examples. Native `MessageDialog`
(from `QtQuick.Dialogs`) is sized by the platform and needs none of this — but
it also cannot carry custom content, so a confirmation with more than one line
of text belongs in a `QtQuick.Controls` `Dialog` written this way.

## `Dialog` with a title and wrapping text — use `DialogHeader`

**Any `Dialog` that has a `title` *and* content that wraps
(`wrapMode: Text.WordWrap`) must set `header: DialogHeader { text: <dialog_id>.title }`
(`bridges/assets/qml/DialogHeader.qml`) instead of using Fusion's default header.**

Fusion's `Dialog` computes
`implicitHeight: … + (implicitHeaderHeight > 0 ? implicitHeaderHeight + spacing : 0) + …`
(`Fusion/Dialog.qml:17-20`) and its default header is a `Label` whose own
implicitHeight is resolved through that same layout pass. Combine that with
content whose height depends on its width, inside a component that is itself
re-laying out, and the dialog's `implicitHeight` oscillates:

```
QML Dialog: Binding loop detected for property "implicitHeight":
qrc:/qt-project.org/imports/QtQuick/Controls/Fusion/Dialog.qml:17:5
```

`DialogHeader` is visually identical to Fusion's (same padding, bold, elide,
same background) but **states its `implicitHeight` outright**, which is the
value the Label already has — no geometry changes, the feedback path just goes
away. Its root is an `Item` wrapping a `Label`, because **`implicitHeight` is
read-only on `Label`**: assigning it there is a load error, not an override, and
the component silently fails to load with `Type … unavailable`. Do not
"simplify" the wrapper away.

Three things must coincide to trigger it, which is why most dialogs are fine:
an unstated header height, width-dependent content height, **and** a declaring
component whose layout reflows in the same pass. A dialog declared directly in
an `ApplicationWindow` root is not exposed (a window's size is driven
externally); one declared inside a `Frame`, `Item` or layout — i.e. any
component embedded in a resizable parent — is. Setting `header:` costs one line
and removes the question, so prefer it whenever the title-plus-wrapping
combination occurs.

Related traps, measured — do **not** reach for these instead: the loop is
unaffected by the content's sizing (`parent.width` vs `availableWidth`, an
explicit `contentItem`, an explicit content height), by `standardButtons`,
`anchors.centerIn`, `parent: Overlay.overlay`, or by the dialog's width clamp.
And **never** "fix" it by removing `width: parent.width` from a dialog's
contentItem — that binding is what makes `wrapMode` work (see
`docs/android-edge-to-edge-and-safe-areas.md`) and is measured to be irrelevant
here.

**To reproduce or verify one, use the existing rig — do not build your own.**
`scripts/tst_dialog_loop_harness.qml.keep` instantiates the real
`SearchBarInput` in a window and drives the four scenarios that isolated the
cause; you count `Binding loop detected for property "implicitHeight"` lines
(baseline 3, with `DialogHeader` 0). Copy it into `bridges/assets/qml/` as a `tst_*.qml`
to resolve project types and **delete it again after use** — `make qml-test`
walks that tree. Its header comment carries the run command and the three things
that invalidated six earlier attempts: `QT_QUICK_CONTROLS_STYLE=Fusion` is
mandatory (the app forces Fusion in `cpp/gui.cpp`, `qmltestrunner` defaults to
Basic, whose `Dialog` cannot produce the loop at all);
`QT_QPA_PLATFORM=offscreen` is mandatory (the trigger is a window **resize**, and
this desktop's WM auto-maximizes windows on X11, silently ignoring every
`width = …`); and a variant that fails to *load* also logs zero loops, so check
`Totals: N passed, 0 failed` rather than the grep count alone. Because
`qmltestrunner` reads QML from disk, `git stash push <file>` → run →
`git stash pop` A/Bs a suspected cause in seconds with no rebuild.

Twelve titled-and-wrapping dialogs declared inside embedded components are a
known watch-list, deliberately left unfixed against zero observed defects (all 40
were enumerated and classified); if one ever logs this loop, the remedy is the
one `header:` line above, and the rig is how you confirm it.

