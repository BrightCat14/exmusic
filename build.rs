use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .qt_module("Quick")
        .qml_module(QmlModule {
            uri: "app.backend",
            rust_files: &["src/player.rs"],
            qml_files: &[
                "qml/Main.qml",
                "qml/LoginView.qml",
                "qml/SearchView.qml",
                "qml/SettingsView.qml",
                "qml/Sidebar.qml",
                "qml/PlayerView.qml",
                "qml/TrackRow.qml",
                "qml/CollectionView.qml",
                "qml/FeedView.qml",
                "qml/HistoryView.qml",
                "qml/DetailView.qml",
                "qml/Visualizer.qml",
                "qml/IconButton.qml",
                "qml/Icon.qml",
                "qml/CoverArt.qml",
                "qml/ThreeDotsMenu.qml",
                "qml/NowPlayingBar.qml",
                "qml/FullViewPlayer.qml",
            ],
            qrc_files: &["assets/fa-solid-900.ttf", "qml/VisualizerShader.frag.qsb", "qml/RoundedMask.frag.qsb"],
            ..Default::default()
        })
        .build();
}
