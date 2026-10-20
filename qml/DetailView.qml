import QtQuick

Item {
    id: root
    property var player
    property string kind: "album"
    property var info: ({})

    signal backRequested()
    signal openAlbum(string albumId)
    signal openArtist(string artistId)

    function sourceJson() {
        if (!player) return ""
        if (kind === "artist") return player.artist_json
        if (kind === "playlist") return player.playlist_json
        return player.album_json
    }

    function refresh() {
        var raw = sourceJson()
        info = raw && raw.length ? JSON.parse(raw) : ({})
    }

    function title() {
        if (kind === "artist") return info.name || ""
        return info.title || ""
    }

    function subtitle() {
        if (kind === "album") {
            var year = info.year ? " · " + info.year : ""
            return (info.artist || "") + year
        }
        if (kind === "playlist") {
            return (info.owner || "") + (info.trackCount ? " · " + player.tracks(player.language, info.trackCount) : "")
        }
        return info.description || ""
    }

    onKindChanged: refresh()

    Connections {
        target: root.player
        function onAlbum_jsonChanged() { if (root.kind === "album") root.refresh() }
        function onArtist_jsonChanged() { if (root.kind === "artist") root.refresh() }
        function onPlaylist_jsonChanged() { if (root.kind === "playlist") root.refresh() }
    }

    Rectangle { anchors.fill: parent; color: "#0d0c10" }

    Column {
        anchors.fill: parent
        anchors.margins: 28
        anchors.topMargin: 52
        spacing: 16

        Row {
            spacing: 12
            Rectangle {
                width: 90; height: 32; radius: 16
                anchors.verticalCenter: parent.verticalCenter
                color: backBtn.containsMouse ? "#302942" : "#241f30"
                Text { anchors.centerIn: parent; text: player.tr(player.language, "common.back"); color: "#e7ddff"; font.pixelSize: 13 }
                MouseArea {
                    id: backBtn
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.backRequested()
                }
            }
        }

        Row {
            width: parent.width
            spacing: 20

            Rectangle {
                width: 150; height: 150; radius: 16; clip: true
                color: "#1b1923"
                CoverArt {
                    id: coverArt
                    anchors.fill: parent
                    radius: 16
                    backgroundColor: "transparent"
                    source: root.info.coverSourceUrl || root.info.coverUrl || ""
                }
                Text {
                    anchors.centerIn: parent
                    text: coverArt.hasImage ? "" : "♪"
                    color: "#e9e1ff"
                    font { pixelSize: 34; bold: true }
                }
            }

            Column {
                width: parent.width - 180
                spacing: 8
                Text {
                    text: {
                        if (kind === "album") return player.tr(player.language, "detail.album")
                        if (kind === "artist") return player.tr(player.language, "detail.artist")
                        return player.tr(player.language, "detail.playlist")
                    }
                    color: "#c4b0ff"
                    font { pixelSize: 13; bold: true; letterSpacing: 1 }
                }
                Text {
                    text: root.title()
                    color: "#f5f2fb"
                    font { pixelSize: 26; weight: Font.Black }
                    elide: Text.ElideRight
                    width: parent.width
                }
                Text {
                    text: root.subtitle()
                    color: "#b3adc0"
                    font.pixelSize: 13
                    elide: Text.ElideRight
                    width: parent.width
                }
                Item { width: 1; height: 6 }
                Rectangle {
                    width: 150; height: 40; radius: 20
                    color: playAll.containsMouse ? "#d6c8ff" : "#c4b0ff"
                    Text { anchors.centerIn: parent; text: player.tr(player.language, "detail.playAll"); color: "#16101f"; font.bold: true; font.pixelSize: 13 }
                    MouseArea {
                        id: playAll
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: if (root.info.tracks && root.info.tracks.length)
                                       root.player.play_list(JSON.stringify(root.info.tracks), 0)
                    }
                }
            }
        }

        Text {
            visible: !root.info.tracks || root.info.tracks.length === 0
            text: root.player.loading ? player.tr(player.language, "common.loading") : player.tr(player.language, "detail.noTracks")
            color: "#b3adc0"
            font.pixelSize: 13
        }

        ListView {
            id: list
            width: parent.width
            height: parent.height - y - 4
            clip: true
            spacing: 4
            model: root.info.tracks ? root.info.tracks : []
            delegate: TrackRow {
                required property var modelData
                required property int index
                width: list.width
                track: modelData
                number: index
                active: root.player.current_track_id === modelData.id
                onClicked: root.player.play_list(JSON.stringify(root.info.tracks), index)
                onArtistRequested: function(artistId) { root.openArtist(artistId) }
                onAlbumRequested: function(albumId) { root.openAlbum(albumId) }
            }
        }
    }
}
