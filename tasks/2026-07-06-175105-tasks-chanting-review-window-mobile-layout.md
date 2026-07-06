# Tasks: Chanting Review Window — Mobile Layout & Waveform Fixes

Derived from `2026-07-06-175105-prd---chanting-review-window-mobile-layout.md`.

## Reference Screenshots (Android, from the user's bug report)

These are the original screenshots that motivated this work. Keep them open while
implementing — they show the exact defects to fix and to check against.

- **`/home/gambhiro/Downloads/LocalSend/Screenshot_20260706_172707.jpg`** — Top of
  the window. The "Recordings" heading is truncated to **"Rec"** because the three
  control buttons (New Recording / Add from File / Add Reference) overlap it in the
  same row. Target of Task 1.0 (§4.1, §4.2). Note: with the heading removed, all
  three full-label buttons already fit on one row at this device width.
- **`/home/gambhiro/Downloads/LocalSend/Screenshot_20260706_172723.jpg`** — The list
  scrolled down; the button row has scrolled completely out of view and is
  unreachable. This is the "buttons must stay in a fixed bottom bar" case (Task 1.0,
  §4.2).
- **`/home/gambhiro/Downloads/LocalSend/Screenshot_20260706_172930.jpg`** — An open
  playback item. Right-edge clipping: the time display shows `07:22 / 07:2…`, the
  volume readout shows `100` (the `%` cut), the scrubber/volume slider handles sit
  flush at the edge, and `Resample` is truncated. The item's **content overflows its
  own frame** to the screen edge. Targets of Task 2.0 (§4.5) and Task 3.0 (waveform
  end/cursor, §4.4).

## Relevant Files

- `assets/qml/ChantingPracticeReviewWindow.qml` — The review window. Currently the
  three control buttons and the "Recordings" heading live in a `RowLayout` *inside*
  the `recordings_scroll` `ScrollView` (≈ lines 388–441). This is where the heading
  is removed and the buttons are lifted into a fixed bottom bar. The `New Recording`
  `onClicked` logic (appends to `new_recordings_model`, ≈ lines 412–428) and the two
  `FileDialog`s (`user_file_dialog`, `reference_file_dialog`) must keep working from
  the new bar.
- `assets/qml/RecordingPlaybackItem.qml` — The inline playback UI. `main_column`
  (`x: 8`, `width: root.width - 16`) holds the controls row, waveform, scrubber,
  volume row, marker controls (incl. Resample), and marker list. The horizontal
  overflow of the controls row / slider handles / volume readout / Resample button
  is fixed here.
- `assets/qml/WaveformView.qml` — The waveform renderer. `clip: true` plus the
  playback cursor at `x: ms_to_x(playback_position_ms) - 1` and end position markers
  get clipped at the far right; fixed here.
- `assets/qml/DownloadAppdataWindow.qml` — Reference only (no change). Lines
  ≈ 379–458 show the `ColumnLayout` → fill-height `ScrollView` + fixed bottom
  `RowLayout` pattern, including the `Layout.bottomMargin: root.is_mobile ? 60 : 20`
  treatment to clear the OS nav bar.
- `assets/icons/32x32/fa_circle-plus-solid.png` — Existing icon used for all three
  control buttons.

### Notes

- No new QML component files are created, so no `bridges/build.rs` `qml_files`
  change is expected. No Rust/bridge changes.
- Per project rules: use the `Logger` module (no `console`), PascalCase components,
  snake_case ids/properties. QML changes are picked up by rebuilding the app.
- Build with `make build -B`. Per project convention, run the build only after all
  sub-tasks of a top-level task are done (skip `make qml-test` unless asked).
- `root.is_mobile` already exists on the review window; `RecordingPlaybackItem.qml`
  has no such property yet — use `Qt.platform.os` there if a mobile check is needed,
  matching how that file already inspects the platform (e.g. `start_recording`).

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, check it off by changing `- [ ]` to
`- [x]`. Update the file after completing each sub-task, not just each parent task.

## Tasks

### 1.0 Restructure the recordings area — heading removal + fixed bottom button bar

**Specs / context (PRD §4.1, §4.2, §4.3):**
- Screenshots: `Screenshot_20260706_172707.jpg` (heading truncated to "Rec" behind
  the buttons) and `Screenshot_20260706_172723.jpg` (buttons scrolled out of reach).
- Target: `ChantingPracticeReviewWindow.qml`, the `Frame` → outer `ColumnLayout`
  (≈ line 291) containing the header, Pāli editor, and `recordings_scroll`.
- Desired structure (mirror `DownloadAppdataWindow.qml`): the `recordings_scroll`
  `ScrollView` keeps `Layout.fillHeight: true` and the three buttons move to a
  **fixed** `RowLayout`/`Frame` that is a sibling *below* the scroll view in the
  outer `ColumnLayout`, so it never scrolls.
- The `RowLayout` that currently holds the `Label { text: "Recordings" }` + the 3
  buttons (≈ lines 400–441) is removed from inside the scroll content; the Reference
  and User Recordings groups + the new-recordings `Repeater` remain in the scroll.
- Buttons: same icon `icons/32x32/fa_circle-plus-solid.png` for all three; short
  text labels that always stay visible (never icon-only); allow shrinking / wrap to
  a second row if all three don't fit on a narrow phone. Applies on desktop + mobile.
- Mobile bottom margin to clear the OS nav bar (`root.is_mobile ? 60 : 20`, matching
  DownloadAppdataWindow).
- Behaviours to preserve: New Recording → append to `new_recordings_model`
  (existing block); Add from File → `user_file_dialog.open()`; Add Reference →
  `reference_file_dialog.open()`.

- [x] 1.1 In `ChantingPracticeReviewWindow.qml`, remove the `Label { text: "Recordings" }` heading and the `RowLayout` wrapper that grouped it with the three buttons, from inside `recordings_scroll`'s content `ColumnLayout`.
- [x] 1.2 Add a fixed bottom bar (e.g. a `Frame` or `RowLayout`) as a sibling below `recordings_scroll` in the outer `ColumnLayout`, with `Layout.fillWidth: true` and `Layout.bottomMargin: root.is_mobile ? 60 : 20` (plus small top/side margins) so it stays clear of the OS nav bar on mobile.
- [x] 1.3 Move the three buttons into the bottom bar, each with `icon.source: "icons/32x32/fa_circle-plus-solid.png"`, an appropriately sized icon, and a short text label (e.g. "New Recording"/"New", "From File", "Reference"). Keep the exact wording readable; do not drop the label.
- [x] 1.4 Make the buttons share/shrink to the available width (e.g. `Layout.fillWidth` with elide, or a `Flow`/`GridLayout` that wraps) so all three fit on a narrow phone without horizontal clipping or scrolling.
- [x] 1.5 Reconnect the button handlers to their existing logic: New Recording → the existing `new_recordings_model.append({...})` block; Add from File → `user_file_dialog.open()`; Add Reference → `reference_file_dialog.open()`.
- [x] 1.6 Confirm `recordings_scroll` still has `Layout.fillHeight: true` so it fills the space above the bar, and that the internal bottom `Item { Layout.fillHeight: true }` spacer is still appropriate (remove if it now causes excess blank space).
- [x] 1.7 Verify no leftover references to the removed heading/RowLayout ids; build with `make build -B` after 2.0 and 3.0 are also done (per the run-tests-once convention), or at minimum confirm the file has no obvious syntax breakage.

### 2.0 Fix right-edge clipping in `RecordingPlaybackItem.qml`

**Specs / context (PRD §4.5):**
- Screenshot: `Screenshot_20260706_172930.jpg` — controls-row time display
  (`07:22 / 07:2…`), scrubber handle at 100%, volume slider handle + `100%` readout
  (shows only `100`), and the `Resample` button are all cut off at the right edge;
  the item's content visibly bleeds past its own white frame to the screen edge.
- **Root cause (two parts):**
  1. The controls `RowLayout` (Record / ⏸ / ⏹ / -5s / +5s + time `Label`) and the
     marker-controls `RowLayout` (＋ Position / ＋ Range / Loop / Resample) hold too
     many **fixed-width** items for a narrow phone. When their combined implicit
     width exceeds the available width, the `Item { Layout.fillWidth: true }` spacer
     collapses to 0 and the trailing item (the time `Label`, the `Resample` button)
     is pushed off the right edge. So these rows must **wrap or shrink**, not just
     gain a margin.
  2. `main_column` is at `x: 8` with `width: root.width - 16`; `Slider` handles still
     overshoot the track ends by ~half a handle-width, so even correctly-sized rows
     clip the handle at 0%/100% without a small inner inset.
- Fix approach (developer judgement, prefer addressing the real overflow over hiding
  it): allow the two crowded rows to wrap/shrink so trailing items stay visible, and
  add enough inner horizontal inset (or reduce `main_column` width) to leave room for
  slider-handle overshoot.

- [ ] 2.1 Reproduce/trace the overflow: confirm which items in `main_column` extend past `root.width` (controls `RowLayout` time `Label` at ≈ line 526; `scrubber` `Slider` ≈ 595; volume `Slider` + `%` `Label` ≈ 622–643; marker-controls `RowLayout` with `Resample` ≈ 647–718).
- [ ] 2.2 Increase the inner horizontal inset so slider handles are fully visible — e.g. widen `main_column`'s side margins (adjust `x`/`width`) or add left/right padding so the `Slider` handle radius at the 0% and 100% ends stays within `root.width`.
- [ ] 2.3 Ensure the audio controls `RowLayout` (Record/Play/Stop/-5s/+5s + time) does not clip the trailing time `Label`: allow it to wrap or shrink (e.g. reduce spacing, let the spacer `Item { Layout.fillWidth: true }` absorb slack, or wrap on narrow widths) so `mm:ss / mm:ss` is fully shown.
- [ ] 2.4 Ensure the marker-controls `RowLayout` (＋ Position / ＋ Range / Loop / Resample) keeps the `Resample` button fully visible on narrow widths (wrap or shrink rather than truncate).
- [ ] 2.5 Verify the volume row's `%` `Label` (`Layout.preferredWidth: 40`, right-aligned) is not pushed off-screen once the inset is corrected.
- [ ] 2.6 Sanity-check that these changes don't regress desktop layout (wider width should just have extra slack).

### 3.0 Fix end-of-track waveform and playback-cursor visibility in `WaveformView.qml`

**Specs / context (PRD §4.4):**
- Screenshot: `Screenshot_20260706_172930.jpg` — the waveform fills to the right
  edge; as playback approaches `duration_ms` the cursor/end markers are clipped to a
  sliver at the edge (the user: "towards the end of the playback the end of the
  waveform marks and thus the playback position is no longer visible").
- `WaveformView` has `clip: true`. The playback cursor is
  `Rectangle { x: ms_to_x(playback_position_ms) - 1; width: 2 }`; at
  `playback_position_ms == duration_ms`, `ms_to_x` returns `root.width`, so the
  cursor spans `x = width-1 .. width+1` and is clipped to a 1px sliver at the edge.
  End position markers (`x: ms_to_x(position_ms) - 1`) have the same problem.
- Goal: the end of the waveform and the cursor/end-markers stay fully visible when
  playback reaches the end. Depends on / pairs with the width/inset fix in 2.0 so the
  whole `WaveformView` sits within the item.
- Fix approach (developer judgement): reserve a few px of horizontal padding for the
  bars area so the last bar and the cursor at `duration_ms` render inside the clipped
  region — e.g. map `ms_to_x` over an inner width (`width - cursor_pad`) offset by
  `cursor_pad/2`, or clamp the cursor/marker `x` so a full 2px line is always inside
  `[0, width-2]`. Keep bars, range backgrounds, position markers, and the drag
  preview consistent with whatever mapping is chosen so clicks still seek correctly.

- [ ] 3.1 Decide and implement the horizontal-padding/clamp strategy so the playback cursor at `playback_position_ms == duration_ms` is fully visible within the clipped `WaveformView` (not a 1px sliver).
- [ ] 3.2 Apply the same treatment to end position markers so a marker at/near `duration_ms` remains visible.
- [ ] 3.3 Keep `x_to_ms` / `ms_to_x` mutually consistent so waveform clicks/drags still seek to the correct time after the padding change (verify a click at the far right still seeks to ≈ end, and a click at the far left to ≈ 0).
- [ ] 3.4 Confirm the waveform bars `Row`, range-marker backgrounds, and drag-preview rectangle still align with the adjusted mapping (no visual gap/overhang at either edge).

### 4.0 Build verification and manual mobile/desktop check

- [ ] 4.1 Run `make build -B` and confirm a clean compile (QML changes are picked up by the rebuild).
- [ ] 4.2 Manual check on mobile (or narrow window), comparing against the reference screenshots: heading gone (cf. `…172707.jpg`); three control buttons always visible at the bottom with a long recordings list (cf. `…172723.jpg`); all three fit with labels; playing a recording to the end shows the cursor at the visible right end, and time display / both slider handles / volume `%` / Resample all fully visible (cf. `…172930.jpg`).
- [ ] 4.3 Manual check on desktop: layout unchanged in behaviour, bottom bar present, nothing regressed.
- [ ] 4.4 If any user-facing doc or `PROJECT_MAP.md` references the review window layout, update it; otherwise note that no doc change is needed.
