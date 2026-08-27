import QtQuick

QtObject {
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

    function find_storage_candidates_json(): string {
        console.log("find_storage_candidates_json()");
        return "[]";
    }

    function probe_storage_candidate(path: string, request_id: string) {
        console.log("probe_storage_candidate(): " + path + ", request_id: " + request_id);
    }

    function cancel_storage_probes() {
        console.log("cancel_storage_probes()");
    }

    signal probeCompleted(path: string, request_id: string, result_json: string);
}
