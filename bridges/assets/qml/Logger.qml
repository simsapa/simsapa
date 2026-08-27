import QtQuick
import com.profoundlabs.simsapa

QtObject {
    id: logger

    function debug(message) {
        SuttaBridge.log_debug(message)
    }

    function info(message) {
        SuttaBridge.log_info(message)
    }

    function warn(message) {
        SuttaBridge.log_warn(message)
    }

    function error(message) {
        SuttaBridge.log_error(message)
    }

    function get_level() {
        return SuttaBridge.get_log_level()
    }

    function set_level(level_str) {
        return SuttaBridge.set_log_level(level_str)
    }
}
