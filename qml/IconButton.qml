import QtQuick
import Qt5Compat.GraphicalEffects

Rectangle {
    id: root
    property string glyph: ""
    property bool active: true
    property int size: 48
    property color tint: "#f5f2fb"
    property real backgroundBlur: 24
    signal clicked()

    width: size
    height: size
    radius: size / 2
    color: area.containsMouse ? "#3dc4b0ff" : "#18c4b0ff"
    border.width: 1
    border.color: area.containsMouse ? "#55c4b0ff" : "#18c4b0ff"
    opacity: active ? 1 : 0.4

    function findBigParent(item) {
        if (!item) return null;
        let p = item.parent;
        let best = p;
        while (p) {
            if (p.width > 0 && (!best || p.width > best.width)) {
                best = p;
            }
            p = p.parent;
        }
        return best;
    }

    ShaderEffectSource {
        id: effectSource
        anchors.fill: parent
        sourceItem: root.findBigParent(root)
        sourceRect: {
            if (!sourceItem) return Qt.rect(0, 0, 0, 0);
            let mapped = root.mapToItem(sourceItem, 0, 0);
            return Qt.rect(Math.round(mapped.x),
                Math.round(mapped.y),
                root.width,
                root.height);
        }
        live: false
        recursive: false
        visible: false
    }


    FrameAnimation {
        running: root.backgroundBlur > 0 && root.visible
        onTriggered: effectSource.scheduleUpdate()
    }

    GaussianBlur {
        anchors.fill: parent
        source: effectSource
        radius: root.backgroundBlur
        samples: Math.min(32, root.backgroundBlur * 2)
        visible: root.backgroundBlur > 0
        z: -1

        layer.enabled: true
        layer.effect: OpacityMask {
            maskSource: Rectangle {
                width: root.width
                height: root.height
                radius: root.radius
            }
        }
    }

    Icon {
        anchors.centerIn: parent
        glyph: root.glyph
        tint: root.tint
        font.pixelSize: root.size * 0.38
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }
}
