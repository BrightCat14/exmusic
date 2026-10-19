import QtQuick

Item {
    id: root
    property var player
    property var results: []

    signal openAlbum(string albumId)
    signal openArtist(string artistId)

    function refreshResults() {
        if (player && player.search_results_json.length > 0)
            results = JSON.parse(player.search_results_json)
    }

    Component.onCompleted: refreshResults()

    Connections {
        target: root.player
        function onSearch_results_jsonChanged() { root.refreshResults() }
    }

    Rectangle {
        anchors.fill: parent
        color: "#131019"
    }

    Column {
        anchors.fill: parent
        anchors.margins: 28
        anchors.topMargin: 52
        spacing: 14

        Text {
            text: player.tr(player.language, "search.title")
            color: "#f7f3fa"
            font { pixelSize: 25; weight: Font.Black }
        }

        Rectangle {
            width: parent.width
            height: 42
            radius: 12
            color: "#211b2d"
            border.color: "#3b3248"
            TextInput {
                id: query
                anchors.fill: parent
                anchors.margins: 12
                color: "white"
                font.pixelSize: 15
                selectByMouse: true
                clip: true
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: player.tr(player.language, "search.placeholder")
                    color: "#8f879c"
                    visible: !parent.text.length
                }
                onAccepted: if (root.player) root.player.search(text)
            }
        }

        ListView {
            id: tracks
            width: parent.width
            height: parent.height - y - 4
            clip: true
            spacing: 6
            model: root.results
            delegate: TrackRow {
                required property var modelData
                width: tracks.width
                track: modelData
                active: root.player && root.player.current_track_id === modelData.id
                onClicked: if (root.player) root.player.play_list(JSON.stringify(root.results), index)
                onArtistRequested: function(artistId) { root.openArtist(artistId) }
                onAlbumRequested: function(albumId) { root.openAlbum(albumId) }
            }
        }
    }
}
