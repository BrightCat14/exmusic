mod integrations;
mod player;
mod yandex;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

unsafe extern "C" {
    fn exmusic_set_app_icon();
}

fn main() {
    let mut app = QGuiApplication::new();

    unsafe {
        exmusic_set_app_icon();
    }

    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/app/backend/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
