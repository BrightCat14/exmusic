import QtQuick
import app.backend

Rectangle {
    id: root
    property var player
    color: "#08080c"

    Column {
        anchors.centerIn: parent
        width: Math.min(parent.width - 64, 430)
        spacing: 18

        Text {
            text: player.tr(player.language, "login.title")
            color: "#f5f2fb"
            font { pixelSize: 32; weight: Font.Black }
        }
        Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: player.tr(player.language, "login.description")
            color: "#b3adc0"
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
            color: "#14131b"
            radius: 10
            border.color: "#2e2a3a"
            TextInput {
                id: clientIdInput
                anchors.fill: parent
                anchors.margins: 12
                color: "#f5f2fb"
                selectByMouse: true
                clip: true
                Text { anchors.verticalCenter: parent.verticalCenter; text: player.tr(player.language, "login.clientId"); color: "#8a8496"; visible: !parent.text.length }
            }
        }
        Rectangle {
            id: clientSecret
            width: parent.width
            height: 44
            color: "#14131b"
            radius: 10
            border.color: "#2e2a3a"
            TextInput {
                id: clientSecretInput
                anchors.fill: parent
                anchors.margins: 12
                color: "#f5f2fb"
                echoMode: TextInput.Password
                selectByMouse: true
                clip: true
                Text { anchors.verticalCenter: parent.verticalCenter; text: player.tr(player.language, "login.clientSecret"); color: "#8a8496"; visible: !parent.text.length }
            }
        }
        Rectangle {
            width: parent.width
            height: 46
            radius: 12
            color: mouse.containsMouse ? "#c4b0ff" : "#c4b0ff"
            Text { anchors.centerIn: parent; text: player && player.login_state === "requesting" ? player.tr(player.language, "login.requesting") : player.tr(player.language, "login.continue"); color: "#16101f"; font.bold: true }
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
            Text { text: player.tr(player.language, "login.instruction"); color: "#d5cfe0"; font.pixelSize: 14 }
            Text { text: player ? player.user_code : ""; color: "#c4b0ff"; font { pixelSize: 34; bold: true; letterSpacing: 4 } }
            Rectangle {
                width: 220; height: 40; radius: 10; color: "#241f30"
                Text { anchors.centerIn: parent; text: player.tr(player.language, "login.openPage"); color: "#efe9ff" }
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
