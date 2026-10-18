import QtQuick

Rectangle {
    id: root
    property string glyph: ""
    property bool active: true
    property int size: 48
    property color tint: "white"
    signal clicked()

    width: size
    height: size
    radius: size / 2
    color: area.containsMouse ? "#3dffffff" : "#18ffffff"
    border.width: 1
    border.color: area.containsMouse ? "#55ffffff" : "#18ffffff"
    opacity: active ? 1 : 0.4

    Icon {
        anchors.centerIn: parent
        glyph: root.glyph
        tint: root.tint
        font.pixelSize: root.size * 0.38
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }
}
