import QtQuick
import app.backend

Rectangle {
    id: root
    property var player
    color: "#110e18"

    Column {
        anchors.centerIn: parent
        width: Math.min(parent.width - 64, 430)
        spacing: 18

        Text {
            text: player.tr(player.language, "login.title")
            color: "#f7f3fa"
            font { pixelSize: 32; weight: Font.Black }
        }
        Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: player.tr(player.language, "login.description")
            color: "#aaa4b5"
            font.pixelSize: 14
        }
        LanguageSelector {
            player: root.player
            z: open ? 100 : 0
        }
        Rectangle {
            id: clientId
            width: parent.width
            height: 44
            color: "#211b2d"
            radius: 10
            border.color: "#3b3248"
            TextInput {
                id: clientIdInput
                anchors.fill: parent
                anchors.margins: 12
                color: "white"
                selectByMouse: true
                clip: true
                Text { anchors.verticalCenter: parent.verticalCenter; text: player.tr(player.language, "login.clientId"); color: "#8f879c"; visible: !parent.text.length }
            }
        }
        Rectangle {
            id: clientSecret
            width: parent.width
            height: 44
            color: "#211b2d"
            radius: 10
            border.color: "#3b3248"
            TextInput {
                id: clientSecretInput
                anchors.fill: parent
                anchors.margins: 12
                color: "white"
                echoMode: TextInput.Password
                selectByMouse: true
                clip: true
                Text { anchors.verticalCenter: parent.verticalCenter; text: player.tr(player.language, "login.clientSecret"); color: "#8f879c"; visible: !parent.text.length }
            }
        }
        Rectangle {
            width: parent.width
            height: 46
            radius: 12
            color: mouse.containsMouse ? "#c4b0ff" : "#a98cf0"
            Text { anchors.centerIn: parent; text: player && player.login_state === "requesting" ? player.tr(player.language, "login.requesting") : player.tr(player.language, "login.continue"); color: "#20192c"; font.bold: true }
            MouseArea {
                id: mouse
                anchors.fill: parent
                hoverEnabled: true
                enabled: player && player.login_state !== "requesting" && player.login_state !== "waiting"
                cursorShape: Qt.PointingHandCursor
                onClicked: player.begin_login(clientIdInput.text, clientSecretInput.text)
            }
        }
        Column {
            width: parent.width
            spacing: 10
            visible: player && (player.login_state === "waiting" || player.login_state === "error")
            Text { text: player.tr(player.language, "login.instruction"); color: "#d7cfdf"; font.pixelSize: 14 }
            Text { text: player ? player.user_code : ""; color: "#c4b0ff"; font { pixelSize: 34; bold: true; letterSpacing: 4 } }
            Rectangle {
                width: 220; height: 40; radius: 10; color: "#2b233c"
                Text { anchors.centerIn: parent; text: player.tr(player.language, "login.openPage"); color: "#eee8ff" }
                MouseArea {
                    anchors.fill: parent
                    enabled: player && player.verification_url.length > 0
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Qt.openUrlExternally(player.verification_url)
                }
            }
        }
    }
}
