import QtQuick

Item {
    function download_urls_and_extract(urls: list<string>, is_initial_setup: bool) {
        console.log("download_urls_and_extract():", is_initial_setup)
        for (let i=0; i < urls.length; i++) {
            console.log(i);
        }
    }

    function get_available_languages(): list<string> {
        console.log("get_available_languages()");
        return [];
    }

    function get_init_languages(): string {
        console.log("get_init_languages()");
        return "";
    }

    function should_auto_start_download(): bool {
        console.log("should_auto_start_download()");
        return false;
    }

    function peek_auto_start_download(): bool {
        console.log("peek_auto_start_download()");
        return false;
    }

    function set_keep_screen_on(on: bool) {
        console.log("set_keep_screen_on(" + on + ")");
    }

    function open_display_settings() {
        console.log("open_display_settings()");
    }

    function check_microphone_permission(): string {
        return "granted";
    }

    function request_microphone_permission() {
        console.log("request_microphone_permission()");
    }

    function remove_sutta_languages(language_codes: list<string>) {
        console.log("remove_sutta_languages():", language_codes);
    }

    signal downloadProgressChanged(op_msg: string, downloaded_bytes: int, total_bytes: int);
    signal downloadShowMsg(message: string);
    signal downloadsCompleted(message: string);
    signal downloadNeedsRetry(failed_url: string, error_message: string);
    signal removalShowMsg(message: string);
    signal removalProgressChanged(current_index: int, total_count: int, language_name: string);
    signal removalCompleted(success: bool, error_msg: string);
}
