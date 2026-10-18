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
            Image {
                id: coverSource
                anchors.fill: parent
                source: row.track.coverUrl || ""
                fillMode: Image.PreserveAspectCrop
                cache: true
                asynchronous: true
                visible: false
            }
            Canvas {
                id: roundedCover
                anchors.fill: parent
                onPaint: {
                    var ctx = getContext("2d")
                    var r = 9
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
            Text {
                anchors.centerIn: parent
                text: (row.track.coverUrl || "").length ? "" : "♪"
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
