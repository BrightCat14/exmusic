import QtQuick

Item {
    id: root
    property var player

    signal expand()

    height: 74

    Rectangle { anchors.fill: parent; color: "#15111e" }
    Rectangle {
        anchors { top: parent.top; left: parent.left; right: parent.right }
        height: 1
        color: "#2a2338"
    }

    Rectangle {
        anchors { top: parent.top; left: parent.left }
        height: 2
        width: parent.width * (player && player.duration_ms > 0
                               ? Math.min(1, player.position_ms / player.duration_ms) : 0)
        color: "#9b7df5"
        visible: player && player.title.length > 0
    }

    Row {
        id: controls
        anchors { right: parent.right; rightMargin: 18; verticalCenter: parent.verticalCenter }
        spacing: 6

        IconButton { size: 36; y: 5; glyph: "\uf065"; onClicked: root.expand() }
        IconButton { size: 36; y: 5; glyph: "\uf074"; tint: player && player.shuffle ? "#c4b0ff" : "white"
                     onClicked: player.toggle_shuffle() }
        IconButton { size: 36; y: 5; glyph: "\uf048"; onClicked: player.previous() }
        IconButton { size: 46; glyph: player && player.playing ? "\uf04c" : "\uf04b"
                     tint: "#c4b0ff"; color: "#20192c"; border.width: 0
                     onClicked: player.toggle_play() }
        IconButton { size: 36; y: 5; glyph: "\uf051"; onClicked: player.next() }
        IconButton { size: 36; y: 5; glyph: "\uf01e"; tint: player && player.repeat ? "#c4b0ff" : "white"
                     onClicked: player.toggle_repeat() }
        IconButton { size: 36; y: 5; glyph: player && player.liked ? "\uf004" : "\uf08a"
                     tint: player && player.liked ? "#ff7da8" : "white"
                     onClicked: player.toggle_like() }
    }

    Item {
        id: infoArea
        anchors { left: parent.left; leftMargin: 18; right: controls.left; rightMargin: 16; verticalCenter: parent.verticalCenter }
        height: 46
        clip: true

        Row {
            anchors.fill: parent
            spacing: 12

            Rectangle {
                width: 46; height: 46; radius: 8
                anchors.verticalCenter: parent.verticalCenter
                color: "#2a2338"
                CoverArt {
                    id: barCoverArt
                    anchors.fill: parent
                    radius: 8
                    backgroundColor: "transparent"
                    source: player ? player.cover_url : ""
                }
                Icon { anchors.centerIn: parent; glyph: "\uf001"; tint: "#cfc4e6"; font.pixelSize: 16; visible: !barCoverArt.hasImage }
            }

            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, parent.width - 58)
                spacing: 2
                Text {
                    width: parent.width
                    elide: Text.ElideRight
                    text: player && player.title.length ? player.title : "Ничего не играет"
                    color: "#f4f0f7"
                    font { pixelSize: 14; bold: true }
                }
                Text {
                    width: parent.width
                    elide: Text.ElideRight
                    text: player ? player.artist : ""
                    color: "#a49bb5"
                    font.pixelSize: 12
                }
            }
        }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: root.expand()
        }
    }
}
