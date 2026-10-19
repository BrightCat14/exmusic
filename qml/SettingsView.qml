import QtQuick

Item {
    id: root
    property var player

    function currentConfig() {
        var config = {
            discord_enabled: false,
            discord_client_id: "",
            lastfm_enabled: false,
            lastfm_api_key: "",
            lastfm_api_secret: "",
            lastfm_session_key: "",
            lastfm_username: "",
            lastfm_password: ""
        }
        if (!player) return config
        try {
            var parsed = JSON.parse(player.integration_config_json)
            for (var key in parsed) config[key] = parsed[key]
        } catch (error) {}
        return config
    }

    function saveConfig(extra) {
        var config = currentConfig()
        config.discord_client_id = discordClientId.text
        config.lastfm_api_key = lastfmApiKey.text
        config.lastfm_api_secret = lastfmApiSecret.text
        config.lastfm_username = lastfmUsername.text
        config.lastfm_password = lastfmPassword.text
        config.lastfm_session_key = lastfmSessionKey.text
        if (extra) {
            for (var key in extra) config[key] = extra[key]
        }
        player.save_integration_config(JSON.stringify(config))
    }

    Component.onCompleted: {
        var config = currentConfig()
        discordClientId.text = config.discord_client_id || ""
        lastfmApiKey.text = config.lastfm_api_key || ""
        lastfmApiSecret.text = config.lastfm_api_secret || ""
        lastfmUsername.text = config.lastfm_username || ""
        lastfmPassword.text = config.lastfm_password || ""
        lastfmSessionKey.text = config.lastfm_session_key || ""
    }

    Rectangle { anchors.fill: parent; color: "#131019" }
    Flickable {
        anchors.fill: parent
        contentHeight: content.height + 64
        clip: true
        Column {
            id: content
            anchors { left: parent.left; right: parent.right; top: parent.top }
            anchors.margins: 32
            spacing: 24
            Text { text: "Settings"; color: "#f7f3fa"; font { pixelSize: 32; weight: Font.Black } }
            Text { text: "Playback, appearance and integrations"; color: "#aaa4b5"; font.pixelSize: 14 }

            Rectangle {
                width: parent.width; height: 82; radius: 16; color: "#1d1728"
                Column {
                    anchors { left: parent.left; verticalCenter: parent.verticalCenter; leftMargin: 20 }
                    spacing: 5
                    Text { text: "Visualizer"; color: "#f4f0f7"; font.bold: true }
                    Text { text: "Show the reactive background on the player"; color: "#aaa4b5"; font.pixelSize: 12 }
                }
                Rectangle {
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter; rightMargin: 20 }
                    width: 52; height: 30; radius: 15
                    color: player.visualizer_enabled ? "#9b7df5" : "#493b68"
                    Rectangle { width: 24; height: 24; radius: 12; anchors.verticalCenter: parent.verticalCenter; x: player.visualizer_enabled ? 25 : 3; color: "white"; Behavior on x { NumberAnimation { duration: 120 } } }
                    MouseArea { anchors.fill: parent; onClicked: player.set_visualizer(!player.visualizer_enabled) }
                }
            }

            Rectangle {
                width: parent.width; height: 112; radius: 16; color: "#1d1728"
                Column { anchors.fill: parent; anchors.margins: 20; spacing: 12
                    Row { width: parent.width
                        Text { text: "Volume"; color: "#f4f0f7"; font.bold: true }
                        Item { width: parent.width - 100; height: 1 }
                        Text { text: player.volume_percent + "%"; color: "#c4b0ff"; font.bold: true }
                    }
                    Rectangle {
                        id: volumeTrack
                        width: parent.width; height: 6; radius: 3; color: "#493b68"
                        Rectangle { width: parent.width * player.volume_percent / 100; height: parent.height; radius: 3; color: "#c4b0ff" }
                        MouseArea {
                            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
                            height: 28
                            onPressed: player.set_volume(Math.max(0, Math.min(100, mouse.x / width * 100)))
                            onPositionChanged: if (pressed) player.set_volume(Math.max(0, Math.min(100, mouse.x / width * 100)))
                        }
                    }
                }
            }

            Rectangle {
                width: parent.width; radius: 16; color: "#1d1728"
                height: integrations.height + 40
                Column {
                    id: integrations
                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
                    spacing: 14

                    Text { text: "Integrations"; color: "#f4f0f7"; font.bold: true }
                    Text { text: "Share what you are listening to on Discord and scrobble plays to Last.fm."; color: "#aaa4b5"; font.pixelSize: 12; width: parent.width; wrapMode: Text.WordWrap }

                    Row {
                        width: parent.width
                        Text { text: "Discord Rich Presence"; color: "#ddd6e6"; font.pixelSize: 14; anchors.verticalCenter: parent.verticalCenter }
                        Item { width: parent.width - 160; height: 1 }
                        Rectangle {
                            width: 52; height: 30; radius: 15
                            color: player.discord_enabled ? "#9b7df5" : "#493b68"
                            Rectangle { width: 24; height: 24; radius: 12; anchors.verticalCenter: parent.verticalCenter; x: player.discord_enabled ? 25 : 3; color: "white"; Behavior on x { NumberAnimation { duration: 120 } } }
                            MouseArea { anchors.fill: parent; onClicked: root.saveConfig({ discord_enabled: !player.discord_enabled }) }
                        }
                    }
                    Rectangle {
                        width: parent.width; height: 40; radius: 10; color: "#211b2d"; border.color: "#3b3248"
                        TextInput {
                            id: discordClientId
                            anchors.fill: parent; anchors.margins: 10
                            color: "white"; selectByMouse: true; clip: true
                            Text { anchors.verticalCenter: parent.verticalCenter; text: "Discord application client ID"; color: "#8f879c"; visible: !parent.text.length }
                        }
                    }

                    Row {
                        width: parent.width
                        Text { text: "Last.fm scrobbling"; color: "#ddd6e6"; font.pixelSize: 14; anchors.verticalCenter: parent.verticalCenter }
                        Item { width: parent.width - 160; height: 1 }
                        Rectangle {
                            width: 52; height: 30; radius: 15
                            color: player.lastfm_enabled ? "#9b7df5" : "#493b68"
                            Rectangle { width: 24; height: 24; radius: 12; anchors.verticalCenter: parent.verticalCenter; x: player.lastfm_enabled ? 25 : 3; color: "white"; Behavior on x { NumberAnimation { duration: 120 } } }
                            MouseArea { anchors.fill: parent; onClicked: root.saveConfig({ lastfm_enabled: !player.lastfm_enabled }) }
                        }
                    }
                    Rectangle {
                        width: parent.width; height: 40; radius: 10; color: "#211b2d"; border.color: "#3b3248"
                        TextInput {
                            id: lastfmApiKey
                            anchors.fill: parent; anchors.margins: 10
                            color: "white"; selectByMouse: true; clip: true
                            Text { anchors.verticalCenter: parent.verticalCenter; text: "Last.fm API key"; color: "#8f879c"; visible: !parent.text.length }
                        }
                    }
                    Rectangle {
                        width: parent.width; height: 40; radius: 10; color: "#211b2d"; border.color: "#3b3248"
                        TextInput {
                            id: lastfmApiSecret
                            anchors.fill: parent; anchors.margins: 10
                            color: "white"; echoMode: TextInput.Password; selectByMouse: true; clip: true
                            Text { anchors.verticalCenter: parent.verticalCenter; text: "Last.fm API secret"; color: "#8f879c"; visible: !parent.text.length }
                        }
                    }
                    Rectangle {
                        width: parent.width; height: 40; radius: 10; color: "#211b2d"; border.color: "#3b3248"
                        TextInput {
                            id: lastfmUsername
                            anchors.fill: parent; anchors.margins: 10
                            color: "white"; selectByMouse: true; clip: true
                            Text { anchors.verticalCenter: parent.verticalCenter; text: "Last.fm username (optional)"; color: "#8f879c"; visible: !parent.text.length }
                        }
                    }
                    Rectangle {
                        width: parent.width; height: 40; radius: 10; color: "#211b2d"; border.color: "#3b3248"
                        TextInput {
                            id: lastfmPassword
                            anchors.fill: parent; anchors.margins: 10
                            color: "white"; echoMode: TextInput.Password; selectByMouse: true; clip: true
                            Text { anchors.verticalCenter: parent.verticalCenter; text: "Last.fm password (optional, or use a session key)"; color: "#8f879c"; visible: !parent.text.length }
                        }
                    }
                    Rectangle {
                        width: parent.width; height: 40; radius: 10; color: "#211b2d"; border.color: "#3b3248"
                        TextInput {
                            id: lastfmSessionKey
                            anchors.fill: parent; anchors.margins: 10
                            color: "white"; selectByMouse: true; clip: true
                            Text { anchors.verticalCenter: parent.verticalCenter; text: "Last.fm session key (optional)"; color: "#8f879c"; visible: !parent.text.length }
                        }
                    }

                    Row {
                        width: parent.width
                        spacing: 14
                        Rectangle {
                            width: 120; height: 40; radius: 10
                            color: saveMouse.containsMouse ? "#c4b0ff" : "#a98cf0"
                            Text { anchors.centerIn: parent; text: "Save"; color: "#20192c"; font.bold: true }
                            MouseArea { id: saveMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.saveConfig(null) }
                        }
                        Text {
                            width: parent.width - 134; anchors.verticalCenter: parent.verticalCenter
                            text: player.integration_status
                            color: "#aaa4b5"; font.pixelSize: 12; elide: Text.ElideRight
                        }
                    }
                }
            }
        }
    }
}
