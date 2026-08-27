pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
/* import QtQuick.Controls */

/* import data // for qml preview */

ColumnLayout {
    id: root

    required property string window_id
    required property bool is_dark
    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    // dict_words.uid in dictionaries.sqlite3: pāpuṇāti 1/dpd
    // not the dpd number id uid
    property alias word_uid: html_view.word_uid

    DictionaryHtmlView {
        id: html_view
        window_id: root.window_id
        is_dark: root.is_dark
        word_uid: root.word_uid
        Layout.fillWidth: true
        Layout.fillHeight: true
    }
}
