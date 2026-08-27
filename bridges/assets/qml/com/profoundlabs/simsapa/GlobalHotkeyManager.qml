import QtQuick

Item {
    function get_global_hotkeys_json(): string {
        return '{"enabled":false,"bindings":{"dictionary_lookup":"Ctrl+C+C"}}';
    }

    function get_default_global_hotkeys_json(): string {
        return '{"enabled":false,"bindings":{"dictionary_lookup":"Ctrl+C+C"}}';
    }

    function set_global_hotkey(action_id: string, sequence: string) {
    }

    function set_global_hotkeys_enabled(enabled: bool) {
    }

    function is_wayland(): bool {
        return false;
    }

    function get_api_url(): string {
        return "http://localhost:4848";
    }

    function is_macos(): bool {
        return false;
    }

    function is_macos_accessibility_trusted(): bool {
        return false;
    }

    function open_macos_accessibility_settings() {
    }

    signal globalHotkeysChanged();

    signal globalDictionaryLookupRequested(query: string);
}
