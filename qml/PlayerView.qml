import QtQuick
import app.backend

Item {
    id: root
    property var player
    signal expandRequested()
    property bool timelineHover: false
    property bool timelineDragging: false
    property bool pendingSeek: false
    property int dragPosition: 0
    property bool waveMenuOpen: false
    property bool lyricsOpen: false
    property bool queueOpen: false
    property string waveDiversity: ""
    property string waveMood: ""
    property string waveLanguage: ""
    property var queueItems: []

    function refreshQueue() {
        if (player && player.queue_json.length > 0)
            queueItems = JSON.parse(player.queue_json)
    }

    Component.onCompleted: refreshQueue()

    Connections {
        target: root.player
        function onQueue_jsonChanged() { root.refreshQueue() }
    }

    onDragPositionChanged: timeline.requestPaint()

    function formatTime(value) {
        var seconds = Math.max(0, Math.floor(value / 1000))
        var minutes = Math.floor(seconds / 60)
        seconds = seconds % 60
        return minutes + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    function applyWaveSettings() {
        var seeds = ["user:onyourwave"]
        if (waveDiversity.length) seeds.push(waveDiversity)
        if (waveMood.length) seeds.push(waveMood)
        if (waveLanguage.length) seeds.push(waveLanguage)
        player.select_wave_mode(seeds.join(","))
    }

    function selectedWaveSeed(category) {
        return category === "diversity" ? waveDiversity
             : category === "mood" ? waveMood : waveLanguage
    }

    Timer {
        interval: 100
        repeat: true
        running: root.timelineHover && !root.timelineDragging && !root.pendingSeek
        onTriggered: {
            root.dragPosition = player.position_ms
        }
    }

    Timer {
        id: seekTimeout
        interval: 800
        repeat: false
        onTriggered: root.pendingSeek = false
    }

    Connections {
        target: root.player
        function onPosition_msChanged() {
            if (root.pendingSeek && Math.abs(player.position_ms - root.dragPosition) < 500)
                root.pendingSeek = false
            timeline.requestPaint()
        }
        function onDuration_msChanged() { timeline.requestPaint() }
    }

    Rectangle {
        id: backgroundSlot
        anchors.fill: parent
        z: -1
        gradient: Gradient {
            orientation: Gradient.Vertical
            GradientStop { position: 0.0; color: "#38234f" }
            GradientStop { position: 0.55; color: "#1b152b" }
            GradientStop { position: 1.0; color: "#0e0c14" }
        }
    }

    Visualizer {
        anchors.fill: parent
        player: root.player
        visible: player && player.visualizer_enabled
        z: 0
    }

    Text {
        id: artist
        z: 1
        text: player.artist
        color: "#f7f3fa"
        font { pixelSize: 88; weight: Font.Black }
        anchors { horizontalCenter: parent.horizontalCenter; verticalCenter: parent.verticalCenter; verticalCenterOffset: -160 }
    }

    Rectangle {
        id: waveButton
        z: 2
        width: 150; height: 36; radius: 18
        anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: 50 }
        color: waveMenuOpen ? "#9b7df5" : "#241d32"
        Text { anchors.centerIn: parent; text: "My Wave  ▾"; color: waveMenuOpen ? "#21182d" : "#d8ccff"; font.pixelSize: 13; font.bold: true }
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: waveMenuOpen = !waveMenuOpen }
    }

    Rectangle {
        id: waveMenu
        z: 5
        visible: waveMenuOpen
        width: 430
        height: 300
        radius: 14
        color: "#211b2d"
        border.width: 1
        border.color: "#493b68"
        anchors { horizontalCenter: waveButton.horizontalCenter; top: waveButton.bottom; topMargin: 8 }
        Row {
            anchors.fill: parent
            anchors.margins: 8
            spacing: 8
            Column {
                width: (parent.width - 16) / 3
                spacing: 5
                Text { text: "Character"; color: "#c4b0ff"; font.bold: true; font.pixelSize: 12 }
                Repeater {
                    model: [
                        { label: "Anything", seed: "", category: "diversity" },
                        { label: "Favorites", seed: "settingDiversity:favorite", category: "diversity" },
                        { label: "Unfamiliar", seed: "settingDiversity:discover", category: "diversity" },
                        { label: "Popular", seed: "settingDiversity:popular", category: "diversity" }
                    ]
                    delegate: waveOption
                }
            }
            Column {
                width: (parent.width - 16) / 3
                spacing: 5
                Text { text: "Mood"; color: "#c4b0ff"; font.bold: true; font.pixelSize: 12 }
                Repeater {
                    model: [
                        { label: "Any mood", seed: "", category: "mood" },
                        { label: "Energetic", seed: "settingMoodEnergy:active", category: "mood" },
                        { label: "Cheerful", seed: "settingMoodEnergy:fun", category: "mood" },
                        { label: "Calm", seed: "settingMoodEnergy:calm", category: "mood" },
                        { label: "Sad", seed: "settingMoodEnergy:sad", category: "mood" }
                    ]
                    delegate: waveOption
                }
            }
            Column {
                width: (parent.width - 16) / 3
                spacing: 5
                Text { text: "Language"; color: "#c4b0ff"; font.bold: true; font.pixelSize: 12 }
                Repeater {
                    model: [
                        { label: "Any language", seed: "", category: "language" },
                        { label: "Russian", seed: "settingLanguage:russian", category: "language" },
                        { label: "Foreign", seed: "settingLanguage:not-russian", category: "language" },
                        { label: "Wordless", seed: "settingLanguage:without-words", category: "language" }
                    ]
                    delegate: waveOption
                }
            }
        }
    }

    Component {
        id: waveOption
        Rectangle {
            required property var modelData
            width: 128; height: 31; radius: 8
            color: modelData.seed === selectedWaveSeed(modelData.category)
                   ? "#3b2e59" : (optionMouse.containsMouse ? "#302542" : "transparent")
            Text { anchors.centerIn: parent; text: modelData.label; color: "#e8ddff"; font.pixelSize: 12 }
            MouseArea {
                id: optionMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    if (modelData.category === "diversity") root.waveDiversity = modelData.seed
                    else if (modelData.category === "mood") root.waveMood = modelData.seed
                    else root.waveLanguage = modelData.seed
                    root.applyWaveSettings()
                }
            }
        }
    }

    // Cover
    Rectangle {
        id: cover
        z: 2
        width: 148; height: 148; radius: 18
        anchors { horizontalCenter: parent.horizontalCenter; top: artist.bottom; topMargin: 54 }
        gradient: Gradient {
            GradientStop { position: 0; color: "#e7b4be" }
            GradientStop { position: 0.52; color: "#775b9e" }
            GradientStop { position: 1; color: "#181523" }
        }
        Image { id: coverSource; anchors.fill: parent; source: player.cover_url; fillMode: Image.PreserveAspectCrop; cache: true; asynchronous: true; visible: false }
        Canvas {
            id: roundedCover
            anchors.fill: parent
            onPaint: {
                var ctx = getContext("2d")
                var r = 18
                ctx.reset()
                ctx.beginPath()
                ctx.moveTo(r, 0); ctx.lineTo(width - r, 0)
                ctx.arc(width - r, r, r, -Math.PI / 2, 0)
                ctx.lineTo(width, height - r)
                ctx.arc(width - r, height - r, r, 0, Math.PI / 2)
                ctx.lineTo(r, height)
                ctx.arc(r, height - r, r, Math.PI / 2, Math.PI)
                ctx.lineTo(0, r)
                ctx.arc(r, r, r, Math.PI, Math.PI * 1.5)
                ctx.clip()
                if (coverSource.status === Image.Ready)
                    ctx.drawImage(coverSource, 0, 0, width, height)
            }
        }
        Connections { target: coverSource; function onStatusChanged() { roundedCover.requestPaint() } }
        Text { anchors.centerIn: parent; text: player.cover_url.length ? "" : "EX"; color: "#fff4fb"; font { pixelSize: 25; weight: Font.Black; letterSpacing: 3 } }
    }

    // Action row
    Row {
        id: actions
        width: childrenRect.width
        spacing: 10
        anchors { horizontalCenter: parent.horizontalCenter; top: cover.bottom; topMargin: 22 }

        IconButton { glyph: "\uf065"; onClicked: root.expandRequested() }
        Item {
            id: volumeControl
            width: 48
            height: 48
            property bool hovering: false

            Timer {
                id: volumeHide
                interval: 250
                onTriggered: volumeControl.hovering = false
            }

            IconButton {
                anchors.fill: parent
                glyph: player.volume_percent === 0 ? "\uf026" : "\uf028"
                onClicked: player.set_volume(player.volume_percent === 0 ? 100 : 0)
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    onEntered: { volumeHide.stop(); volumeControl.hovering = true }
                    onExited: volumeHide.restart()
                    onClicked: player.set_volume(player.volume_percent === 0 ? 100 : 0)
                }
            }

            Rectangle {
                id: volumePopup
                x: 0
                y: -218
                width: 48
                height: 210
                radius: 24
                color: "#221d2d"
                border.width: 1
                border.color: "#3b3248"
                visible: volumeControl.hovering
                z: 20

                Text {
                    anchors.top: parent.top
                    anchors.topMargin: 12
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: player.volume_percent + "%"
                    color: "#eee8ff"
                    font { pixelSize: 12; bold: true }
                }
                Rectangle {
                    id: volumeTrack
                    x: 22
                    y: 48
                    width: 4
                    height: 145
                    radius: 2
                    color: "#493b68"
                    Rectangle {
                        anchors.bottom: parent.bottom
                        width: parent.width
                        height: parent.height * player.volume_percent / 100
                        radius: parent.radius
                        color: "#c4b0ff"
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    onEntered: volumeHide.stop()
                    onExited: volumeHide.restart()
                    onPressed: player.set_volume(Math.max(0, Math.min(100, (volumeTrack.y + volumeTrack.height - mouse.y) / volumeTrack.height * 100)))
                    onPositionChanged: if (pressed) player.set_volume(Math.max(0, Math.min(100, (volumeTrack.y + volumeTrack.height - mouse.y) / volumeTrack.height * 100)))
                }
            }
        }
        IconButton { glyph: "\uf7a9" } // dislike
        Rectangle {
            width: 262; height: 48; radius: 24
            color: "#221d2d"
            border.width: 1; border.color: "#3b3248"
            clip: true
            Canvas {
                id: timeline
                anchors.fill: parent
                onPaint: {
                    var ctx = getContext("2d")
                    var radius = height / 2
                    var progress = player.duration_ms > 0
                            ? width * Math.min(1, ((root.timelineDragging || root.pendingSeek) ? root.dragPosition : player.position_ms) / player.duration_ms)
                            : 0
                    ctx.reset()
                    ctx.beginPath()
                    ctx.moveTo(radius, 0)
                    ctx.lineTo(width - radius, 0)
                    ctx.arc(width - radius, radius, radius, -Math.PI / 2, Math.PI / 2)
                    ctx.lineTo(radius, height)
                    ctx.arc(radius, radius, radius, Math.PI / 2, Math.PI * 1.5)
                    ctx.closePath()
                    ctx.fillStyle = "#221d2d"
                    ctx.fill()
                    ctx.save()
                    ctx.clip()
                    ctx.fillStyle = "#493b68"
                    ctx.fillRect(0, 0, progress, height)
                    ctx.restore()
                }
            }
            Text {
                anchors.centerIn: parent
                text: root.timelineHover
                      ? root.formatTime((root.timelineDragging || root.pendingSeek) ? root.dragPosition : player.position_ms) + " / " + root.formatTime(player.duration_ms)
                      : player.title
                color: "#d7cfdf"
                font { pixelSize: 16; bold: true }
                elide: Text.ElideRight
                width: parent.width - 24
                horizontalAlignment: Text.AlignHCenter
            }
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: {
                    root.timelineHover = true
                    root.dragPosition = player.position_ms
                }
                onExited: root.timelineHover = false
                onPressed: function(mouse) {
                    root.timelineHover = true
                    root.timelineDragging = true
                    root.dragPosition = player.duration_ms > 0
                            ? Math.max(0, Math.min(player.duration_ms, mouse.x / width * player.duration_ms))
                            : 0
                }
                onPositionChanged: function(mouse) {
                    if (pressed) root.dragPosition = player.duration_ms > 0
                            ? Math.max(0, Math.min(player.duration_ms, mouse.x / width * player.duration_ms))
                            : 0
                }
                onReleased: {
                    if (player.duration_ms > 0) {
                        player.seek(root.dragPosition)
                        root.pendingSeek = true
                        seekTimeout.restart()
                    }
                    root.timelineDragging = false
                }
                onCanceled: root.timelineDragging = false
            }
        }
        IconButton { glyph: player.liked ? "\uf004" : "\uf08a"; tint: player.liked ? "#ff7da8" : "white"
                     onClicked: player.toggle_like() }
        IconButton { glyph: "\uf1e6" }
        ThreeDotsMenu {
            onLyricsRequested: { root.lyricsOpen = !root.lyricsOpen; root.queueOpen = false }
            onQueueRequested: { root.queueOpen = !root.queueOpen; root.lyricsOpen = false }
        }
    }

    // Transport
    Row {
        width: childrenRect.width
        spacing: 24
        anchors { horizontalCenter: parent.horizontalCenter; top: actions.bottom; topMargin: 26 }

        TransportGlyph { glyph: "\uf074"; lit: player.shuffle; onClicked: player.toggle_shuffle() }
        TransportGlyph { glyph: "\uf048"; onClicked: player.previous() }
         IconButton {
             size: 66
             tint: "#20192c"
             color: "#c4b0ff"
             border.width: 0
             glyph: player.playing ? "\uf04c" : "\uf04b"
            onClicked: player.toggle_play()
        }
        TransportGlyph { glyph: "\uf051"; onClicked: player.next() }
        TransportGlyph { glyph: "\uf01e"; lit: player.repeat; onClicked: player.toggle_repeat() }
    }

    // Lyrics panel
    Rectangle {
        id: lyricsPanel
        visible: root.lyricsOpen
        z: 30
        width: 420
        height: parent.height - 120
        radius: 18
        color: "#141019"
        border.width: 1
        border.color: "#493b68"
        anchors { right: parent.right; rightMargin: 26; verticalCenter: parent.verticalCenter }
        Column {
            anchors.fill: parent
            anchors.margins: 18
            spacing: 10
            Item {
                width: parent.width
                height: 24
                Text { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; text: "Текст песни"; color: "#f4f0f7"; font { pixelSize: 18; bold: true } }
                Text {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "✕"; color: "#aaa4b5"; font.pixelSize: 16
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.lyricsOpen = false }
                }
            }
            Flickable {
                width: parent.width
                height: parent.height - 40
                contentHeight: lyricsText.implicitHeight
                clip: true
                Text {
                    id: lyricsText
                    width: parent.width
                    text: player.lyric.length ? player.lyric : "Текст появится после загрузки трека"
                    color: "#d7cfdf"
                    font.pixelSize: 14
                    wrapMode: Text.Wrap
                }
            }
        }
    }

    // Queue panel
    Rectangle {
        id: queuePanel
        visible: root.queueOpen
        z: 30
        width: 420
        height: parent.height - 120
        radius: 18
        color: "#141019"
        border.width: 1
        border.color: "#493b68"
        anchors { left: parent.left; leftMargin: 26; verticalCenter: parent.verticalCenter }
        Column {
            anchors.fill: parent
            anchors.margins: 18
            spacing: 10
            Item {
                width: parent.width
                height: 24
                Text { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; text: "Очередь"; color: "#f4f0f7"; font { pixelSize: 18; bold: true } }
                Text {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "✕"; color: "#aaa4b5"; font.pixelSize: 16
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.queueOpen = false }
                }
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
                    color: index === player.queue_position ? "#33284a" : (qh.containsMouse ? "#262033" : "transparent")
                    Row {
                        anchors.fill: parent
                        anchors.margins: 6
                        spacing: 10
                        Text { text: (index + 1); color: "#8f879c"; font.pixelSize: 12; width: 18; anchors.verticalCenter: parent.verticalCenter }
                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - 40
                            Text { text: modelData.title; color: "#f4f0f7"; font.pixelSize: 13; elide: Text.ElideRight; width: parent.width }
                            Text { text: modelData.artist; color: "#aaa4b5"; font.pixelSize: 11; elide: Text.ElideRight; width: parent.width }
                        }
                    }
                    MouseArea { id: qh; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: player.jump_to(index) }
                }
            }
        }
    }

    component TransportGlyph: Item {
        property string glyph
        property bool lit: false
        signal clicked()
        width: 24; height: 64
        Icon {
            anchors.centerIn: parent
            glyph: parent.glyph
            tint: "white"
            opacity: parent.lit ? 1 : 0.55
            font.pixelSize: 22
        }
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: parent.clicked() }
    }

    component Badge: Rectangle {
        property string label
        property color fill
        height: 26; radius: 13; color: fill
        width: t.implicitWidth + 24
        Text { id: t; anchors.centerIn: parent; text: parent.label; color: "white"; font { pixelSize: 12; bold: true } }
    }
}
