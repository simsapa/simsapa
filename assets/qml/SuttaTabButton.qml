import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

TabButton {
    id: control

    required property int index

    // NOTE: same attributes as returned from new_tab_data().
    required property string item_uid
    required property string table_name
    required property string sutta_title
    required property string sutta_ref
    required property bool pinned
    required property bool focus_on_new
    required property string id_key
    required property string web_item_key

    signal pinToggled(bool pinned)
    signal closeClicked()

    // Aliases to allow triggering from outside (e.g., keyboard shortcut)
    property alias close_btn: close_btn
    property alias pin_btn: pin_btn

    /* implicitWidth: Math.min(200, Math.max(150, implicitContentWidth + 30)) */

    function elide_long_uid(uid) {
        if (uid === undefined || uid === null) {
            return '';
        }
        let parts = uid.split('/');
        if (parts.length === 1) {
            return uid;
        }
        let first = parts[0].length > 20 ? parts[0].substring(0, 20) + '...' : parts[0];
        let rest = parts.slice(1).join('/');
        return `${first}/${rest}`;
    }

    contentItem: RowLayout {
        Button {
            id: pin_btn
            checkable: true
            checked: control.pinned
            icon.source: checked ? "icons/32x32/material-symbols-light--push-pin.png" : "icons/32x32/material-symbols-light--push-pin-outline.png"
            Layout.preferredWidth: 24
            flat: true
            onCheckedChanged: control.pinToggled(checked)
        }

        Label {
            text: {
                if (control.table_name && control.table_name === "dpd_headwords") {
                    // "25671/dpd" -> "cakka-1/dpd" (spaces replaced with hyphens)
                    let base_uid = control.sutta_title.replace(/ /g, "-") + "/dpd";
                    return control.elide_long_uid(base_uid);
                } else {
                    return control.elide_long_uid(control.item_uid);
                }
            }
            elide: Text.ElideRight
            /* Layout.fillWidth: true */
        }

        Button {
            id: close_btn
            icon.source: "icons/32x32/mdi--close.png"
            Layout.preferredWidth: 24
            flat: true
            onClicked: control.closeClicked()
        }
    }

    Logger { id: startup_trace_logger }

    Component.onCompleted: {
        startup_trace_logger.info("STARTUP-TRACE: SuttaTabButton onCompleted start, focus_on_new=" + control.focus_on_new);
        if (control.focus_on_new) {
            control.click();
        }
        startup_trace_logger.info("STARTUP-TRACE: SuttaTabButton onCompleted end");
    }
}
