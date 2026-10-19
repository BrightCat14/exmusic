import QtQuick

Item {
    id: row
    property var track: ({})
    property bool active: false
    property int number: -1
    height: 58

    signal clicked()
    signal artistRequested(string artistId)
    signal albumRequested(string albumId)

    Rectangle {
        anchors.fill: parent
        radius: 10
        color: row.active ? "#2f2643" : (hover.containsMouse ? "#221c30" : "transparent")
    }

    MouseArea {
        id: hover
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: row.clicked()
    }

    Row {
        anchors.fill: parent
        anchors.margins: 7
        spacing: 12

        Text {
            visible: row.number >= 0
            width: 24
            anchors.verticalCenter: parent.verticalCenter
            text: (row.number + 1)
            color: row.active ? "#c4b0ff" : "#8f879c"
            font.pixelSize: 13
            horizontalAlignment: Text.AlignHCenter
        }

        Rectangle {
            width: 44; height: 44; radius: 9; clip: true
            anchors.verticalCenter: parent.verticalCenter
            color: "#302842"
            CoverArt {
                id: coverArt
                anchors.fill: parent
                radius: 9
                backgroundColor: "transparent"
                source: row.track.coverSourceUrl || row.track.coverUrl || ""
            }
            Text {
                anchors.centerIn: parent
                text: coverArt.hasImage ? "" : "♪"
                color: "#e9e1ff"
                font { pixelSize: 20; bold: true }
            }
        }

        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - (row.number >= 0 ? 130 : 105)
            spacing: 2
            Text {
                text: row.track.title || ""
                color: "#f4f0f7"
                font.bold: true
                elide: Text.ElideRight
                width: parent.width
            }
            Row {
                spacing: 6
                width: parent.width
                Text {
                    text: row.track.artist || ""
                    color: artistHover.containsMouse ? "#c4b0ff" : "#aaa4b5"
                    font.pixelSize: 12
                    elide: Text.ElideRight
                    MouseArea {
                        id: artistHover
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: if (row.track.artistId) row.artistRequested(row.track.artistId)
                    }
                }
                Text {
                    visible: (row.track.album || "").length > 0
                    text: "·"
                    color: "#6f6980"
                    font.pixelSize: 12
                }
                Text {
                    text: row.track.album || ""
                    color: albumHover.containsMouse ? "#c4b0ff" : "#8f879c"
                    font.pixelSize: 12
                    elide: Text.ElideRight
                    width: parent.width - 120
                    MouseArea {
                        id: albumHover
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: if (row.track.albumId) row.albumRequested(row.track.albumId)
                    }
                }
            }
        }
    }
}
