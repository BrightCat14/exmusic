import QtQuick

Item {
    id: root
    property var player
    property var tracks: []
    property bool loaded: false

    function refresh() {
        if (player && player.wave_results_json.length > 0)
            tracks = JSON.parse(player.wave_results_json)
    }

    Component.onCompleted: {
        refresh()
    }

    onVisibleChanged: if (visible && !loaded) {
        loaded = true
        player.load_wave()
    }

    Connections {
        target: root.player
        function onWave_results_jsonChanged() { root.refresh() }
    }

    Rectangle { anchors.fill: parent; color: "#131019" }

    Column {
        anchors.fill: parent
        anchors.margins: 28
        spacing: 18

        Row {
            width: parent.width
            spacing: 18
            Text {
                text: "My Wave"
                color: "#f7f3fa"
                font { pixelSize: 32; weight: Font.Black }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "Personal recommendations"
                color: "#aaa4b5"
                font.pixelSize: 14
            }
            Item { width: parent.width - 400; height: 1 }
            Rectangle {
                width: 120; height: 38; radius: 19
                color: reload.containsMouse ? "#3b2e59" : "#2b233c"
                Text { anchors.centerIn: parent; text: "Refresh"; color: "#e8ddff"; font.bold: true }
                MouseArea {
                    id: reload; anchors.fill: parent; hoverEnabled: true
                    onClicked: root.player.load_wave()
                }
            }
        }

        Text {
            text: "A stream built from your listening history"
            color: "#aaa4b5"
            font.pixelSize: 14
        }

        ListView {
            id: list
            width: parent.width
            height: parent.height - 115
            spacing: 7
            clip: true
            model: root.tracks
            delegate: Rectangle {
                required property var modelData
                width: list.width
                height: 64
                radius: 12
                color: hover.containsMouse ? "#2b233c" : "#1d1728"
                Row {
                    anchors.fill: parent
                    anchors.margins: 9
                    spacing: 14
                    Rectangle {
                        width: 46; height: 46; radius: 10; color: "#302842"
                        CoverArt {
                            id: waveArt
                            anchors.fill: parent
                            radius: 10
                            backgroundColor: "transparent"
                            source: modelData.coverSourceUrl || modelData.coverUrl || ""
                        }
                        Text { anchors.centerIn: parent; text: waveArt.hasImage ? "" : "♪"; color: "#e9e1ff"; font.pixelSize: 20 }
                    }
                    Column {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 75
                        Text { text: modelData.title; color: "#f4f0f7"; font.bold: true; elide: Text.ElideRight; width: parent.width }
                        Text { text: modelData.artist + (modelData.album.length ? "  ·  " + modelData.album : ""); color: "#aaa4b5"; font.pixelSize: 12; elide: Text.ElideRight; width: parent.width }
                    }
                }
                MouseArea {
                    id: hover; anchors.fill: parent; hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.player.play_track(modelData.id)
                }
            }
        }
    }
}
