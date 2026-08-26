pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    title: "Re-indexing Dictionaries"
    width: is_mobile ? Screen.desktopAvailableWidth : 520
    height: is_mobile ? Screen.desktopAvailableHeight : 220
    visible: true
    color: palette.window
    flags: Qt.Dialog
    modality: Qt.ApplicationModal

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property int pointSize: is_mobile ? 16 : 12

    // The window starts reconciliation on Component.onCompleted via the
    // DictionaryManager bridge, drives the progress bar from
    // `reconcileProgress`, and closes itself on `reconcileFinished`. C++ shows
    // the window before the main `SuttaSearchWindow` and waits for it to close.
    property string stage_text: "Re-indexing imported dictionaries — please wait."
    property real progress_value: 0.0
    property bool indeterminate: true

    DictionaryManager {
        id: dict_mgr

        onReconcileProgress: function(stage, done, total) {
            root.stage_text = stage;
            if (total > 0) {
                root.progress_value = done / total;
                root.indeterminate = false;
            } else {
                root.indeterminate = true;
            }
        }

        onReconcileFinished: {
            root.close();
        }
    }

    Component.onCompleted: {
        dict_mgr.start_reconcile();
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        Item { Layout.fillHeight: true }

        Label {
            id: stage_label
            text: root.stage_text
            font.pointSize: root.pointSize
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }

        ProgressBar {
            id: progress_bar
            from: 0
            to: 1
            value: root.progress_value
            indeterminate: root.indeterminate
            Layout.fillWidth: true
        }

        Item { Layout.fillHeight: true }
    }
}
