use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    println!("cargo::rerun-if-changed=src/app_icon.cpp");
    println!("cargo::rerun-if-changed=src/tray.cpp");
    println!("cargo::rerun-if-changed=assets/exmusic.svg");
    println!("cargo::rerun-if-changed=assets/exmusic.png");

    CxxQtBuilder::new()
        .qt_module("Quick")
        .qt_module("Svg")
        .qt_module("Widgets")
        .qt_module("Network")
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
                "qml/LanguageSelector.qml",
            ],
            qrc_files: &[
                "assets/fa-solid-900.ttf",
                "assets/exmusic.svg",
                "assets/exmusic.png",
                "assets/exmusic-with-no-bg.svg",
                "qml/VisualizerShader.frag.qsb",
                "qml/RoundedMask.frag.qsb",
                "qml/GaussianBlur.frag.qsb",
            ],
            ..Default::default()
        })
        .cc_builder(|cc| {
            cc.file("src/app_icon.cpp");
            cc.file("src/tray.cpp");
        })
        .build();
}
