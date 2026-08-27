pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts

import com.profoundlabs.simsapa

StackLayout {
    id: root

    required property string window_id
    required property bool is_dark
    required property bool is_reading_mode
    property string current_key: ""
    property var items_map: ({})

    signal page_loaded()

    function show_transient_message(msg) {
        root.items_map[root.current_key].show_transient_message(msg);
    }

    Logger { id: logger }

    Component {
        id: sutta_html_component

        SuttaHtmlView {
            id: html_view
            window_id: root.window_id
            is_dark: root.is_dark
            is_reading_mode: root.is_reading_mode
            Layout.fillWidth: true
            Layout.fillHeight: true

            property int index
            /* Component.onCompleted: logger.info("SuttaHtmlView created at index", html_view.index) */
            /* Component.onDestruction: logger.info("SuttaHtmlView destroyed at index", html_view.index) */
        }
    }

    function add_item(tab_data: var, show_item = true) {
        logger.debug("STACK_LAYOUT: add_item() called - item_uid: " + tab_data.item_uid + " web_item_key: " + tab_data.web_item_key + " show_item: " + show_item);
        logger.debug("STACK_LAYOUT: Current items_map keys: " + Object.keys(root.items_map).join(", "));
        let key = tab_data.web_item_key;
        if (root.items_map.hasOwnProperty(key)) {
            logger.error("STACK_LAYOUT: ERROR - Item with key " + key + " already exists in items_map!");
            return;
        }

        let data = {
            item_uid: tab_data.item_uid,
            table_name: tab_data.table_name,
            sutta_ref: tab_data.sutta_ref,
            sutta_title: tab_data.sutta_title,
            anchor: tab_data.anchor || "",
        };
        let data_json = JSON.stringify(data);
        logger.debug("STACK_LAYOUT: Creating webview component for key: " + key);
        let comp = sutta_html_component.createObject(root, { item_key: key, data_json: data_json }) as SuttaHtmlView;

        comp.page_loaded.connect(function() { root.page_loaded(); });

        let is_current = Qt.binding(() => root.current_key === key);
        comp.should_be_visible = is_current;
        // Explicitly bind Loader visibility to prevent WebView flash on mobile startup
        // when parent container is invisible but WebView hasn't received visibility update yet
        comp.visible = Qt.binding(() => (root.current_key === key) && root.visible);
        // Width/height dimension bindings removed to prevent layout jitter during transitions.
        // Visibility control now relies solely on visible, should_be_visible, and enabled properties.
        root.items_map[key] = comp;
        logger.debug("STACK_LAYOUT: Added item to items_map. Total items: " + Object.keys(root.items_map).length);
        if (show_item) {
            logger.debug("STACK_LAYOUT: show_item=true, setting current_key to: " + key);
            root.current_key = key;
        }
        logger.debug("STACK_LAYOUT: add_item completed");
    }

    function get_item(key) {
        return root.items_map[key] || null;
    }

    function get_current_item(key) {
        return root.items_map[root.current_key];
    }

    function has_item(key) {
        return root.items_map.hasOwnProperty(key);
    }

    function delete_item(key, show_key_after_delete = null) {
        logger.debug("STACK_LAYOUT: delete_item() called for key: " + key);
        logger.debug("STACK_LAYOUT: Current items_map keys: " + Object.keys(root.items_map).join(", "));
        logger.debug("STACK_LAYOUT: Current current_key: " + root.current_key);
        if (!root.items_map.hasOwnProperty(key)) {
            logger.error("STACK_LAYOUT: ERROR - Item with key " + key + " not found in items_map!")
            return;
        }

        const item = root.items_map[key];
        delete root.items_map[key];
        logger.debug("STACK_LAYOUT: Deleted item from items_map. Remaining items: " + Object.keys(root.items_map).length);
        item.destroy();
        logger.debug("STACK_LAYOUT: Destroyed webview component");

        // Update current_key if needed
        if (root.current_key === key) {
            logger.debug("STACK_LAYOUT: Deleted item was current, updating current_key to: " + (show_key_after_delete || "(empty)"));
            root.current_key = show_key_after_delete || "";
        }
        logger.debug("STACK_LAYOUT: delete_item completed");
    }

    onCurrent_keyChanged: {
        logger.debug("STACK_LAYOUT: current_key changed to: " + root.current_key);
        logger.debug("STACK_LAYOUT: items_map has key: " + root.items_map.hasOwnProperty(root.current_key));
        update_currentIndex();
    }
    onCurrentIndexChanged: update_current_key()

    function update_currentIndex() {
        logger.debug("STACK_LAYOUT: update_currentIndex() called. current_key: " + root.current_key);
        if (!root.items_map[root.current_key] || root.current_key === "") {
            logger.debug("STACK_LAYOUT: current_key not in items_map or empty, setting currentIndex to -1");
            root.currentIndex = -1;
            return;
        }

        let item = root.items_map[root.current_key];
        logger.debug("STACK_LAYOUT: Found item in items_map");

        // Parse item data safely - data_json might not be set yet during initialization
        if (item.data_json && item.data_json.length > 0) {
            try {
                let item_data = JSON.parse(item.data_json);
                if (item_data.item_uid !== "Sutta" && item_data.item_uid !== "Word") {
                    SuttaBridge.emit_update_window_title(item_data.item_uid, item_data.sutta_ref, item_data.sutta_title);
                }
            } catch (e) {
                logger.error("STACK_LAYOUT: Failed to parse item.data_json in update_currentIndex: " + e);
            }
        }

        let found = false;
        for (let i = 0; i < root.children.length; i++) {
            if (root.children[i].item_key === root.current_key) {
                logger.debug("STACK_LAYOUT: Found matching child at index " + i + ", setting currentIndex");
                root.currentIndex = i;
                found = true;
                break;
            }
        }
        if (!found) {
            logger.debug("STACK_LAYOUT: No matching child found, setting currentIndex to -1");
            root.currentIndex = -1;
        }
    }

    function update_current_key() {
        if (root.currentIndex >= 0 && root.currentIndex < root.children.length) {
            const item = root.children[root.currentIndex];
            root.current_key = item.item_key || "";
        } else {
            root.current_key = "";
        }
    }

    Component.onDestruction: logger.info("SuttaStackLayout destroyed, children: ", root.children.length)
}
