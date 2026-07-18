# WebEngineView stale black frame after window re-activation (Linux)

## The problem

On Linux desktop, after switching to another window and then back to the
Simsapa window, the HTML reader panels (`WebEngineView`) rendered as **solid
black**. The view came back only when something forced a redraw inside the
page — e.g. moving the mouse over one of the `menu.html` icons, whose CSS
`:hover` state triggers an in-page repaint. Manually resizing the app window
also restored the view.

Mechanism: while the app window is inactive/occluded, Chromium stops producing
compositor frames. On re-activation, the Qt scene graph can be left holding an
invalid/stale texture for the webview, which renders as black until Chromium
submits a fresh frame.

This is a long-standing QtWebEngine issue family:

- [QTBUG-54127](https://bugreports.qt.io/browse/QTBUG-54127) — hiding a web
  engine view and showing it again leaves the content area empty **until the
  window is resized**. This is the report that pointed at the working fix: a
  resize forces Chromium's render widget host to regenerate and submit a frame
  unconditionally.
- [QTBUG-51892](https://bugreports.qt.io/browse/QTBUG-51892) — QWebEngineView
  black after suspend; same stale-frame symptom from a different trigger.

## What did NOT work

The first attempt was a JavaScript repaint nudge on window re-activation:
`runJavaScript` toggling `document.documentElement.style.opacity` to `0.99`
and back on the next animation frame. The reasoning was that any in-page paint
(like the hover state) fixes the view. **It did not work** — presumably the
frame Chromium produced during the re-activation churn was still discarded, or
the composited result was identical enough not to propagate a new texture. Do
not reintroduce a JS-only nudge.

## The working fix: resize jiggle

`assets/qml/WebEngineRepaintNudge.qml` — a small helper instantiated next to
each desktop `WebEngineView`:

```qml
WebEngineRepaintNudge { web: web }
```

It watches the containing window's `active` property (via the `Window.window`
attached property). When the window becomes active again, a 50 ms `Timer`
sets `web.anchors.bottomMargin = 1`, and a second 50 ms `Timer` restores it to
`0`. The 1px resize forces Chromium to re-render and submit a fresh frame at
both sizes, replacing the black texture. The two-step deferral matters: the
jiggle must land *after* the window re-activation churn, and the margin change
only triggers a Chromium resize if the two sizes are actually distinct frames
(hence timers, not a same-tick set-and-restore).

Requirements/notes:

- The target `WebEngineView` must be anchored with `anchors.fill`, so the
  jiggle can be applied via `anchors.bottomMargin`.
- The helper logs `WebEngineRepaintNudge: window re-activated, resize-nudging
  webview` on each trigger, useful when diagnosing whether activation
  detection fired at all.
- Wired into both desktop webview components:
  `assets/qml/SuttaHtmlView_Desktop.qml` and
  `assets/qml/DictionaryHtmlView_Desktop.qml`. **Add it to any new desktop
  component that embeds a `WebEngineView`.** (Mobile variants use `WebView`,
  not QtWebEngine, and don't need it.)

## If it regresses / escalation ladder

Diagnose with the log line first:

- Log line present but view stays black → activation detection works but the
  resize nudge no longer suffices; heavier options are toggling the view's
  `visible`, or Chromium flags such as
  `QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu-compositing` (global perf cost).
- Log line absent → the window `active` property isn't changing as assumed
  (possible on some Wayland/X11 focus setups); the trigger would need to move
  to exposure/visibility events on the C++ side.
