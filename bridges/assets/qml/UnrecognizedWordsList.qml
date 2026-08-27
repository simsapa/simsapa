pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root
    
    // Required properties
    required property var word_list
    required property string prefix_text
    required property string bg_color_darker
    required property string bg_color_lighter
    required property string text_color
    required property string border_color
    
    // Optional properties with defaults
    property int max_words: 20
    property string more_text: "and %1 more..."
    
    // Signals
    signal wordClicked(string word)
    
    // Calculate visible words and overflow
    readonly property var visible_words: (word_list && word_list.length > max_words) ? word_list.slice(0, max_words) : (word_list || [])
    readonly property int overflow_count: Math.max(0, (word_list ? word_list.length : 0) - max_words)
    readonly property bool has_overflow: overflow_count > 0
    
    // Only show when there are words
    visible: word_list && word_list.length > 0
    
    implicitHeight: visible ? column_layout.implicitHeight : 0
    
    ColumnLayout {
        id: column_layout
        anchors.fill: parent
        spacing: 5

        // Prefix text
        Text {
            Layout.fillWidth: true
            text: root.prefix_text
            color: root.text_color
            font.pointSize: 10
            wrapMode: Text.WordWrap
        }
        
        // Words flow layout
        Flow {
            Layout.fillWidth: true
            spacing: 5
            
            // Word buttons
            Repeater {
                model: root.visible_words
                delegate: Button {
                    id: word_btn
                    required property string modelData
                    
                    text: modelData
                    flat: true
                    
                    background: Rectangle {
                        color: word_btn.pressed ? Qt.darker(root.bg_color_darker, 1.3) : (word_btn.hovered ? root.bg_color_lighter : root.bg_color_darker)
                        radius: 4
                        border.width: 1
                        border.color: word_btn.pressed ? Qt.darker(root.border_color, 1.5) : root.border_color
                    }
                    
                    contentItem: Text {
                        text: word_btn.text
                        color: root.text_color
                        font.pointSize: 9
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                    }
                    
                    onClicked: root.wordClicked(modelData)
                }
            }
            
            // "and X more..." text if there's overflow
            Text {
                visible: root.has_overflow
                text: root.more_text.arg(root.overflow_count)
                color: root.text_color
                font.pointSize: 9
                font.italic: true
            }
        }
    }
}
