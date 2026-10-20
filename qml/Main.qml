import QtQuick
import app.backend

Window {
    id: win
    property string page: "search"
    property string detailKind: "album"
    property var stack: []
    width: 1280
    height: 800
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    color: "#08080c"
    title: "EXmusic"
    flags: Qt.Window | Qt.FramelessWindowHint

    PlayerBackend { id: backend }

    function navigate(target) {
        stack = []
        page = target
    }

    function openDetail(kind, id) {
        detailKind = kind
        if (kind === "artist") backend.open_artist(id)
        else if (kind === "playlist") backend.open_playlist(id)
        else backend.open_album(id)
        stack = stack.concat([page])
        page = "detail"
    }

    function goBack() {
        if (stack.length > 0) {
            page = stack[stack.length - 1]
            stack = stack.slice(0, stack.length - 1)
        } else {
            page = "search"
        }
    }

    Item {
        anchors.fill: parent
        visible: backend.authenticated

        Row {
            id: mainRow
            anchors { left: parent.left; right: parent.right; top: parent.top; bottom: nowBar.top }
            Sidebar {
                player: backend
                width: 272
                height: parent.height
                current: win.page
                onNavigate: function(target) { win.navigate(target) }
                onOpenPlaylist: function(uuid) { win.openDetail("playlist", uuid) }
                onSettingsRequested: win.navigate("settings")
            }
            Item {
                id: contentColumn
                width: parent.width - 272
                height: parent.height

                Item {
                    id: contentArea
                    anchors.fill: parent

                SearchView {
                    player: backend
                    anchors.fill: parent
                    visible: win.page === "search"
                    onOpenAlbum: function(albumId) { win.openDetail("album", albumId) }
                    onOpenArtist: function(artistId) { win.openDetail("artist", artistId) }
                }
                PlayerView {
                    player: backend
                    anchors.fill: parent
                    visible: win.page === "wave"
                    onExpandRequested: fullPlayer.visible = true
                }
                CollectionView {
                    player: backend
                    anchors.fill: parent
                    visible: win.page === "collection"
                    onOpenAlbum: function(albumId) { win.openDetail("album", albumId) }
                    onOpenArtist: function(artistId) { win.openDetail("artist", artistId) }
                }
                FeedView {
                    player: backend
                    anchors.fill: parent
                    visible: win.page === "feed"
                    onOpenAlbum: function(albumId) { win.openDetail("album", albumId) }
                    onOpenArtist: function(artistId) { win.openDetail("artist", artistId) }
                }
                HistoryView {
                    player: backend
                    anchors.fill: parent
                    visible: win.page === "history"
                    onOpenAlbum: function(albumId) { win.openDetail("album", albumId) }
                    onOpenArtist: function(artistId) { win.openDetail("artist", artistId) }
                }
                DetailView {
                    player: backend
                    anchors.fill: parent
                    visible: win.page === "detail"
                    kind: win.detailKind
                    onBackRequested: win.goBack()
                    onOpenAlbum: function(albumId) { win.openDetail("album", albumId) }
                    onOpenArtist: function(artistId) { win.openDetail("artist", artistId) }
                }
                SettingsView { player: backend; anchors.fill: parent; visible: win.page === "settings" }
                }
            }
        }

        NowPlayingBar {
            id: nowBar
            player: backend
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: win.page === "wave" ? 0 : 74
            visible: height > 0
            onExpand: fullPlayer.visible = true
        }
    }

    FullViewPlayer {
        id: fullPlayer
        player: backend
        anchors.fill: parent
        z: 5
        visible: false
        onCollapse: fullPlayer.visible = false
    }

    LoginView { anchors.fill: parent; player: backend; visible: !backend.authenticated }

    Timer {
        interval: 250
        running: !backend.authenticated
        repeat: true
        onTriggered: backend.poll_login()
    }

    Timer {
        interval: 100
        running: backend.authenticated
        repeat: true
        onTriggered: backend.poll_backend()
    }

    Component.onCompleted: backend.initialize()

    // Title bar: drag area + window controls
    Item {
        id: titleBar
        height: 40
        anchors { top: parent.top; left: parent.left; right: parent.right }
        z: 10
        DragHandler { target: null; onActiveChanged: if (active) win.startSystemMove() }
        TapHandler { acceptedButtons: Qt.LeftButton; gesturePolicy: TapHandler.DragThreshold
                     onDoubleTapped: win.visibility === Window.Maximized ? win.showNormal() : win.showMaximized() }
    }
    Row {
        anchors { top: parent.top; right: parent.right; rightMargin: 8; topMargin: 6 }
        spacing: 4
        z: 11
        Repeater {
            model: [
                { g: "—", a: 0 },
                { g: "☐", a: 1 },
                { g: "✕", a: 2 }
            ]
            delegate: Rectangle {
                width: 30; height: 28; radius: 8
                color: control.containsMouse ? "#1a1820" : "transparent"
                Text {
                    anchors.centerIn: parent
                    text: modelData.g; color: "#c4becf"; font.pixelSize: 13
                }
                id: control
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: modelData.a === 0 ? win.showMinimized()
                             : modelData.a === 1 ? (win.visibility === Window.Maximized ? win.showNormal() : win.showMaximized())
                             : win.close()
                }
            }
        }
    }
}
