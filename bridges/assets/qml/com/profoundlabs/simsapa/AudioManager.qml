import QtQuick

// qmllint type stub for the Rust `AudioManager` CXX-Qt bridge.
// See bridges/src/audio_manager.rs for the real implementation.
Item {
    // Player-state enum values (match PlayerState::as_i32 in the backend).
    readonly property int Stopped: 0
    readonly property int Playing: 1
    readonly property int Paused: 2

    // Properties (qproperty in the bridge).
    property int state: 0
    property int position_ms: 0
    property int duration_ms: 0
    property bool loading: false

    function start_recording(output_path: string) {
        console.log("start_recording(" + output_path + ")");
    }

    function stop_recording() {
        console.log("stop_recording()");
    }

    function load(path: string) {
        console.log("load(" + path + ")");
    }

    function play() {
        console.log("play()");
    }

    function pause() {
        console.log("pause()");
    }

    function stop() {
        console.log("stop()");
    }

    function seek(position_ms: int) {
        console.log("seek(" + position_ms + ")");
    }

    function set_volume(volume: real) {
        console.log("set_volume(" + volume + ")");
    }

    function play_range(start_ms: int, end_ms: int, looping: bool) {
        console.log("play_range(" + start_ms + ", " + end_ms + ", " + looping + ")");
    }

    function clear_range() {
        console.log("clear_range()");
    }

    signal recordingFinished(file_path: string);
    signal errorOccurred(message: string);
}
