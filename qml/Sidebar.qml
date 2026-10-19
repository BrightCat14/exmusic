import QtQuick

import app.backend

Rectangle {
    id: root
    property var player
    signal navigate(string target)
    signal openPlaylist(string uuid)
    signal settingsRequested()
    property string current: "search"
    property var playlists: []
    color: "#131019"

    readonly property var nav: [
        { icon: "\uf002", label: "Поиск", target: "search" },
        { icon: "\uf001", label: "Моя волна", target: "wave" },
        { icon: "\uf005", label: "Для вас и Тренды", target: "feed" },
        { icon: "\uf004", label: "Коллекция", target: "collection" },
        { icon: "\uf017", label: "История", target: "history" }
    ]

    function refreshPlaylists() {
        if (player && player.playlists_json.length > 0)
            playlists = JSON.parse(player.playlists_json)
    }

    Component.onCompleted: {
        refreshPlaylists()
        if (player) player.load_playlists()
    }

    Connections {
        target: root.player
        function onPlaylists_jsonChanged() { root.refreshPlaylists() }
    }

    Column {
        id: navColumn
        anchors { top: parent.top; left: parent.left; right: parent.right; margins: 16 }
        topPadding: 20
        spacing: 0

        Row {
            height: 58
            spacing: 12
            Icon { glyph: "\uf001"; tint: "#b69cff"; font.pixelSize: 28; width: 40; horizontalAlignment: Text.AlignHCenter; anchors.verticalCenter: parent.verticalCenter }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                Text { text: "EX"; color: "#b69cff"; font { pixelSize: 25; bold: true; letterSpacing: 2 } }
                Text { text: "music"; color: "#ffffff"; font { pixelSize: 11; weight: Font.Black } }
            }
        }
        Item { width: 1; height: 8 }

        Repeater {
            model: root.nav
            delegate: Item {
                width: root.width - 32
                height: 50
                Rectangle {
                    anchors.fill: parent
                    radius: 14
                    color: root.current === modelData.target ? "#2b233c" : (ma.containsMouse ? "#211b2d" : "transparent")
                    Rectangle { width: 3; height: 22; radius: 2; color: "#b69cff"; anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; visible: root.current === modelData.target }
                }
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.left: parent.left
                    spacing: 12
                    Icon { glyph: modelData.icon; tint: root.current === modelData.target ? "#d8ccff" : "#8e8899"; font.pixelSize: 18; width: 40; horizontalAlignment: Text.AlignHCenter }
                    Text {
                        text: modelData.label
                        color: root.current === modelData.target ? "#ffffff" : "#aaa4b5"
                        font { pixelSize: 14; bold: root.current === modelData.target }
                    }
                }
                MouseArea {
                    id: ma
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.navigate(modelData.target)
                }
            }
        }
    }

    Column {
        id: playlistsSection
        anchors {
            left: parent.left; right: parent.right
            top: navColumn.bottom; bottom: settings.top
            leftMargin: 16; rightMargin: 16; topMargin: 14; bottomMargin: 12
        }
        spacing: 10

        Text {
            visible: root.playlists.length > 0
            text: "Плейлисты"
            color: "#7d7689"
            font { pixelSize: 11; bold: true; letterSpacing: 1 }
        }

        ListView {
            width: parent.width
            height: Math.min(contentHeight, parent.height - 26)
            clip: true
            spacing: 10
            model: root.playlists
            delegate: Item {
                required property var modelData
                width: ListView.view.width
                height: 40
                Row {
                    anchors.fill: parent
                    spacing: 12
                    Rectangle {
                        width: 40; height: 40; radius: 6; clip: true; color: "#302842"
                        anchors.verticalCenter: parent.verticalCenter
                        CoverArt {
                            id: playlistArt
                            anchors.fill: parent
                            radius: 6
                            backgroundColor: "transparent"
                            source: modelData.coverUrl || ""
                        }
                        Text { anchors.centerIn: parent; text: playlistArt.hasImage ? "" : "♪"; color: "#cfc4e6"; font.pixelSize: 15 }
                    }
                    Column {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 52
                        Text {
                            text: modelData.title
                            color: plHover.containsMouse ? "#ffffff" : "#d6d0e0"
                            font { pixelSize: 13; bold: true }
                            width: parent.width
                            elide: Text.ElideRight
                        }
                        Text { text: modelData.trackCount + " треков"; color: "#9a9a9a"; font.pixelSize: 12 }
                    }
                }
                MouseArea {
                    id: plHover
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.openPlaylist(modelData.uuid)
                }
            }
        }
    }

    Row {
        id: settings
        anchors { left: parent.left; bottom: profile.top; leftMargin: 16; bottomMargin: 20 }
        spacing: 12
        Icon { glyph: "\uf013"; tint: "#aaa4b5"; font.pixelSize: 17; width: 40; horizontalAlignment: Text.AlignHCenter }
        Text { text: "Настройки"; color: "#aaa4b5"; font.pixelSize: 14 }
    }
    MouseArea { anchors.fill: settings; cursorShape: Qt.PointingHandCursor; onClicked: root.settingsRequested() }

    Row {
        id: profile
        anchors { left: parent.left; bottom: parent.bottom; leftMargin: 16; bottomMargin: 20 }
        spacing: 12
        Rectangle {
            width: 40; height: 40; radius: 20; color: "#302842"
            border { width: 2; color: "#9b7df5" }
            clip: true
            CoverArt {
                id: avatarArt
                anchors.fill: parent
                radius: 20
                backgroundColor: "transparent"
                source: player ? player.avatar_url : ""
            }
            Icon { anchors.centerIn: parent; glyph: "\uf007"; tint: "#e9e1ff"; font.pixelSize: 16; visible: !avatarArt.hasImage }
        }
        Column {
            y: (parent.height - height) / 2
            spacing: 2
            Text { text: player && player.account_name.length ? player.account_name : "Yandex Music"; color: "#f4f0f7"; font { pixelSize: 14; bold: true } }
        }
    }
}
