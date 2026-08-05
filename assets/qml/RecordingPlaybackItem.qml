pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

Item {
    id: root

    Logger { id: logger }

    // PlayerState integer values exposed by the Rust AudioManager
    // (see backend/src/audio/player.rs `PlayerState::as_i32`). The Rust QObject
    // has no Q_ENUM, so the named values live here rather than on AudioManager.
    readonly property int player_stopped: 0
    readonly property int player_playing: 1
    readonly property int player_paused: 2

    // Properties (6.1)
    property string recording_uid: ""
    property string file_path: ""
    property string label: ""
    property string recording_type: "user"  // "reference" or "user"
    property bool is_new_recording: false
    property real volume: 1.0
    property int playback_position_ms: 0

    // Signals (6.7, 6.8)
    signal closed()
    signal recording_completed(string recorded_file_path)
    signal remove_requested(string recording_uid)
    signal playback_started()
    signal label_edited(string new_label)

    function pause_playback() {
        if (audio.state === root.player_playing) {
            audio.pause();
        }
    }

    // Markers data (JSON array of marker objects)
    property string markers_json: "[]"
    property var markers: []

    // Cached waveform data from database (set by parent)
    property string waveform_json: ""

    // Internal state
    property bool is_recording: false
    property int recording_elapsed_ms: 0
    property bool file_not_found: false
    property string error_message: ""
    property var waveform_data: []
    property int waveform_num_bars: 0  // total number of bars stored in cached waveform
    property bool waveform_loading: false

    // Range creation state: "idle", "waiting_start", "waiting_end"
    property string range_create_state: "idle"
    property int range_create_start_ms: -1

    // Range playback state (8.8, 8.9) — the Rust player owns the range/loop
    // boundary + looping; these track which marker is active for the UI.
    property bool loop_enabled: false
    property string active_range_id: ""  // ID of range marker currently being played
    property int active_range_start_ms: 0
    property int active_range_end_ms: 0
    // ID of the position marker that started the current playback, so only that
    // marker's button shows "pause" (not every marker during normal playback).
    property string active_position_marker_id: ""

    implicitHeight: main_column.implicitHeight + 16

    // The Rust audio backend instance — replaces Qt's MediaPlayer + MediaRecorder.
    // The cpal output stream is created lazily on load()/play() and torn down on
    // destruction (Rust Drop). See docs/pure-rust-audio-backend.md.
    AudioManager {
        id: audio
    }

    Connections {
        target: audio

        function onStateChanged() {
            if (audio.state === root.player_playing) {
                root.playback_started();
            } else if (audio.state === root.player_stopped) {
                // Playback stopped (natural end or stop) — clear active markers.
                root.active_position_marker_id = "";
                if (root.active_range_id !== "") {
                    root.active_range_id = "";
                    root.active_range_start_ms = 0;
                    root.active_range_end_ms = 0;
                }
            }
        }

        function onRecordingFinished(file_path: string) {
            root.is_recording = false;
            root.file_not_found = false;
            root.error_message = "";
            // Clear cached waveform so it regenerates from the new file.
            root.waveform_json = "";
            root.waveform_data = [];
            root.waveform_num_bars = 0;
            // Setting file_path triggers onFile_pathChanged → check_file / load.
            root.file_path = file_path;
            root.recording_completed(file_path);
        }

        function onErrorOccurred(message: string) {
            root.is_recording = false;
            root.error_message = message;
            logger.error("AudioManager error: " + message);
        }
    }

    Rectangle {
        anchors.fill: parent
        color: root.file_not_found ? Qt.rgba(1, 0.9, 0.9, 1) : palette.base
        border.color: root.file_not_found ? "red" : palette.mid
        border.width: 1
        radius: 4
    }

    function check_file() {
        if (root.file_path !== "" && !root.is_new_recording) {
            let exists = SuttaBridge.check_file_exists(root.file_path);
            logger.info("RecordingPlaybackItem check_file: " + root.file_path + " exists: " + exists);
            root.file_not_found = !exists;
            if (!exists) {
                root.error_message = "Audio file not found:\n" + root.file_path;
            } else {
                root.error_message = "";
            }
        } else {
            root.file_not_found = false;
            root.error_message = "";
        }
    }

    // Path already handed to audio.load(); avoids decoding the same file twice
    // (onFile_pathChanged and Component.onCompleted both call load_audio).
    property string loaded_path: ""

    // Load the file into the Rust player and restore the saved position/volume.
    function load_audio() {
        if (root.file_path !== "" && !root.file_not_found && root.file_path !== root.loaded_path) {
            root.loaded_path = root.file_path;
            audio.load(root.file_path);
            audio.set_volume(root.volume);
            if (root.playback_position_ms > 0) {
                audio.seek(root.playback_position_ms);
            }
        }
    }

    onFile_pathChanged: {
        check_file();
        load_waveform();
        load_audio();
    }

    onVolumeChanged: {
        audio.set_volume(root.volume);
    }

    onMarkers_jsonChanged: {
        try {
            root.markers = JSON.parse(root.markers_json);
        } catch (e) {
            root.markers = [];
        }
    }

    Component.onCompleted: {
        // Delay check slightly to ensure all properties are bound
        Qt.callLater(check_file);
        Qt.callLater(load_waveform);
        Qt.callLater(load_audio);
        // Parse initial markers
        try {
            root.markers = JSON.parse(root.markers_json);
        } catch (e) {
            root.markers = [];
        }
    }

    // Parse waveform JSON, handling both new object format and legacy plain array
    function parse_waveform_json(json_str: string) {
        let parsed = JSON.parse(json_str);
        if (Array.isArray(parsed)) {
            // Legacy format: plain array of peaks, derive num_bars from length
            root.waveform_data = parsed;
            root.waveform_num_bars = parsed.length;
        } else if (parsed && typeof parsed === "object" && Array.isArray(parsed.peaks)) {
            root.waveform_data = parsed.peaks;
            root.waveform_num_bars = parsed.num_bars || parsed.peaks.length;
        } else {
            root.waveform_data = [];
            root.waveform_num_bars = 0;
        }
    }

    function load_waveform() {
        if (root.file_path === "" || root.file_not_found) {
            root.waveform_data = [];
            root.waveform_num_bars = 0;
            root.waveform_loading = false;
            return;
        }

        // Use cached data from database if available
        if (root.waveform_json !== "" && root.waveform_json !== "[]") {
            try {
                parse_waveform_json(root.waveform_json);
                root.waveform_loading = false;
                return;
            } catch (e) {
                // Fall through to generate
            }
        }

        // Generate in background
        root.waveform_loading = true;
        SuttaBridge.generate_waveform_data(root.recording_uid, root.file_path, 200);
    }

    // Handle async waveform data from background thread
    Connections {
        target: SuttaBridge
        function onWaveformDataReady(recording_uid: string, waveform_json: string) {
            if (recording_uid !== root.recording_uid) return;
            root.waveform_loading = false;
            root.waveform_json = waveform_json;
            try {
                root.parse_waveform_json(waveform_json);
            } catch (e) {
                root.waveform_data = [];
                root.waveform_num_bars = 0;
            }
        }
    }

    // Debounce timer for persisting volume changes
    Timer {
        id: volume_save_timer
        interval: 300
        repeat: false
        onTriggered: {
            if (root.recording_uid !== "") {
                SuttaBridge.update_recording_volume(root.recording_uid, root.volume);
            }
        }
    }

    // Debounce timer for persisting playback position
    Timer {
        id: position_save_timer
        interval: 300
        repeat: false
        onTriggered: {
            root.save_position();
        }
    }

    function save_position() {
        if (root.recording_uid !== "" && !root.is_new_recording) {
            SuttaBridge.update_recording_playback_position(root.recording_uid, audio.position_ms);
        }
    }

    // Stop playback/recording and persist volume + position
    function cleanup() {
        if (root.is_recording) {
            stop_recording();
        }
        if (audio.state === root.player_playing) {
            audio.stop();
        }
        save_position();
        if (root.recording_uid !== "") {
            SuttaBridge.update_recording_volume(root.recording_uid, root.volume);
        }
    }

    // Timer for recording elapsed time (6.6)
    Timer {
        id: recording_timer
        interval: 100
        repeat: true
        running: root.is_recording
        onTriggered: {
            root.recording_elapsed_ms += 100;
        }
    }

    // Helper to format ms to MM:SS
    function format_time(ms: int): string {
        let total_secs = Math.floor(ms / 1000);
        let mins = Math.floor(total_secs / 60);
        let secs = total_secs % 60;
        return String(mins).padStart(2, '0') + ":" + String(secs).padStart(2, '0');
    }

    ColumnLayout {
        id: main_column
        x: 10
        y: 8
        // Slightly wider side inset (10px) than the frame's border so the
        // scrubber/volume slider handles at the 0% and 100% extremes stay
        // fully within the item and are not clipped at the right edge.
        width: root.width - 20
        spacing: 6

        // Header row with label, duration, edit and close button (6.7)
        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Label {
                id: title_label
                text: root.label + (audio.duration_ms > 0 ? " (" + root.format_time(audio.duration_ms) + ")" : "")
                font.bold: true
                elide: Text.ElideRight
                Layout.fillWidth: true
                visible: !title_edit_field.visible
            }

            TextField {
                id: title_edit_field
                visible: false
                text: root.label
                font.bold: true
                Layout.fillWidth: true
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
                onAccepted: {
                    root.label = text;
                    root.label_edited(text);
                    visible = false;
                }
                Keys.onEscapePressed: {
                    text = root.label;
                    visible = false;
                }
                onActiveFocusChanged: {
                    if (!activeFocus && visible) {
                        text = root.label;
                        visible = false;
                    }
                }
            }

            Button {
                icon.source: "icons/32x32/fa_pen-to-square-solid.png"
                Layout.preferredWidth: 24
                flat: true
                visible: !title_edit_field.visible
                onClicked: {
                    title_edit_field.text = root.label;
                    title_edit_field.visible = true;
                    title_edit_field.forceActiveFocus();
                    title_edit_field.selectAll();
                }
                ToolTip.visible: hovered
                ToolTip.text: "Edit title"
            }

            Button {
                icon.source: "icons/32x32/mdi--close.png"
                Layout.preferredWidth: 24
                flat: true
                onClicked: {
                    root.save_position();
                    root.closed();
                }
            }
        }

        // File not found error display
        ColumnLayout {
            Layout.fillWidth: true
            visible: root.file_not_found
            spacing: 6

            Label {
                text: root.error_message
                color: "red"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                Button {
                    text: "Remove Recording"
                    onClicked: {
                        root.remove_requested(root.recording_uid);
                    }
                }

                Button {
                    text: "Close"
                    onClicked: root.closed()
                }
            }
        }

        // Recording state indicator
        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            visible: root.is_recording

            Rectangle {
                id: recording_dot
                Layout.preferredWidth: 12
                Layout.preferredHeight: 12
                radius: 6
                color: "red"

                SequentialAnimation on opacity {
                    running: root.is_recording
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.3; duration: 500 }
                    NumberAnimation { to: 1.0; duration: 500 }
                }
            }

            Label {
                text: "Recording  " + root.format_time(root.recording_elapsed_ms)
                color: "red"
                font.bold: true
            }
        }

        // Audio controls row (6.2) — hidden when file not found, disabled while
        // the player is still decoding. A Flow (not a RowLayout) so that on a
        // narrow phone the trailing time display wraps to a second line instead
        // of being pushed off the right edge.
        Flow {
            Layout.fillWidth: true
            spacing: 4
            visible: !root.file_not_found
            enabled: !audio.loading

            // Record button — only for new/user recordings (6.4)
            Button {
                id: record_button
                icon.source: root.is_recording ? "icons/32x32/fluent--record-stop-24-regular.png" : "icons/32x32/fluent--record-24-regular.png"
                icon.width: 16
                icon.height: 16
                text: root.is_recording ? "Stop" : "Record"
                visible: root.is_new_recording || root.recording_type === "user"
                enabled: audio.state !== root.player_playing
                onClicked: {
                    if (root.is_recording) {
                        root.stop_recording();
                    } else {
                        root.start_recording();
                    }
                }
            }

            // Play/Pause button
            Button {
                id: play_button
                icon.source: audio.state === root.player_playing ? "icons/32x32/fluent--pause-circle-24-regular.png" : "icons/32x32/fluent--play-circle-24-regular.png"
                icon.width: 20
                icon.height: 20
                enabled: !root.is_recording && root.file_path !== "" && !root.file_not_found
                implicitWidth: 40
                onClicked: {
                    if (audio.state === root.player_playing) {
                        audio.pause();
                        position_save_timer.restart();
                    } else {
                        // Main play is general playback, not tied to a marker.
                        root.active_position_marker_id = "";
                        audio.play();
                    }
                }
            }

            // Stop button
            Button {
                icon.source: "icons/32x32/fluent--record-stop-24-regular.png"
                icon.width: 20
                icon.height: 20
                enabled: !root.is_recording && audio.state !== root.player_stopped
                implicitWidth: 40
                onClicked: {
                    root.stop_range_playback();
                    audio.stop();
                    position_save_timer.restart();
                }
            }

            // Quick seek buttons
            Button {
                text: "-5s"
                enabled: !root.is_recording && root.file_path !== "" && !root.file_not_found
                implicitWidth: 40
                onClicked: {
                    root.active_position_marker_id = "";
                    let new_pos = Math.max(0, audio.position_ms - 5000);
                    audio.seek(new_pos);
                    position_save_timer.restart();
                }
            }

            Button {
                text: "+5s"
                enabled: !root.is_recording && root.file_path !== "" && !root.file_not_found
                implicitWidth: 40
                onClicked: {
                    root.active_position_marker_id = "";
                    let max_pos = audio.duration_ms > 0 ? audio.duration_ms : audio.position_ms;
                    let new_pos = Math.min(max_pos, audio.position_ms + 5000);
                    audio.seek(new_pos);
                    position_save_timer.restart();
                }
            }

            // Time display (6.3)
            Label {
                text: root.format_time(audio.position_ms) + " / " + root.format_time(audio.duration_ms)
                font.family: "monospace"
                verticalAlignment: Text.AlignVCenter
                height: play_button.height
                visible: !root.is_recording && root.file_path !== "" && !root.file_not_found
            }
        }

        // Loading / waveform placeholder. Shown while the Rust player decodes
        // the file (audio.loading) or while the waveform is being generated.
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 60
            visible: !root.is_recording && root.file_path !== "" && !root.file_not_found
                && (audio.loading || (root.waveform_loading && root.waveform_data.length === 0))
            color: palette.base
            border.color: palette.mid
            border.width: 1
            radius: 2

            Label {
                anchors.centerIn: parent
                text: audio.loading ? "Loading…" : "Loading waveform…"
                color: palette.placeholderText
                font.pointSize: 10
            }
        }

        // Waveform visualization — above the scrubber
        WaveformView {
            id: waveform_view
            Layout.fillWidth: true
            Layout.preferredHeight: 60
            visible: !root.is_recording && !audio.loading && root.file_path !== "" && !root.file_not_found && root.waveform_data.length > 0

            waveform_data: root.waveform_data
            duration_ms: audio.duration_ms
            playback_position_ms: audio.position_ms
            is_playing: audio.state === root.player_playing
            markers: root.markers
            range_create_active: root.range_create_state !== "idle"
            range_create_pending_ms: root.range_create_start_ms

            onSeek_requested: function(position_ms) {
                if (root.range_create_state === "waiting_start") {
                    root.range_create_start_ms = position_ms;
                    root.range_create_state = "waiting_end";
                } else if (root.range_create_state === "waiting_end") {
                    let start = Math.min(root.range_create_start_ms, position_ms);
                    let end = Math.max(root.range_create_start_ms, position_ms);
                    root.add_range_marker(start, end);
                    root.range_create_state = "idle";
                    root.range_create_start_ms = -1;
                } else {
                    root.stop_range_playback();
                    audio.seek(position_ms);
                    position_save_timer.restart();
                }
            }

            onRange_selected: function(start_ms, end_ms) {
                if (root.range_create_state !== "idle") {
                    // During range creation, ignore drag-based range selection
                    return;
                }
                root.add_range_marker(start_ms, end_ms);
            }
        }

        // Scrubber slider (6.3) — hidden when file not found
        Slider {
            id: scrubber
            Layout.fillWidth: true
            visible: !root.is_recording && root.file_path !== "" && !root.file_not_found
            enabled: !audio.loading
            from: 0
            to: audio.duration_ms > 0 ? audio.duration_ms : 1
            value: audio.position_ms

            onMoved: {
                root.stop_range_playback();
                audio.seek(Math.round(value));
                position_save_timer.restart();
            }
        }

        // Volume control — hidden when file not found
        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            visible: !root.is_recording && root.file_path !== "" && !root.file_not_found

            Label {
                text: "🔊"
                font.pointSize: 10
            }

            Slider {
                id: volume_slider
                Layout.fillWidth: true
                from: 0.0
                to: 1.0
                value: root.volume
                stepSize: 0.05

                onMoved: {
                    root.volume = value;
                    if (root.recording_uid !== "") {
                        volume_save_timer.restart();
                    }
                }
            }

            Label {
                text: Math.round(volume_slider.value * 100) + "%"
                font.family: "monospace"
                Layout.preferredWidth: 40
                horizontalAlignment: Text.AlignRight
            }
        }

        // Marker controls row (8.2, 8.3, 8.9) — a Flow so the trailing Loop /
        // Resample controls wrap to a second line on a narrow phone instead of
        // the Resample button being truncated at the right edge.
        Flow {
            Layout.fillWidth: true
            spacing: 6
            visible: !root.is_recording && root.file_path !== "" && !root.file_not_found
            enabled: !audio.loading

            Button {
                text: "＋ Position"
                enabled: audio.duration_ms > 0
                onClicked: root.add_position_marker()

                ToolTip.visible: hovered
                ToolTip.text: "Add a position marker at the current playback time"
            }

            Button {
                id: range_create_button
                text: root.range_create_state === "idle" ? "＋ Range"
                    : root.range_create_state === "waiting_start" ? "Set Start"
                    : "Set End"
                enabled: audio.duration_ms > 0
                highlighted: root.range_create_state !== "idle"
                onClicked: {
                    if (root.range_create_state === "idle") {
                        root.range_create_state = "waiting_start";
                        root.range_create_start_ms = -1;
                    } else {
                        // Cancel range creation on button click
                        root.range_create_state = "idle";
                        root.range_create_start_ms = -1;
                    }
                }

                ToolTip.visible: hovered
                ToolTip.text: root.range_create_state === "idle"
                    ? "Click to start creating a range by clicking on the waveform"
                    : root.range_create_state === "waiting_start"
                    ? "Click on the waveform to set the range start (or click here to cancel)"
                    : "Click on the waveform to set the range end (or click here to cancel)"
            }

            CheckBox {
                id: loop_checkbox
                text: "Loop"
                checked: root.loop_enabled
                onCheckedChanged: root.loop_enabled = checked

                ToolTip.visible: hovered
                ToolTip.text: "When checked, range playback repeats automatically"
            }

            Button {
                id: resample_button
                text: "Resample"
                enabled: root.waveform_data.length > 0 && !root.waveform_loading && audio.duration_ms > 0
                onClicked: {
                    // Calculate current samples per second from num_bars and duration
                    let duration_secs = audio.duration_ms / 1000.0;
                    let current_sps = duration_secs > 0
                        ? Math.round(root.waveform_num_bars / duration_secs)
                        : 10;
                    resample_dialog.current_samples_per_second = current_sps;
                    resample_spinbox.value = current_sps;
                    resample_dialog.open();
                }

                ToolTip.visible: hovered
                ToolTip.text: "Regenerate waveform with a different sample rate"
            }
        }

        // Marker list (8.6, 8.7, 8.8, 8.10, 8.11)
        ColumnLayout {
            id: marker_list_column
            Layout.fillWidth: true
            spacing: 2
            visible: !root.is_recording && root.file_path !== "" && !root.file_not_found && root.markers.length > 0
            enabled: !audio.loading

            // Sorted copy: position markers by position_ms, range markers by start_ms
            property var sorted_markers: {
                let arr = root.markers.slice();
                arr.sort(function(a, b) {
                    let a_time = a.type === "position" ? a.position_ms : a.start_ms;
                    let b_time = b.type === "position" ? b.position_ms : b.start_ms;
                    return a_time - b_time;
                });
                return arr;
            }

            Label {
                text: "Markers"
                font.bold: true
                font.pointSize: 10
            }

            Repeater {
                model: marker_list_column.sorted_markers.length

                Rectangle {
                    id: marker_row_bg

                    required property int index

                    Layout.fillWidth: true
                    implicitHeight: marker_row.implicitHeight + 8
                    color: index % 2 === 0 ? "transparent" : Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.06)
                    radius: 4

                    ColumnLayout {
                        id: marker_row

                        property var marker: marker_row_bg.index < marker_list_column.sorted_markers.length ? marker_list_column.sorted_markers[marker_row_bg.index] : null
                        property bool is_position: marker !== null && marker.type === "position"
                        property bool is_active_range: marker !== null && marker.id === root.active_range_id

                        anchors.fill: parent
                        anchors.margins: 4
                        spacing: 2

                    // Top row: play, indicator, label, time, delete
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 4

                        // Seek / Play / Pause button
                        Button {
                            property bool is_playing_this: marker_row.is_position
                                ? (marker_row.marker !== null && marker_row.marker.id === root.active_position_marker_id && audio.state === root.player_playing)
                                : marker_row.is_active_range && audio.state === root.player_playing

                            icon.source: is_playing_this ? "icons/32x32/fluent--pause-circle-24-regular.png" : "icons/32x32/fluent--play-circle-24-regular.png"
                            icon.width: 16
                            icon.height: 16
                            implicitWidth: 32
                            implicitHeight: 28
                            flat: true
                            onClicked: {
                                if (marker_row.marker === null) return;
                                if (marker_row.is_position) {
                                    if (is_playing_this) {
                                        audio.pause();
                                        position_save_timer.restart();
                                    } else {
                                        root.stop_range_playback();
                                        root.active_position_marker_id = marker_row.marker.id;
                                        audio.seek(marker_row.marker.position_ms);
                                        audio.play();
                                        position_save_timer.restart();
                                    }
                                } else {
                                    if (is_playing_this) {
                                        audio.pause();
                                    } else if (marker_row.is_active_range) {
                                        // Range is active but paused — resume
                                        audio.play();
                                    } else {
                                        root.play_range(marker_row.marker.id, marker_row.marker.start_ms, marker_row.marker.end_ms);
                                    }
                                }
                            }

                            ToolTip.visible: hovered
                            ToolTip.text: is_playing_this ? "Pause" : (marker_row.is_position ? "Play from this position" : "Play this range")
                        }

                        // Type indicator
                        Rectangle {
                            Layout.preferredWidth: 8
                            Layout.preferredHeight: 8
                            radius: marker_row.is_position ? 4 : 1
                            color: marker_row.is_position ? "red" : palette.highlight
                            Layout.alignment: Qt.AlignVCenter
                        }

                        // Mark / Range label
                        Label {
                            text: marker_row.marker !== null ? marker_row.marker.label : ""
                            Layout.preferredWidth: 80
                            Layout.alignment: Qt.AlignVCenter
                            color: palette.text
                        }

                        // Time display
                        Label {
                            text: {
                                if (marker_row.marker === null) return "";
                                if (marker_row.is_position) {
                                    return root.format_time(marker_row.marker.position_ms);
                                } else {
                                    return root.format_time(marker_row.marker.start_ms) + " – " + root.format_time(marker_row.marker.end_ms);
                                }
                            }
                            font.family: "monospace"
                            font.pointSize: 9
                            Layout.alignment: Qt.AlignVCenter
                        }

                        Item { Layout.fillWidth: true }

                        // Delete button
                        Button {
                            icon.source: "icons/32x32/ion--trash-outline.png"
                            implicitWidth: 28
                            implicitHeight: 28
                            flat: true
                            onClicked: {
                                if (marker_row.marker !== null) {
                                    root.delete_marker(marker_row.marker.id);
                                }
                            }

                            ToolTip.visible: hovered
                            ToolTip.text: "Delete this marker"
                        }
                    }

                    // Adjust buttons row
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: 12
                        spacing: 4
                        visible: marker_row.marker !== null

                        // Edit button — opens precise time input dialog
                        Button {
                            icon.source: "icons/32x32/fa_pen-to-square-solid.png"
                            implicitWidth: 28
                            implicitHeight: 24
                            flat: true
                            onClicked: {
                                if (marker_row.marker === null) return;
                                marker_time_dialog.marker_id = marker_row.marker.id;
                                marker_time_dialog.is_position = marker_row.is_position;
                                if (marker_row.is_position) {
                                    marker_time_dialog.set_time_fields(pos_min_spin, pos_sec_spin, pos_ms_spin, marker_row.marker.position_ms);
                                } else {
                                    marker_time_dialog.set_time_fields(range_start_min_spin, range_start_sec_spin, range_start_ms_spin, marker_row.marker.start_ms);
                                    marker_time_dialog.set_time_fields(range_end_min_spin, range_end_sec_spin, range_end_ms_spin, marker_row.marker.end_ms);
                                }
                                marker_comment_field.text = marker_row.marker.comment !== undefined ? marker_row.marker.comment : "";
                                marker_time_dialog.open();
                            }
                            ToolTip.visible: hovered
                            ToolTip.text: "Edit timing and comment"
                        }

                        // Position marker: single -1s / +1s pair
                        Button {
                            icon.source: "icons/32x32/fa_angle-left-solid.png"
                            text: "-1s"
                            implicitHeight: 24
                            flat: true
                            visible: marker_row.is_position
                            onClicked: {
                                if (marker_row.marker === null) return;
                                root.update_marker_time(marker_row.marker.id, "position_ms", Math.max(0, marker_row.marker.position_ms - 1000));
                            }
                        }
                        Button {
                            icon.source: "icons/32x32/fa_angle-right-solid.png"
                            text: "+1s"
                            implicitHeight: 24
                            flat: true
                            visible: marker_row.is_position
                            LayoutMirroring.enabled: true
                            onClicked: {
                                if (marker_row.marker === null) return;
                                let max_ms = audio.duration_ms > 0 ? audio.duration_ms : marker_row.marker.position_ms;
                                root.update_marker_time(marker_row.marker.id, "position_ms", Math.min(max_ms, marker_row.marker.position_ms + 1000));
                            }
                        }

                        // Range marker: start -1s / +1s, then end -1s / +1s
                        Label {
                            text: "Start:"
                            font.pointSize: 8
                            visible: !marker_row.is_position
                            Layout.alignment: Qt.AlignVCenter
                        }
                        Button {
                            icon.source: "icons/32x32/fa_angle-left-solid.png"
                            text: "-1s"
                            implicitHeight: 24
                            flat: true
                            visible: !marker_row.is_position
                            onClicked: {
                                if (marker_row.marker === null) return;
                                root.update_marker_time(marker_row.marker.id, "start_ms", Math.max(0, marker_row.marker.start_ms - 1000));
                            }
                        }
                        Button {
                            icon.source: "icons/32x32/fa_angle-right-solid.png"
                            text: "+1s"
                            implicitHeight: 24
                            flat: true
                            visible: !marker_row.is_position
                            LayoutMirroring.enabled: true
                            onClicked: {
                                if (marker_row.marker === null) return;
                                let max_ms = marker_row.marker.end_ms - 100;
                                root.update_marker_time(marker_row.marker.id, "start_ms", Math.min(max_ms, marker_row.marker.start_ms + 1000));
                            }
                        }

                        Label {
                            text: "End:"
                            font.pointSize: 8
                            visible: !marker_row.is_position
                            Layout.leftMargin: 8
                            Layout.alignment: Qt.AlignVCenter
                        }
                        Button {
                            icon.source: "icons/32x32/fa_angle-left-solid.png"
                            text: "-1s"
                            implicitHeight: 24
                            flat: true
                            visible: !marker_row.is_position
                            onClicked: {
                                if (marker_row.marker === null) return;
                                let min_ms = marker_row.marker.start_ms + 100;
                                root.update_marker_time(marker_row.marker.id, "end_ms", Math.max(min_ms, marker_row.marker.end_ms - 1000));
                            }
                        }
                        Button {
                            icon.source: "icons/32x32/fa_angle-right-solid.png"
                            text: "+1s"
                            implicitHeight: 24
                            flat: true
                            visible: !marker_row.is_position
                            LayoutMirroring.enabled: true
                            onClicked: {
                                if (marker_row.marker === null) return;
                                let max_ms = audio.duration_ms > 0 ? audio.duration_ms : marker_row.marker.end_ms;
                                root.update_marker_time(marker_row.marker.id, "end_ms", Math.min(max_ms, marker_row.marker.end_ms + 1000));
                            }
                        }

                        Item { Layout.fillWidth: true }
                    }

                    // Comment display row
                    Label {
                        Layout.fillWidth: true
                        Layout.leftMargin: 12
                        visible: marker_row.marker !== null
                            && marker_row.marker.comment !== undefined
                            && marker_row.marker.comment !== ""
                        text: marker_row.marker !== null && marker_row.marker.comment !== undefined ? marker_row.marker.comment : ""
                        wrapMode: Text.Wrap
                        font.pointSize: 9
                        color: palette.placeholderText
                    }
                    }
                }
            }
        }
    }

    // Resample dialog
    Dialog {
        id: resample_dialog
        title: "Resample Waveform"
        parent: Overlay.overlay
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        width: Math.min(400, parent ? parent.width - 40 : 400)

        property int current_samples_per_second: 0

        ColumnLayout {
            anchors.fill: parent
            spacing: 12

            Label {
                text: "Current: " + resample_dialog.current_samples_per_second + " samples/sec"
                    + " (" + root.waveform_num_bars + " total)"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            RowLayout {
                spacing: 8

                Label { text: "New rate:" }

                SpinBox {
                    id: resample_spinbox
                    from: 1
                    to: 500
                    stepSize: 1
                    value: 10
                    editable: true
                }

                Label { text: "samples/sec" }
            }

            Label {
                text: "Higher values show more detail but use more memory."
                font.pointSize: 9
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
        }

        onAccepted: {
            let sps = resample_spinbox.value;
            let duration_secs = audio.duration_ms / 1000.0;
            let num_bars = Math.max(10, Math.round(sps * duration_secs));
            root.waveform_loading = true;
            root.waveform_data = [];
            root.waveform_num_bars = 0;
            SuttaBridge.generate_waveform_data(root.recording_uid, root.file_path, num_bars);
        }
    }

    // Marker time edit dialog
    Dialog {
        id: marker_time_dialog
        title: marker_time_dialog.is_position ? "Edit Position" : "Edit Range"
        standardButtons: Dialog.Ok | Dialog.Cancel
        parent: Overlay.overlay
        anchors.centerIn: parent
        modal: true
        width: Math.min(420, parent ? parent.width - 40 : 420)

        property string marker_id: ""
        property bool is_position: true

        function set_time_fields(min_spin: SpinBox, sec_spin: SpinBox, ms_spin: SpinBox, total_ms: int) {
            let total_secs = Math.floor(total_ms / 1000);
            min_spin.value = Math.floor(total_secs / 60);
            sec_spin.value = total_secs % 60;
            ms_spin.value = total_ms % 1000;
        }

        function fields_to_ms(min_spin: SpinBox, sec_spin: SpinBox, ms_spin: SpinBox): int {
            return (min_spin.value * 60 + sec_spin.value) * 1000 + ms_spin.value;
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 12

            // Position marker fields
            RowLayout {
                Layout.fillWidth: true
                visible: marker_time_dialog.is_position
                spacing: 4

                Label { text: "Position:" }
                SpinBox { id: pos_min_spin; from: 0; to: 999; editable: true; Layout.fillWidth: true; Layout.preferredWidth: 80; Layout.minimumWidth: 55 }
                Label { text: "m" }
                SpinBox { id: pos_sec_spin; from: 0; to: 59; editable: true; Layout.fillWidth: true; Layout.preferredWidth: 80; Layout.minimumWidth: 55 }
                Label { text: "s" }
                SpinBox { id: pos_ms_spin; from: 0; to: 999; editable: true; Layout.fillWidth: true; Layout.preferredWidth: 90; Layout.minimumWidth: 55 }
                Label { text: "ms" }
            }

            // Range marker fields
            RowLayout {
                visible: !marker_time_dialog.is_position
                Layout.fillWidth: true
                spacing: 4

                Label { text: "Start:" }
                SpinBox { id: range_start_min_spin; from: 0; to: 999; editable: true; Layout.fillWidth: true; Layout.preferredWidth: 80; Layout.minimumWidth: 55 }
                Label { text: "m" }
                SpinBox { id: range_start_sec_spin; from: 0; to: 59; editable: true; Layout.fillWidth: true; Layout.preferredWidth: 80; Layout.minimumWidth: 55 }
                Label { text: "s" }
                SpinBox { id: range_start_ms_spin; from: 0; to: 999; editable: true; Layout.fillWidth: true; Layout.preferredWidth: 90; Layout.minimumWidth: 55 }
                Label { text: "ms" }
            }

            RowLayout {
                visible: !marker_time_dialog.is_position
                Layout.fillWidth: true
                spacing: 4

                Label { text: "End:  " }
                SpinBox { id: range_end_min_spin; from: 0; to: 999; editable: true; Layout.fillWidth: true; Layout.preferredWidth: 80; Layout.minimumWidth: 55 }
                Label { text: "m" }
                SpinBox { id: range_end_sec_spin; from: 0; to: 59; editable: true; Layout.fillWidth: true; Layout.preferredWidth: 80; Layout.minimumWidth: 55 }
                Label { text: "s" }
                SpinBox { id: range_end_ms_spin; from: 0; to: 999; editable: true; Layout.fillWidth: true; Layout.preferredWidth: 90; Layout.minimumWidth: 55 }
                Label { text: "ms" }
            }

            Label { text: "Comment:" }

            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: 80

                TextArea {
                    id: marker_comment_field
                    placeholderText: "Add a comment..."
                    wrapMode: TextEdit.Wrap
                }
            }
        }

        onAccepted: {
            if (marker_time_dialog.is_position) {
                let ms = marker_time_dialog.fields_to_ms(pos_min_spin, pos_sec_spin, pos_ms_spin);
                let max_ms = audio.duration_ms > 0 ? audio.duration_ms : ms;
                root.update_marker_time(marker_time_dialog.marker_id, "position_ms", Math.min(max_ms, Math.max(0, ms)));
            } else {
                let start = marker_time_dialog.fields_to_ms(range_start_min_spin, range_start_sec_spin, range_start_ms_spin);
                let end = marker_time_dialog.fields_to_ms(range_end_min_spin, range_end_sec_spin, range_end_ms_spin);
                // Ensure correct order
                let actual_start = Math.min(start, end);
                let actual_end = Math.max(start, end);
                let max_ms = audio.duration_ms > 0 ? audio.duration_ms : actual_end;
                root.update_marker_time(marker_time_dialog.marker_id, "start_ms", Math.max(0, actual_start));
                root.update_marker_time(marker_time_dialog.marker_id, "end_ms", Math.min(max_ms, actual_end));
            }
            root.update_marker_field(marker_time_dialog.marker_id, "comment", marker_comment_field.text);
        }
    }

    // --- Marker management functions (8.2, 8.3, 8.11) ---

    function save_markers() {
        root.markers_json = JSON.stringify(root.markers);
        if (root.recording_uid !== "") {
            SuttaBridge.update_recording_markers(root.recording_uid, root.markers_json);
        }
    }

    function add_position_marker() {
        let new_markers = root.markers.slice();
        new_markers.push({
            "id": "pos_" + Date.now(),
            "type": "position",
            "label": "Mark",
            "comment": "",
            "position_ms": audio.position_ms
        });
        root.markers = new_markers;
        save_markers();
    }

    function add_range_marker(start_ms: int, end_ms: int) {
        let new_markers = root.markers.slice();
        new_markers.push({
            "id": "range_" + Date.now(),
            "type": "range",
            "label": "Range",
            "comment": "",
            "start_ms": start_ms,
            "end_ms": end_ms
        });
        root.markers = new_markers;
        save_markers();
    }

    function delete_marker(marker_id: string) {
        let new_markers = [];
        for (let i = 0; i < root.markers.length; i++) {
            if (root.markers[i].id !== marker_id) {
                new_markers.push(root.markers[i]);
            }
        }
        root.markers = new_markers;
        // Stop range playback if the active range was deleted
        if (root.active_range_id === marker_id) {
            stop_range_playback();
        }
        save_markers();
    }

    function update_marker_label(marker_id: string, new_label: string) {
        let new_markers = root.markers.slice();
        for (let i = 0; i < new_markers.length; i++) {
            if (new_markers[i].id === marker_id) {
                new_markers[i] = Object.assign({}, new_markers[i], {"label": new_label});
                break;
            }
        }
        root.markers = new_markers;
        save_markers();
    }

    function update_marker_time(marker_id: string, field: string, value_ms: int) {
        let new_markers = root.markers.slice();
        let update = {};
        update[field] = value_ms;
        for (let i = 0; i < new_markers.length; i++) {
            if (new_markers[i].id === marker_id) {
                new_markers[i] = Object.assign({}, new_markers[i], update);
                break;
            }
        }
        root.markers = new_markers;
        save_markers();
    }

    function update_marker_field(marker_id: string, field: string, value: string) {
        let new_markers = root.markers.slice();
        let update = {};
        update[field] = value;
        for (let i = 0; i < new_markers.length; i++) {
            if (new_markers[i].id === marker_id) {
                new_markers[i] = Object.assign({}, new_markers[i], update);
                break;
            }
        }
        root.markers = new_markers;
        save_markers();
    }

    // Range playback (8.8) — the Rust player owns the boundary + loop logic.
    function play_range(marker_id: string, start_ms: int, end_ms: int) {
        root.active_position_marker_id = "";
        root.active_range_id = marker_id;
        root.active_range_start_ms = start_ms;
        root.active_range_end_ms = end_ms;
        audio.play_range(start_ms, end_ms, root.loop_enabled);
    }

    function stop_range_playback() {
        root.active_range_id = "";
        root.active_range_start_ms = 0;
        root.active_range_end_ms = 0;
        // Seeking/stopping away also ends any position-marker playback context.
        root.active_position_marker_id = "";
        audio.clear_range();
    }

    // Permission helper (instantiated once per RecordingPlaybackItem)
    AssetManager { id: permission_manager }

    // Tracks whether we're waiting for the async permission result
    property bool permission_requested: false

    // Runtime microphone permission check
    function check_microphone_permission(): bool {
        let status = permission_manager.check_microphone_permission();
        if (status === "granted") {
            return true;
        }
        if (status === "undetermined" && !root.permission_requested) {
            root.permission_requested = true;
            permission_manager.request_microphone_permission();
            root.error_message = "Microphone permission requested. Please grant it and tap Record again.";
            return false;
        }
        if (status === "undetermined" && root.permission_requested) {
            // Permission dialog was shown but result not yet received; ask user to try again.
            root.error_message = "Waiting for microphone permission. Please grant it and tap Record again.";
            return false;
        }
        // Permission denied
        if (Qt.platform.os === "osx") {
            root.error_message = "Microphone access denied. Enable it in System Settings > Privacy & Security > Microphone.";
        } else {
            root.error_message = "Microphone permission denied. Please enable it in Android Settings > Apps > Simsapa > Permissions.";
        }
        return false;
    }

    function start_recording() {
        // On macOS, skip the explicit Qt permission check. The OS triggers the
        // TCC dialog automatically when CoreAudio (cpal) accesses the hardware.
        // Qt's QMicrophonePermission API does not reliably reflect TCC state
        // for unsigned/ad-hoc signed apps. Failures arrive via errorOccurred.
        if (Qt.platform.os !== "osx" && !check_microphone_permission()) {
            return;
        }

        root.error_message = "";

        // Generate output file path. The Rust recorder captures from the default
        // input device (re-detected at start), so no Qt MediaDevices step.
        let recordings_dir = SuttaBridge.get_chanting_recordings_dir();
        let timestamp = Date.now();
        let output_file = recordings_dir + "/" + root.recording_uid + "_" + timestamp + ".flac";

        root.recording_elapsed_ms = 0;
        root.is_recording = true;
        audio.start_recording(output_file);
    }

    function stop_recording() {
        root.is_recording = false;
        // File finalization happens in Rust; the recordingFinished signal
        // delivers the final path (see the AudioManager Connections above).
        audio.stop_recording();
    }
}
