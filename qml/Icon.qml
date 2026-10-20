import QtQuick

Text {
    id: root
    property string glyph: ""
    property color tint: "#f5f2fb"

    color: root.tint
    text: root.glyph
    font.family: faSolid.name
    font.styleName: "Solid"
    renderType: Text.NativeRendering

    FontLoader {
        id: faSolid
        source: "qrc:/qt/qml/app/backend/assets/fa-solid-900.ttf"
    }
}
