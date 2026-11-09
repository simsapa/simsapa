pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

// The DPD dictionary update offer, shown by the update check when a newer
// compatible DPD release exists. Three screens: the offer, the confirmation
// the user reads before anything starts, and the "Simsapa will now close"
// screen once the marker is written. The upgrade itself runs at the next start,
// in DpdUpgradeWindow, in place of the main app. See docs/dpd-upgrades.md.
ApplicationWindow {
    id: root
    title: root.view === "offer" ? "DPD Dictionary Update Available"
         : root.view === "confirm" ? "Update the DPD Dictionary"
         : "Restart Required"
    width: is_mobile ? Screen.desktopAvailableWidth : 560
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(680, Screen.desktopAvailableHeight)
    visible: false
    color: palette.window
    flags: Qt.Dialog
    modality: Qt.ApplicationModal

    Logger { id: logger }

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property int pointSize: is_mobile ? 14 : 12
    required property int extra_top_margin

    // "offer", "confirm" or "closing"
    property string view: "offer"

    // From the DpdUpdateOffer JSON
    property string installed_version: ""
    property string version: ""
    property string release_notes: ""
    property string download_size: ""
    property string space_needed: ""
    property string space_available: ""
    property bool enough_space: true
    property int minutes_min: 20
    property int minutes_max: 30
    property var outline: []
    property string benefit: ""

    property string marker_error: ""

    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    readonly property string minutes_text: root.minutes_min + "–" + root.minutes_max + " minutes"

    function show_offer(update_info_json: string) {
        try {
            const info = JSON.parse(update_info_json);
            root.installed_version = info.installed_version || "";
            root.version = info.version || "";
            root.release_notes = info.release_notes || "";
            root.download_size = info.download_size || "";
            root.space_needed = info.space_needed || "";
            root.space_available = info.space_available || "";
            root.enough_space = info.enough_space !== false;
            root.minutes_min = info.estimated_minutes_min || 20;
            root.minutes_max = info.estimated_minutes_max || 30;
            root.outline = info.outline || [];
            root.benefit = info.benefit || "";
        } catch (e) {
            logger.error("DpdUpdateDialog: cannot parse the offer: " + e + " json: " + update_info_json);
            return;
        }
        if (root.version.length === 0) {
            logger.error("DpdUpdateDialog: the offer names no version");
            return;
        }
        root.marker_error = "";
        root.view = "offer";
        theme_helper.apply();
        root.show();
        root.raise();
        root.requestActivate();
    }

    function start_update() {
        if (SuttaBridge.prepare_for_dpd_upgrade(root.version)) {
            root.marker_error = "";
            root.view = "closing";
        } else {
            root.marker_error = "The update could not be scheduled: Simsapa could not write to its storage location. "
                + "The log file has the details.";
        }
    }

    // The outline's first line is the download; it carries the size.
    function outline_line(index: int, text: string): string {
        if (index === 0 && root.download_size.length > 0) {
            return text + " (about " + root.download_size + ")";
        }
        return text;
    }

    StackLayout {
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin
        currentIndex: root.view === "offer" ? 0 : (root.view === "confirm" ? 1 : 2)

        // =====================================================================
        // The offer
        // =====================================================================
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                spacing: 0
                anchors.fill: parent

                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: parent.width
                        spacing: 10

                        Label {
                            text: "DPD Dictionary Update Available"
                            font.bold: true
                            font.pointSize: root.pointSize + 4
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "A newer release of the Digital Pāḷi Dictionary can be installed without a new Simsapa release."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "Installed version: " + root.installed_version
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "New version: " + root.version
                            font.pointSize: root.pointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: root.release_notes.length > 0
                            text: "Release Notes:"
                            font.pointSize: root.pointSize
                            font.bold: true
                            Layout.fillWidth: true
                        }

                        Rectangle {
                            visible: root.release_notes.length > 0
                            Layout.fillWidth: true
                            Layout.preferredHeight: 200
                            color: root.palette.base
                            border.color: root.palette.mid
                            border.width: 1
                            radius: 4

                            ScrollView {
                                anchors.fill: parent
                                anchors.margins: 5

                                TextArea {
                                    text: root.release_notes
                                    font.pointSize: root.pointSize - 1
                                    wrapMode: Text.WordWrap
                                    textFormat: Text.RichText
                                    selectByMouse: true
                                    readOnly: true
                                    background: null

                                    onLinkActivated: function(link) {
                                        Qt.openUrlExternally(link);
                                    }

                                    MouseArea {
                                        anchors.fill: parent
                                        acceptedButtons: Qt.NoButton
                                        cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                                    }
                                }
                            }
                        }

                        Label {
                            text: "Download: about " + root.download_size
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "Free space needed: about " + root.space_needed
                                + (root.space_available.length > 0 ? " (" + root.space_available + " available)" : "")
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: !root.enough_space
                            text: "There is not enough free space in Simsapa's storage location for this update. Free up some space and check for updates again."
                            font.pointSize: root.pointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "The update takes about " + root.minutes_text
                                + ". Simsapa closes first, and the dictionary is not available until the update finishes."
                                + (root.is_mobile ? " On a phone, keep it plugged in." : "")
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Later"
                        font.pointSize: root.pointSize
                        onClicked: root.close()
                    }

                    Button {
                        text: "Update"
                        font.pointSize: root.pointSize
                        enabled: root.enough_space
                        onClicked: root.view = "confirm"
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }

        // =====================================================================
        // The confirmation: the time first, then what happens and why.
        // Plain words only: no internal names.
        // =====================================================================
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                spacing: 0
                anchors.fill: parent

                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: parent.width
                        spacing: 10

                        Label {
                            text: "Update the DPD dictionary to " + root.version + "?"
                            font.bold: true
                            font.pointSize: root.pointSize + 3
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "This takes about " + root.minutes_text + "."
                            font.bold: true
                            font.pointSize: root.pointSize + 2
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "Simsapa will close. When you start it again, it runs the update and shows the progress. "
                                + "The dictionary is not available until the update finishes."
                                + (root.is_mobile ? " Keep the phone plugged in." : "")
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "What happens:"
                            font.bold: true
                            font.pointSize: root.pointSize
                            Layout.fillWidth: true
                            Layout.topMargin: 5
                        }

                        Repeater {
                            model: root.outline
                            delegate: Label {
                                required property int index
                                required property string modelData
                                text: "•  " + root.outline_line(index, modelData)
                                font.pointSize: root.pointSize
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                                Layout.leftMargin: 10
                            }
                        }

                        Label {
                            text: "What you get:"
                            font.bold: true
                            font.pointSize: root.pointSize
                            Layout.fillWidth: true
                            Layout.topMargin: 5
                        }

                        Label {
                            text: root.benefit
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "Needs about " + root.space_needed + " of free space. "
                                + "If anything goes wrong, your current dictionary stays as it is."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.topMargin: 5
                        }

                        Label {
                            visible: root.marker_error.length > 0
                            text: root.marker_error
                            font.pointSize: root.pointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Not now"
                        font.pointSize: root.pointSize
                        onClicked: root.close()
                    }

                    Button {
                        text: "Start update"
                        font.pointSize: root.pointSize
                        onClicked: root.start_update()
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }

        // =====================================================================
        // The marker is written: quit, the next start runs the update.
        // =====================================================================
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                spacing: 0
                anchors.fill: parent

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 10

                        Label {
                            text: "Simsapa will now close."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: "Start it again to begin the DPD dictionary update."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Quit"
                        font.pointSize: root.pointSize
                        onClicked: Qt.quit()
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }
    }
}
