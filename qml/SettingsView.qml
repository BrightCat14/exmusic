import QtQuick

Item {
    id: root
    property var player
    Rectangle { anchors.fill: parent; color: "#131019" }
    Column {
        anchors { left: parent.left; right: parent.right; top: parent.top }
        anchors.margins: 32
        spacing: 24
        Text { text: "Settings"; color: "#f7f3fa"; font { pixelSize: 32; weight: Font.Black } }
        Text { text: "Playback and appearance"; color: "#aaa4b5"; font.pixelSize: 14 }

        Rectangle {
            width: parent.width; height: 82; radius: 16; color: "#1d1728"
            Column {
                anchors { left: parent.left; verticalCenter: parent.verticalCenter; leftMargin: 20 }
                spacing: 5
                Text { text: "Visualizer"; color: "#f4f0f7"; font.bold: true }
                Text { text: "Show the reactive background on the player"; color: "#aaa4b5"; font.pixelSize: 12 }
            }
            Rectangle {
                anchors { right: parent.right; verticalCenter: parent.verticalCenter; rightMargin: 20 }
                width: 52; height: 30; radius: 15
                color: player.visualizer_enabled ? "#9b7df5" : "#493b68"
                Rectangle { width: 24; height: 24; radius: 12; anchors.verticalCenter: parent.verticalCenter; x: player.visualizer_enabled ? 25 : 3; color: "white"; Behavior on x { NumberAnimation { duration: 120 } } }
                MouseArea { anchors.fill: parent; onClicked: player.set_visualizer(!player.visualizer_enabled) }
            }
        }

        Rectangle {
            width: parent.width; height: 112; radius: 16; color: "#1d1728"
            Column { anchors.fill: parent; anchors.margins: 20; spacing: 12
                Row { width: parent.width
                    Text { text: "Volume"; color: "#f4f0f7"; font.bold: true }
                    Item { width: parent.width - 100; height: 1 }
                    Text { text: player.volume_percent + "%"; color: "#c4b0ff"; font.bold: true }
                }
                Rectangle {
                    id: volumeTrack
                    width: parent.width; height: 6; radius: 3; color: "#493b68"
                    Rectangle { width: parent.width * player.volume_percent / 100; height: parent.height; radius: 3; color: "#c4b0ff" }
                    MouseArea {
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
                        height: 28
                        onPressed: player.set_volume(Math.max(0, Math.min(100, mouse.x / width * 100)))
                        onPositionChanged: if (pressed) player.set_volume(Math.max(0, Math.min(100, mouse.x / width * 100)))
                    }
                }
            }
        }
    }
}
