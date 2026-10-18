import QtQuick

Item {
    id: root
    property bool open: false
    property string direction: "down"

    signal lyricsRequested()
    signal queueRequested()

    width: 48
    height: 48

    IconButton {
        anchors.fill: parent
        glyph: "\uf142"
        tint: root.open ? "#c4b0ff" : "white"
        onClicked: root.open = !root.open
    }

    Rectangle {
        id: dropdown
        visible: root.open
        z: 120
        width: 190
        height: 92
        radius: 12
        color: "#171320"
        border.width: 1
        border.color: "#332b45"
        x: (root.width - width) / 2
        y: root.direction === "up" ? -height - 10 : root.height + 10

        Column {
            anchors.fill: parent
            anchors.margins: 6
            spacing: 2

            MenuRow {
                glyph: "\uf075"
                label: "Текст песни"
                onTriggered: { root.lyricsRequested(); root.open = false }
            }
            MenuRow {
                glyph: "\uf142"
                label: "Очередь"
                onTriggered: { root.queueRequested(); root.open = false }
            }
        }
    }

    component MenuRow: Rectangle {
        id: mi
        property string glyph: ""
        property string label: ""
        signal triggered()

        width: parent.width
        height: 38
        radius: 8
        color: miMouse.containsMouse ? "#241d33" : "transparent"

        Row {
            anchors.fill: parent
            anchors.leftMargin: 10
            spacing: 10
            Icon { glyph: mi.glyph; tint: "#d8ccff"; font.pixelSize: 15; width: 18; anchors.verticalCenter: parent.verticalCenter }
            Text { text: mi.label; color: "#ece6f5"; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
        }
        MouseArea {
            id: miMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: mi.triggered()
        }
    }
}
