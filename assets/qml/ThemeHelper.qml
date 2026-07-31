import QtQuick

import com.profoundlabs.simsapa

QtObject {
    id: theme_applier

    required property var target_window
    property bool is_dark: false

    property Logger logger: Logger { id: logger }

    function apply() {
        is_dark = SuttaBridge.get_theme_name() === "dark";

        // Rich-text <a href> links are coloured by QTextDocument from the
        // *application* palette, not from the window palette assigned below,
        // and that also overrides Text.linkColor. This is the only thing that
        // makes them follow the theme. Idempotent, so calling it from every
        // window is fine.
        SuttaBridge.apply_theme_link_colors();

        var theme_json = SuttaBridge.get_saved_theme();
        if (theme_json.length === 0 || theme_json === "{}") {
            logger.error("Couldn't get theme JSON.")
            return;
        }

        try {
            var d = JSON.parse(theme_json);

            for (var color_group_key in d) {
                if (!target_window.palette.hasOwnProperty(color_group_key) || target_window.palette[color_group_key] === undefined) {
                    logger.error("Member not found on target_window.palette:", color_group_key);
                    continue;
                }
                var color_group = d[color_group_key];
                for (var color_role_key in color_group) {
                    if (!target_window.palette[color_group_key].hasOwnProperty(color_role_key) || target_window.palette[color_group_key][color_role_key] === undefined) {
                        logger.error("Member not found on target_window.palette:", color_group_key, color_role_key);
                        continue;
                    }
                    try {
                        target_window.palette[color_group_key][color_role_key] = color_group[color_role_key];
                    } catch (e) {
                        logger.error("Could not set palette property:", color_group_key, color_role_key, e);
                    }
                }
            }
        } catch (e) {
            logger.error("Failed to parse theme JSON:", e);
        }
    }
}
