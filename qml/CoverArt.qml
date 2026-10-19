import QtQuick

Item {
    id: root

    property string source: ""
    property int radius: 8
    property color backgroundColor: "transparent"
    property int maxRetries: 6

    readonly property bool hasImage: image.status === Image.Ready

    property int retryCount: 0

    readonly property string effectiveSource: {
        if (source.length === 0 || retryCount === 0 || source.indexOf("data:") === 0)
            return source
        return source + (source.indexOf("?") === -1 ? "?" : "&") + "r=" + retryCount
    }

    function retry() {
        if (root.retryCount < root.maxRetries)
            root.retryCount += 1
    }

    Rectangle {
        id: frame
        anchors.fill: parent
        radius: root.radius
        color: root.backgroundColor
        layer.enabled: root.radius > 0
        layer.effect: ShaderEffect {
            property vector2d size: Qt.vector2d(frame.width, frame.height)
            property real radius: root.radius
            fragmentShader: "RoundedMask.frag.qsb"
        }

        Image {
            id: image
            anchors.fill: parent
            source: root.effectiveSource
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
            visible: status === Image.Ready
        }
    }

    onSourceChanged: retryCount = 0

    Connections {
        target: image
        function onStatusChanged() {
            if (image.status === Image.Loading) {
                stallTimer.restart()
            } else {
                stallTimer.stop()
            }
            if (image.status === Image.Error)
                retryTimer.restart()
        }
    }

    Connections {
        target: image
        function onProgressChanged() {
            if (image.status === Image.Loading && image.progress < 1)
                stallTimer.restart()
        }
    }

    Timer {
        id: stallTimer
        interval: 8000
        repeat: false
        onTriggered: if (image.status === Image.Loading) root.retry()
    }

    Timer {
        id: retryTimer
        interval: 1200
        repeat: false
        onTriggered: root.retry()
    }
}
