import QtQuick

ShaderEffect {
    id: root

    property var player
    property vector2d resolution: Qt.vector2d(width, height)
    property real time: 0
    property real displayBass: 0
    property real displayMid: 0
    property real displayHigh: 0
    property real displayBeat: 0
    property vector4d audio: Qt.vector4d(
        displayBass, displayMid, displayHigh, displayBeat)

    fragmentShader: "VisualizerShader.frag.qsb"

    Timer {
        interval: 16
        repeat: true
        running: root.visible
        onTriggered: {
            var factor = 1 - Math.exp(-0.016 / 0.06)
            var bass = player && player.playing ? player.visual_bass : 0
            var mid = player && player.playing ? player.visual_mid : 0
            var high = player && player.playing ? player.visual_high : 0
            var beat = player && player.playing ? player.visual_beat : 0
            root.displayBass += (bass - root.displayBass) * factor
            root.displayMid += (mid - root.displayMid) * factor
            root.displayHigh += (high - root.displayHigh) * factor
            root.displayBeat += (beat - root.displayBeat) * factor
            root.time += player && player.playing ? 0.016 : 0.006
        }
    }
}
