import QtQuick

Item {
    function get_app_data_storage_paths_json(): string {
        console.log("get_app_data_storage_paths_json()");
        return "[{}]";
    }

    function save_storage_path(path: string, is_internal: bool): bool {
        console.log("save_storage_path(): " + path + ", is_internal: " + is_internal);
        return true;
    }

    function storage_path_state(): string {
        console.log("storage_path_state()");
        return "absent";
    }

    function recorded_storage_path(): string {
        console.log("recorded_storage_path()");
        return "";
    }
}
