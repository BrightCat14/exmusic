import QtQuick

Item {
    id: root
    property var player
    property var tracks: []
    property bool requested: false

    signal openAlbum(string albumId)
    signal openArtist(string artistId)

    function refresh() {
        if (player && player.collection_json.length > 0)
            tracks = JSON.parse(player.collection_json)
    }

    function ensureLoaded() {
        if (!requested && player) {
            requested = true
            player.load_collection()
        }
    }

    Component.onCompleted: { refresh(); ensureLoaded() }
    onVisibleChanged: if (visible) ensureLoaded()

    Connections {
        target: root.player
        function onCollection_jsonChanged() { root.refresh() }
    }

    Rectangle { anchors.fill: parent; color: "#131019" }

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
                text: player.tr(player.language, "collection.title")
                color: "#f7f3fa"
                font { pixelSize: 30; weight: Font.Black }
                anchors.verticalCenter: parent.verticalCenter
            }
            Item { width: Math.max(0, parent.width - titleText.width - 130 - 110 - 48); height: 1 }
            Rectangle {
                width: 130; height: 38; radius: 19
                anchors.verticalCenter: parent.verticalCenter
                color: playAll.containsMouse ? "#a98cf7" : "#9b7df5"
                Text { anchors.centerIn: parent; text: player.tr(player.language, "common.playAll"); color: "#1a1326"; font.bold: true; font.pixelSize: 13 }
                MouseArea {
                    id: playAll
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: if (root.tracks.length) root.player.play_list(JSON.stringify(root.tracks), 0)
                }
            }
            Rectangle {
                width: 110; height: 38; radius: 19
                anchors.verticalCenter: parent.verticalCenter
                color: refreshBtn.containsMouse ? "#3b2e59" : "#2b233c"
                Text { anchors.centerIn: parent; text: player.tr(player.language, "common.refresh"); color: "#e8ddff"; font.bold: true; font.pixelSize: 13 }
                MouseArea {
                    id: refreshBtn
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.player.load_collection()
                }
            }
        }

        Text {
            text: root.player.loading && root.tracks.length === 0
                  ? player.tr(player.language, "common.loading")
                  : player.favorites(player.language, root.tracks.length)
            color: "#aaa4b5"
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
                number: index
                active: root.player.current_track_id === modelData.id
                onClicked: root.player.play_list(JSON.stringify(root.tracks), index)
                onArtistRequested: function(artistId) { root.openArtist(artistId) }
                onAlbumRequested: function(albumId) { root.openAlbum(albumId) }
            }
        }
    }
}
