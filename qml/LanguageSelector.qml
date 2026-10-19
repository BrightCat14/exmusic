import QtQuick

Item {
    id: root
    property var player
    property string lang: player ? player.language : "en"
    property int controlWidth: 170
    property int controlHeight: 36
    property bool open: false
    property var languages: []

    width: controlWidth
    height: controlHeight

    function refresh() {
        if (!player)
            return
        try {
            var parsed = JSON.parse(player.languages_json)
            if (parsed && parsed.length)
                languages = parsed
        } catch (error) {}
    }

    function labelFor(value) {
        for (var index = 0; index < languages.length; index++)
            if (languages[index].code === value)
                return languages[index].name
        return value
    }

    Component.onCompleted: refresh()

    Connections {
        target: root.player
        function onLanguages_jsonChanged() { root.refresh() }
    }

    Rectangle {
        id: button
        anchors.fill: parent
        radius: root.controlHeight / 2
        color: root.open || buttonMouse.containsMouse ? "#3b2e59" : "#2b233c"
        border.width: 1
        border.color: "#493b68"

        Text {
            anchors { left: parent.left; leftMargin: 16; verticalCenter: parent.verticalCenter }
            text: root.labelFor(root.lang)
            color: "#e8ddff"
            font { pixelSize: 13; bold: true }
        }
        Text {
            anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter }
            text: "▾"
            color: "#aaa4b5"
            font.pixelSize: 12
        }
        MouseArea {
            id: buttonMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.open = !root.open
        }
    }

    Rectangle {
        id: menu
        visible: root.open
        z: 100
        width: root.width
        height: root.languages.length * 36 + 8
        radius: 12
        color: "#171320"
        border.width: 1
        border.color: "#332b45"
        anchors { top: button.bottom; topMargin: 8; left: button.left }

        Column {
            anchors.fill: parent
            anchors.margins: 4
            Repeater {
                model: root.languages
                delegate: Rectangle {
                    required property var modelData
                    width: parent.width
                    height: 36
                    radius: 8
                    color: modelData.code === root.lang ? "#2b233c" : (optionMouse.containsMouse ? "#241d33" : "transparent")
                    Text {
                        anchors { left: parent.left; leftMargin: 12; verticalCenter: parent.verticalCenter }
                        text: modelData.name
                        color: modelData.code === root.lang ? "#c4b0ff" : "#e8ddff"
                        font.pixelSize: 13
                    }
                    MouseArea {
                        id: optionMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            root.open = false
                            if (root.player)
                                root.player.set_ui_language(modelData.code)
                        }
                    }
                }
            }
        }
    }
}
