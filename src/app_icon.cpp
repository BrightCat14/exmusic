#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

extern "C" void exmusic_set_app_icon()
{
    QIcon icon;
    icon.addFile(QStringLiteral(":/qt/qml/app/backend/assets/exmusic.png"));
    icon.addFile(QStringLiteral(":/qt/qml/app/backend/assets/exmusic.svg"));

    QGuiApplication::setWindowIcon(icon);
    QGuiApplication::setDesktopFileName(QStringLiteral("exmusic"));
    QGuiApplication::setApplicationName(QStringLiteral("EXmusic"));
}
