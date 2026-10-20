import QtQuick

Item {
    id: root
    property var player
    property bool lyricsOpen: false
    property bool queueOpen: false
    property var queueItems: []
    property real backgroundBlur: 48

    signal collapse()

    function refreshQueue() {
        if (player && player.queue_json.length > 0)
            queueItems = JSON.parse(player.queue_json)
    }

    function formatTime(value) {
        var seconds = Math.max(0, Math.floor(value / 1000))
        var minutes = Math.floor(seconds / 60)
        seconds = seconds % 60
        return minutes + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    Component.onCompleted: refreshQueue()

    Connections {
        target: root.player
        function onQueue_jsonChanged() { root.refreshQueue() }
    }

    Rectangle { anchors.fill: parent; color: "#08080c" }

    Image {
        id: backgroundImage
        anchors.fill: parent
        source: player ? player.cover_url : ""
        sourceSize: Qt.size(48, 48)
        fillMode: Image.PreserveAspectCrop
        smooth: true
        asynchronous: true
        opacity: 0.55
        layer.enabled: root.backgroundBlur > 0
        layer.effect: ShaderEffect {
            property vector2d texelSize: Qt.vector2d(1 / Math.max(1, backgroundImage.width),
                                                     1 / Math.max(1, backgroundImage.height))
            property real radius: root.backgroundBlur
            fragmentShader: "GaussianBlur.frag.qsb"
        }
    }
    Rectangle {
        anchors.fill: parent
        opacity: 0.45
        gradient: Gradient {
            orientation: Gradient.Vertical
            GradientStop {
                position: 0.0
                color: root.player ? root.player.mood_top_color : "#ECDEFF"
                Behavior on color { ColorAnimation { duration: 900; easing.type: Easing.InOutQuad } }
            }
            GradientStop {
                position: 0.52
                color: root.player ? root.player.mood_mid_color : "#D8C2FF"
                Behavior on color { ColorAnimation { duration: 900; easing.type: Easing.InOutQuad } }
            }
            GradientStop {
                position: 1.0
                color: root.player ? root.player.mood_bottom_color : "#C3A4FF"
                Behavior on color { ColorAnimation { duration: 900; easing.type: Easing.InOutQuad } }
            }
        }
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Vertical
            GradientStop { position: 0.0; color: "#6608080c" }
            GradientStop { position: 1.0; color: "#f208080c" }
        }
    }

    Column {
        anchors.centerIn: parent
        spacing: 14

        Rectangle {
            id: bigCover
            width: 240; height: 240; radius: 22
            anchors.horizontalCenter: parent.horizontalCenter
            gradient: Gradient {
                GradientStop { position: 0; color: "#d8c2ff" }
                GradientStop { position: 0.52; color: "#6d5a9e" }
                GradientStop { position: 1; color: "#110f17" }
            }
            CoverArt {
                id: bigCoverArt
                anchors.fill: parent
                radius: 22
                backgroundColor: "transparent"
                source: player ? player.cover_url : ""
            }
            Text { anchors.centerIn: parent; text: bigCoverArt.hasImage ? "" : "EX"; color: "#f5f2fb"; font { pixelSize: 40; weight: Font.Black; letterSpacing: 4 } }
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: player ? player.title : ""
            color: "#f5f2fb"
            font { pixelSize: 30; weight: Font.Black }
            elide: Text.ElideRight
            width: 560
            horizontalAlignment: Text.AlignHCenter
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: player ? player.artist : ""
            color: "#b8b2c4"
            font.pixelSize: 16
        }

        Item {
            id: scrub
            width: 560; height: 26
            anchors.horizontalCenter: parent.horizontalCenter
            property bool dragging: false
            property real dragMs: 0

            function progress() {
                if (!player || player.duration_ms <= 0) return 0
                var ms = scrub.dragging ? scrub.dragMs : player.position_ms
                return Math.max(0, Math.min(1, ms / player.duration_ms))
            }

            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width; height: 8; radius: 4
                color: "#2ec4b0ff"
                Rectangle {
                    width: parent.width * scrub.progress()
                    height: parent.height; radius: 4
                    color: "#c4b0ff"
                }
            }
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onPressed: function(mouse) {
                    scrub.dragging = true
                    scrub.dragMs = player.duration_ms > 0 ? mouse.x / width * player.duration_ms : 0
                }
                onPositionChanged: function(mouse) {
                    if (scrub.dragging && player.duration_ms > 0)
                        scrub.dragMs = Math.max(0, Math.min(player.duration_ms, mouse.x / width * player.duration_ms))
                }
                onReleased: {
                    if (player.duration_ms > 0)
                        player.seek(Math.max(0, Math.min(player.duration_ms, scrub.dragMs)))
                    scrub.dragging = false
                }
            }
        }

        Row {
            width: scrub.width
            anchors.horizontalCenter: parent.horizontalCenter
            Text {
                text: root.formatTime(scrub.dragging ? scrub.dragMs : (player ? player.position_ms : 0))
                color: "#b8b2c4"; font.pixelSize: 12
            }
            Item { width: parent.width - 80; height: 1 }
            Text {
                text: root.formatTime(player ? player.duration_ms : 0)
                color: "#b8b2c4"; font.pixelSize: 12
            }
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 20
            IconButton { size: 52; glyph: "\uf074"; tint: player && player.shuffle ? "#c4b0ff" : "#f5f2fb"
                         onClicked: player.toggle_shuffle() }
            IconButton { size: 52; glyph: "\uf048"; onClicked: player.previous() }
            IconButton { size: 70; glyph: player && player.playing ? "\uf04c" : "\uf04b"
                         tint: "#16101f"; color: "#c4b0ff"; border.width: 0
                         onClicked: player.toggle_play() }
            IconButton { size: 52; glyph: "\uf051"; onClicked: player.next() }
            IconButton { size: 52; glyph: "\uf01e"; tint: player && player.repeat ? "#c4b0ff" : "#f5f2fb"
                         onClicked: player.toggle_repeat() }
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 14
            IconButton { size: 46; glyph: player && player.liked ? "\uf004" : "\uf08a"
                         tint: player && player.liked ? "#dbb5ff" : "#f5f2fb"
                         onClicked: player.toggle_like() }
            Item {
                width: 48; height: 48
                ThreeDotsMenu {
                    anchors.fill: parent
                    player: root.player
                    onLyricsRequested: { root.lyricsOpen = !root.lyricsOpen; root.queueOpen = false }
                    onQueueRequested: { root.queueOpen = !root.queueOpen; root.lyricsOpen = false }
                }
            }
        }
    }

    IconButton {
        glyph: "\uf066"
        z: 50
        anchors { top: parent.top; right: parent.right; topMargin: 46; rightMargin: 24 }
        onClicked: root.collapse()
    }

    Rectangle {
        id: lyricsPanel
        visible: root.lyricsOpen
        z: 40
        width: 420
        height: parent.height - 120
        radius: 18
        color: "#111016"
        border.width: 1
        border.color: "#2e2a3a"
        anchors { right: parent.right; rightMargin: 26; verticalCenter: parent.verticalCenter }
        Column {
            anchors.fill: parent
            anchors.margins: 18
            spacing: 10
            Item {
                width: parent.width
                height: 24
                Text { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; text: player.tr(player.language, "lyrics.title"); color: "#f5f2fb"; font { pixelSize: 18; bold: true } }
                Text { text: "✕"; color: "#b3adc0"; font.pixelSize: 16
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.lyricsOpen = false } }
            }
            Flickable {
                width: parent.width
                height: parent.height - 40
                contentHeight: lyricsText.implicitHeight
                clip: true
                Text {
                    id: lyricsText
                    width: parent.width
                    text: player && player.lyric.length ? player.lyric : player.tr(player.language, "lyrics.empty")
                    color: "#d5cfe0"
                    font.pixelSize: 14
                    wrapMode: Text.Wrap
                }
            }
        }
    }

    Rectangle {
        id: queuePanel
        visible: root.queueOpen
        z: 40
        width: 420
        height: parent.height - 120
        radius: 18
        color: "#111016"
        border.width: 1
        border.color: "#2e2a3a"
        anchors { left: parent.left; leftMargin: 26; verticalCenter: parent.verticalCenter }
        Column {
            anchors.fill: parent
            anchors.margins: 18
            spacing: 10
            Item {
                width: parent.width
                height: 24
                Text { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; text: player.tr(player.language, "queue.title"); color: "#f5f2fb"; font { pixelSize: 18; bold: true } }
                Text { text: "✕"; color: "#b3adc0"; font.pixelSize: 16
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.queueOpen = false } }
            }
            ListView {
                id: queueList
                width: parent.width
                height: parent.height - 40
                clip: true
                spacing: 2
                model: root.queueItems
                delegate: Rectangle {
                    required property var modelData
                    required property int index
                    width: queueList.width
                    height: 44
                    radius: 8
                    color: index === player.queue_position ? "#241f30" : (qh.containsMouse ? "#17151d" : "transparent")
                    Row {
                        anchors.fill: parent
                        anchors.margins: 6
                        spacing: 10
                        Text { text: (index + 1); color: "#8a8496"; font.pixelSize: 12; width: 18; anchors.verticalCenter: parent.verticalCenter }
                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - 40
                            Text { text: modelData.title; color: "#f5f2fb"; font.pixelSize: 13; elide: Text.ElideRight; width: parent.width }
                            Text { text: modelData.artist; color: "#b3adc0"; font.pixelSize: 11; elide: Text.ElideRight; width: parent.width }
                        }
                    }
                    MouseArea { id: qh; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: player.jump_to(index) }
                }
            }
        }
    }
}
