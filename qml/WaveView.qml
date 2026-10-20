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

    Rectangle { anchors.fill: parent; color: "#0d0c10" }

    Column {
        anchors.fill: parent
        anchors.margins: 28
        spacing: 18

        Row {
            width: parent.width
            spacing: 18
            Text {
                text: player.tr(player.language, "wavePage.title")
                color: "#f5f2fb"
                font { pixelSize: 32; weight: Font.Black }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: player.tr(player.language, "wavePage.subtitle")
                color: "#b3adc0"
                font.pixelSize: 14
            }
            Item { width: parent.width - 400; height: 1 }
            Rectangle {
                width: 120; height: 38; radius: 19
                color: reload.containsMouse ? "#302942" : "#241f30"
                Text { anchors.centerIn: parent; text: player.tr(player.language, "common.refresh"); color: "#e7ddff"; font.bold: true }
                MouseArea {
                    id: reload; anchors.fill: parent; hoverEnabled: true
                    onClicked: root.player.load_wave()
                }
            }
        }

        Text {
            text: player.tr(player.language, "wavePage.description")
            color: "#b3adc0"
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
                color: hover.containsMouse ? "#241f30" : "#14121a"
                Row {
                    anchors.fill: parent
                    anchors.margins: 9
                    spacing: 14
                    Rectangle {
                        width: 46; height: 46; radius: 10; color: "#1b1923"
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
                        Text { text: modelData.title; color: "#f5f2fb"; font.bold: true; elide: Text.ElideRight; width: parent.width }
                        Text { text: modelData.artist + (modelData.album.length ? "  ·  " + modelData.album : ""); color: "#b3adc0"; font.pixelSize: 12; elide: Text.ElideRight; width: parent.width }
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
