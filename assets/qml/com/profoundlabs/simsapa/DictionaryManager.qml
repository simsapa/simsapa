import QtQuick

Item {
    function import_zip(zip_path: string, label: string, lang: string): string {
        console.log("import_zip():", zip_path, label, lang);
        return "ok";
    }

    function import_dir(dir_path: string, label: string, lang: string): string {
        console.log("import_dir():", dir_path, label, lang);
        return "ok";
    }

    function scan_source(kind: string, path: string): string {
        console.log("scan_source():", kind, path);
        return "ok";
    }

    function stage_picked_file(url: url): string {
        console.log("stage_picked_file():", url);
        return "ok";
    }

    function abort_staging() {
        console.log("abort_staging()");
    }

    function cleanup_staged_file(path: string): bool {
        console.log("cleanup_staged_file():", path);
        return true;
    }

    function abort_import() {
        console.log("abort_import()");
    }

    function delete_dictionary(dictionary_id: int): string {
        console.log("delete_dictionary():", dictionary_id);
        return "ok";
    }

    function rename_label(dictionary_id: int, new_label: string): string {
        console.log("rename_label():", dictionary_id, new_label);
        return "ok";
    }

    function list_dictionaries(): string {
        return "[]";
    }

    function list_dictionaries_without_dpd_and_bold(): string {
        return "[]";
    }

    function list_user_dictionaries(): string {
        return "[]";
    }

    function list_shipped_source_uids(): string {
        return "[]";
    }

    function dpd_source_uids(): string {
        return "[\"dpd\"]";
    }

    function commentary_definitions_source_uids(): string {
        return "[]";
    }

    function label_status(label: string): string {
        return "available";
    }

    function check_label_status(label: string) {
        console.log("check_label_status():", label);
    }

    function suggested_label_for_zip(zip_path: string): string {
        return "";
    }

    function is_known_tokenizer_lang(lang: string): bool {
        return true;
    }

    function get_dict_enabled(label: string): bool {
        return true;
    }

    function set_dict_enabled(label: string, enabled: bool) {
        console.log("set_user_dict_enabled():", label, enabled);
    }

    function get_dict_enabled_map(): string {
        return "{}";
    }

    function get_dpd_enabled(): bool {
        return true;
    }

    function set_dpd_enabled(enabled: bool) {
        console.log("set_dpd_enabled():", enabled);
    }

    function get_commentary_definitions_enabled(): bool {
        return true;
    }

    function set_commentary_definitions_enabled(enabled: bool) {
        console.log("set_commentary_definitions_enabled():", enabled);
    }

    function reconcile_needed(): bool {
        return false;
    }

    function start_reconcile() {
        console.log("start_reconcile()");
    }

    signal importProgress(stage: string, done: int, total: int);
    signal importFinished(dictionary_id: int, label: string, inserted_count: int, elapsed_ms: int);
    signal importFailed(message: string);
    signal importCancelled(message: string, inserted_count: int);
    signal stagingProgress(done_bytes: real, total_bytes: real);
    signal stagingFinished(path: string);
    signal stagingFailed(message: string);
    signal scanFinished(items_json: string);
    signal scanFailed(message: string);
    signal deleteFinished(dictionary_id: int, label: string, removed_count: int, elapsed_ms: int);
    signal deleteFailed(message: string);
    signal renameFinished(dictionary_id: int, old_label: string, new_label: string, elapsed_ms: int);
    signal renameFailed(message: string);
    signal labelStatusChecked(label: string, status: string);
    signal reconcileProgress(stage: string, done: int, total: int);
    signal reconcileFinished();
}
