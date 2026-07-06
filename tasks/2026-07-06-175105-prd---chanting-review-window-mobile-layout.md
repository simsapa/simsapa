# PRD: Chanting Review Window — Mobile Layout & Waveform Fixes

## 1. Introduction / Overview

The Chanting Practice Review window (`assets/qml/ChantingPracticeReviewWindow.qml`)
lets a user view a chant section's Pāli text and manage its recordings (reference
audio and their own practice recordings), with inline playback, a waveform, and
markers. Testing on Android surfaced three layout problems that make the window
awkward to use on a phone:

1. The **"Recordings"** heading is visually collided with / covered by the three
   control buttons in the same row (see screenshot 1).
2. The control buttons (**New Recording**, **Add from File**, **Add Reference**)
   live *inside* the scrollable recordings list, so once a section has many
   recordings they scroll out of view and become unreachable without scrolling
   back to the top (screenshot 2).
3. During playback, the **end of the waveform and the playback-position cursor
   become invisible** near the end of a recording, and more generally several
   controls in the playback item are **clipped at the right edge** on mobile —
   the time display (`07:22 / 07:2…`), the volume readout (`100`), the scrubber
   and volume slider handles, and the **Resample** button (screenshot 3).

This feature fixes all three, making the window usable and visually clean on
mobile while keeping desktop behaviour consistent.

## 2. Goals

1. Remove the redundant "Recordings" heading so nothing overlaps at the top of
   the recordings area.
2. Move the three recording-control buttons into a **fixed bottom bar** that is
   always visible regardless of how many recordings exist, following the pattern
   used in `DownloadAppdataWindow.qml`.
3. Ensure the three buttons fit cleanly on a narrow phone by giving them
   **icons + (shrinkable) labels**.
4. Eliminate all right-edge clipping in `RecordingPlaybackItem.qml` on mobile:
   the waveform end + playback cursor, the time display, the slider handles, the
   volume percentage, and the Resample button must all be fully visible.
5. Apply the layout changes consistently on **both desktop and mobile**.

## 3. User Stories

- **As a chanting practitioner on Android**, I want the recording-control buttons
  to stay visible at the bottom of the window, so that I can start a new recording
  or add a file no matter how far I have scrolled through my many recordings.
- **As a user with a long recording**, I want to see the playback cursor and the
  end of the waveform as the audio plays to the end, so that I can tell where I am
  in the recording at all times.
- **As a mobile user**, I want the playback controls (time, sliders, Resample) to
  be fully on-screen, so that I can read the elapsed/total time and drag the
  scrubber and volume all the way.
- **As any user**, I want a clean, uncluttered header for the recordings area that
  is not overlapped by buttons.

## 4. Functional Requirements

### 4.1 Remove the "Recordings" heading

1. The bold **"Recordings"** `Label` currently at the top of the recordings list
   (inside `recordings_scroll`, the `RowLayout` around lines 400–441 of
   `ChantingPracticeReviewWindow.qml`) MUST be removed.
2. The **"Reference"** and **"User Recordings"** group sub-headings MUST remain
   unchanged; they already communicate the section context.

### 4.2 Fixed bottom control bar

3. The three buttons **New Recording**, **Add from File**, and **Add Reference**
   MUST be moved out of the scrollable content (`recordings_scroll`) into a fixed
   bar anchored to the bottom of the window, so they do not scroll with the list.
4. The scrollable recordings list MUST fill the available vertical space above the
   fixed bar (the bar does not overlap the list).
5. The fixed bar MUST be present on **both desktop and mobile**.
6. On mobile, the bar MUST include an extra bottom margin so the OS bottom
   navigation bar does not cover it (mirror the
   `Layout.bottomMargin: root.is_mobile ? 60 : 20` treatment in
   `DownloadAppdataWindow.qml`).
7. Each of the three buttons MUST retain its existing behaviour:
   - **New Recording** → appends a new entry to `new_recordings_model` (current
     `onClicked` logic, lines ~412–428).
   - **Add from File** → opens `user_file_dialog`.
   - **Add Reference** → opens `reference_file_dialog`.

### 4.3 Button appearance (icons + labels)

8. Each of the three buttons MUST show an **icon plus a text label**.
9. The text labels MUST be **kept (never dropped to icon-only)**, even on the
   narrowest screens, because the function of each button is hard to guess from an
   icon alone. Labels MAY be shortened (e.g. "New", "From File", "Reference") and
   the buttons MAY share/shrink to the available width, but a readable text label
   MUST always accompany the icon. Wrapping the bar to a second row is acceptable
   if all three cannot fit on one row.
10. All three buttons MUST use the same icon,
    `icons/32x32/fa_circle-plus-solid.png` (a plain "add" glyph). The short text
    label is what distinguishes them (see req 9).

### 4.4 Waveform end & playback cursor visibility

11. When playback reaches the end of a recording (playback position at or near
    `duration_ms`), the **end of the waveform** and the **playback-position
    cursor** MUST remain fully visible within the playback item — they must not be
    clipped off the right edge.
12. This applies to the `WaveformView.qml` playback cursor and the position
    markers rendered at the far right of the waveform.

### 4.5 Right-edge clipping in the playback item

13. All content in `RecordingPlaybackItem.qml` MUST fit within the item's width on
    mobile, with nothing clipped at the right edge. Specifically:
    - The **time display** (`mm:ss / mm:ss`) in the controls row MUST be fully
      visible.
    - The **scrubber slider** handle MUST be reachable/visible at the 100%
      (far-right) position.
    - The **volume slider** handle and the **percentage readout** (`100%`) MUST be
      fully visible.
    - The **Resample** button MUST be fully visible (not truncated to
      `Resample…`).
14. The fix SHOULD address the underlying horizontal overflow (e.g. margins /
    layout width so the inner content width accounts for slider-handle overshoot
    and the waveform's clip region), rather than merely hiding symptoms.

## 5. Non-Goals (Out of Scope)

- No change to the **audio recording/playback engine** (the pure-Rust
  `AudioManager` backend) or to how recordings are stored.
- No change to **marker** creation, editing, or the marker list UI beyond keeping
  it within bounds (it already lays out in rows).
- No change to the **Pāli text editor**, the "Gloss Chanting Text" button, or the
  section header (collection / chant / section titles).
- No redesign of the recordings list item rows (Open/Close/Delete) beyond the
  heading removal.
- No new recording sources or file formats.

## 6. Design Considerations

- **Reference pattern:** `assets/qml/DownloadAppdataWindow.qml` (lines ~379–458)
  shows the intended structure — a `ColumnLayout` with a `ScrollView`
  (`Layout.fillHeight: true`) for scrollable content and a fixed `RowLayout` /
  button area below it, with a larger `bottomMargin` on mobile.
- The recordings area is currently one `ScrollView` (`recordings_scroll`)
  containing the header row, the Reference group, the User Recordings group, and
  the new-recordings repeater. The header-row buttons must be lifted out to the
  new fixed bar; the groups stay in the scroll area.
- Buttons follow existing icon-button conventions used elsewhere in
  `RecordingPlaybackItem.qml` (`icon.source`, `icon.width/height`).
- Mobile is detected with the existing `root.is_mobile` property.
- Follow project QML rules: PascalCase components, snake_case ids/properties, use
  the `Logger` module (no `console`), and register any **new** QML file in the
  `qml_files` list in `bridges/build.rs` (not expected to be needed here since no
  new component files are anticipated).

## 7. Technical Considerations

- Files expected to change:
  - `assets/qml/ChantingPracticeReviewWindow.qml` — remove the heading, restructure
    the recordings area into scroll-area + fixed bottom bar, restyle the three
    buttons with icons.
  - `assets/qml/RecordingPlaybackItem.qml` — fix horizontal overflow of the
    controls row, sliders, volume readout, and Resample button.
  - `assets/qml/WaveformView.qml` — ensure the end-of-waveform cursor/markers are
    not clipped at the right edge (e.g. reserve horizontal padding or adjust
    `ms_to_x` / `clip` handling so the cursor at `duration_ms` stays visible).
- The playback item's inner layout uses `main_column` at `x: 8` with
  `width: root.width - 16`; slider handles and the waveform's `clip: true` region
  can still overshoot. The likely fix is additional inner horizontal margin so the
  usable content width leaves room for the slider handle radius and the cursor
  width at the extreme right.
- No Rust/bridge changes are anticipated. If a new icon asset is required, add it
  under `assets/icons/` following the existing naming.

## 8. Success Metrics

- On a narrow Android phone, all three control buttons are visible at the bottom of
  the window at all times, regardless of the number of recordings, and none of
  their labels/icons are clipped.
- No "Recordings" heading overlaps any button.
- Playing a recording to its end shows the playback cursor moving to and remaining
  visible at the right end of the waveform.
- In an opened playback item on mobile, the time display, both slider handles, the
  volume percentage, and the Resample button are fully visible with no right-edge
  truncation.
- Desktop layout remains functional and visually consistent (manual verification).

## 9. Open Questions

None outstanding. Resolved decisions:

- **Icons:** all three buttons use the same `fa_circle-plus-solid.png`
  (`assets/icons/32x32/`); the text label distinguishes them. See §4.3 req 10.
- **Narrow screens:** always keep a short text label with each icon (never
  icon-only), since the function is hard to guess; shorten labels and/or wrap the
  bar to a second row rather than dropping the text.
