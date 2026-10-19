mod i18n;
mod integrations;
mod player;
mod tray;
mod yandex;

use cxx_qt_lib::{QQmlApplicationEngine, QUrl};

unsafe extern "C" {
    fn exmusic_set_app_icon();
}

fn main() {
    tray::create_application();

    unsafe {
        exmusic_set_app_icon();
    }

    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/app/backend/qml/Main.qml"));
    }

    tray::exec();
}
