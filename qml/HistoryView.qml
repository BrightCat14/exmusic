import QtQuick

Item {
    id: root
    property var player
    property var tracks: []
    property bool requested: false

    signal openAlbum(string albumId)
    signal openArtist(string artistId)

    function refresh() {
        if (player && player.history_json.length > 0)
            tracks = JSON.parse(player.history_json)
    }

    function ensureLoaded() {
        if (!requested && player) {
            requested = true
            player.load_history()
        }
    }

    Component.onCompleted: refresh()
    onVisibleChanged: if (visible) ensureLoaded()

    Connections {
        target: root.player
        function onHistory_jsonChanged() { root.refresh() }
    }

    Rectangle { anchors.fill: parent; color: "#0d0c10" }

    Column {
        anchors.fill: parent
        anchors.margins: 28
        anchors.topMargin: 52
        spacing: 16

        Row {
            width: parent.width
            spacing: 16
            Text {
                id: titleText
                text: player.tr(player.language, "history.title")
                color: "#f5f2fb"
                font { pixelSize: 30; weight: Font.Black }
                anchors.verticalCenter: parent.verticalCenter
            }
            Item { width: Math.max(0, parent.width - titleText.width - 110 - 32); height: 1 }
            Rectangle {
                width: 110; height: 38; radius: 19
                anchors.verticalCenter: parent.verticalCenter
                color: refreshBtn.containsMouse ? "#302942" : "#241f30"
                Text { anchors.centerIn: parent; text: player.tr(player.language, "common.refresh"); color: "#e7ddff"; font.bold: true; font.pixelSize: 13 }
                MouseArea {
                    id: refreshBtn
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.player.load_history()
                }
            }
        }

        Text {
            text: root.player.loading && root.tracks.length === 0
                  ? player.tr(player.language, "common.loading")
                  : (root.tracks.length ? player.tr(player.language, "history.count").replace("%1", player.tracks(player.language, root.tracks.length)) : player.tr(player.language, "history.empty"))
            color: "#b3adc0"
            font.pixelSize: 13
        }

        ListView {
            id: list
            width: parent.width
            height: parent.height - y - 4
            clip: true
            spacing: 4
            model: root.tracks
            delegate: TrackRow {
                required property var modelData
                required property int index
                width: list.width
                track: modelData
                active: root.player.current_track_id === modelData.id
                onClicked: root.player.play_list(JSON.stringify(root.tracks), index)
                onArtistRequested: function(artistId) { root.openArtist(artistId) }
                onAlbumRequested: function(albumId) { root.openAlbum(albumId) }
            }
        }
    }
}
