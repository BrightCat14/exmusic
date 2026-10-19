#include <QtCore/QCoreApplication>
#include <QtCore/QString>
#include <QtGui/QAction>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QWindow>
#include <QtWidgets/QApplication>
#include <QtWidgets/QMenu>
#include <QtWidgets/QSystemTrayIcon>

extern "C" void exmusic_tray_event(int action);

namespace {

QApplication* g_application = nullptr;
QSystemTrayIcon* g_tray = nullptr;
QMenu* g_menu = nullptr;
QAction* g_trackAction = nullptr;
QAction* g_playPauseAction = nullptr;
QAction* g_nextAction = nullptr;
QAction* g_previousAction = nullptr;
QAction* g_showAction = nullptr;
QAction* g_quitAction = nullptr;

QString g_playLabel = QStringLiteral("Play");
QString g_pauseLabel = QStringLiteral("Pause");
QString g_nextLabel = QStringLiteral("Next");
QString g_previousLabel = QStringLiteral("Previous");
QString g_showLabel = QStringLiteral("Show EXmusic");
QString g_hideLabel = QStringLiteral("Hide EXmusic");
QString g_quitLabel = QStringLiteral("Quit");
QString g_nothingLabel = QStringLiteral("Nothing playing");

QString g_lastTitle;
QString g_lastArtist;
int g_lastPlaying = -1;

QIcon appIcon()
{
    QIcon icon;
    icon.addFile(QStringLiteral(":/qt/qml/app/backend/assets/exmusic.png"));
    icon.addFile(QStringLiteral(":/qt/qml/app/backend/assets/exmusic.svg"));
    return icon;
}

QWindow* mainWindow()
{
    const auto windows = QGuiApplication::topLevelWindows();
    for (QWindow* window : windows) {
        if (window && window->isVisible()) {
            return window;
        }
    }
    return windows.isEmpty() ? nullptr : windows.first();
}

void toggleWindow()
{
    QWindow* window = mainWindow();
    if (!window) {
        return;
    }
    if (window->isVisible()) {
        window->hide();
    } else {
        window->show();
        window->requestActivate();
    }
}

void ensureTray()
{
    if (g_tray || !QSystemTrayIcon::isSystemTrayAvailable()) {
        return;
    }

    g_tray = new QSystemTrayIcon(appIcon());
    g_tray->setToolTip(QStringLiteral("EXmusic"));

    g_menu = new QMenu();
    g_trackAction = g_menu->addAction(g_nothingLabel);
    g_trackAction->setEnabled(false);
    g_menu->addSeparator();

    g_playPauseAction = g_menu->addAction(g_playLabel);
    QObject::connect(g_playPauseAction, &QAction::triggered, []() {
        exmusic_tray_event(0);
    });

    g_nextAction = g_menu->addAction(g_nextLabel);
    QObject::connect(g_nextAction, &QAction::triggered, []() {
        exmusic_tray_event(1);
    });

    g_previousAction = g_menu->addAction(g_previousLabel);
    QObject::connect(g_previousAction, &QAction::triggered, []() {
        exmusic_tray_event(2);
    });

    g_menu->addSeparator();
    g_showAction = g_menu->addAction(g_showLabel);
    QObject::connect(g_showAction, &QAction::triggered, []() {
        toggleWindow();
    });

    g_quitAction = g_menu->addAction(g_quitLabel);
    QObject::connect(g_quitAction, &QAction::triggered, []() {
        QCoreApplication::quit();
    });

    QObject::connect(g_menu, &QMenu::aboutToShow, []() {
        if (g_showAction) {
            QWindow* window = mainWindow();
            g_showAction->setText(window && window->isVisible() ? g_hideLabel
                                                                : g_showLabel);
        }
    });

    QObject::connect(g_tray, &QSystemTrayIcon::activated,
                     [](QSystemTrayIcon::ActivationReason reason) {
                         if (reason == QSystemTrayIcon::Trigger
                             || reason == QSystemTrayIcon::DoubleClick) {
                             toggleWindow();
                         }
                     });

    g_tray->setContextMenu(g_menu);
    g_tray->show();
}

void updateTrayText()
{
    if (g_trackAction) {
        QString label = g_nothingLabel;
        if (!g_lastTitle.isEmpty()) {
            label = g_lastArtist.isEmpty()
                ? g_lastTitle
                : g_lastArtist + QStringLiteral(" — ") + g_lastTitle;
        }
        if (label.size() > 72) {
            label = label.left(69) + QStringLiteral("...");
        }
        g_trackAction->setText(label);
    }
    if (g_playPauseAction) {
        g_playPauseAction->setText(g_lastPlaying > 0 ? g_pauseLabel : g_playLabel);
    }
    if (g_tray) {
        QString tooltip = QStringLiteral("EXmusic");
        if (!g_lastTitle.isEmpty()) {
            tooltip = g_lastArtist.isEmpty()
                ? g_lastTitle
                : g_lastArtist + QStringLiteral(" — ") + g_lastTitle;
        }
        g_tray->setToolTip(tooltip);
    }
}

} // namespace

extern "C" {

void exmusic_application_new()
{
    if (g_application) {
        return;
    }
    static int argc = 1;
    static char applicationName[] = "exmusic";
    static char* argv[] = { applicationName, nullptr };
    g_application = new QApplication(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("EXmusic"));
    QCoreApplication::setApplicationVersion(QStringLiteral("0.1.0"));
}

int exmusic_application_exec()
{
    return g_application ? g_application->exec() : 1;
}

unsigned long long exmusic_window_handle()
{
    QWindow* window = mainWindow();
    return window ? static_cast<unsigned long long>(window->winId()) : 0;
}

void exmusic_window_raise()
{
    QWindow* window = mainWindow();
    if (!window) {
        return;
    }
    window->show();
    window->requestActivate();
}

void exmusic_quit()
{
    QCoreApplication::quit();
}

int exmusic_tray_set_visible(int visible)
{
    if (!visible) {
        if (g_tray) {
            g_tray->hide();
        }
        if (g_application) {
            g_application->setQuitOnLastWindowClosed(true);
        }
        return 0;
    }

    ensureTray();
    if (!g_tray) {
        if (g_application) {
            g_application->setQuitOnLastWindowClosed(true);
        }
        return 0;
    }

    g_tray->show();
    if (g_application) {
        g_application->setQuitOnLastWindowClosed(false);
    }
    return 1;
}

void exmusic_tray_set_labels(const char* play,
                             const char* pause,
                             const char* next,
                             const char* previous,
                             const char* show,
                             const char* hide,
                             const char* quit,
                             const char* nothing)
{
    g_playLabel = QString::fromUtf8(play ? play : "");
    g_pauseLabel = QString::fromUtf8(pause ? pause : "");
    g_nextLabel = QString::fromUtf8(next ? next : "");
    g_previousLabel = QString::fromUtf8(previous ? previous : "");
    g_showLabel = QString::fromUtf8(show ? show : "");
    g_hideLabel = QString::fromUtf8(hide ? hide : "");
    g_quitLabel = QString::fromUtf8(quit ? quit : "");
    g_nothingLabel = QString::fromUtf8(nothing ? nothing : "");

    if (g_nextAction) {
        g_nextAction->setText(g_nextLabel);
    }
    if (g_previousAction) {
        g_previousAction->setText(g_previousLabel);
    }
    if (g_showAction) {
        QWindow* window = mainWindow();
        g_showAction->setText(window && window->isVisible() ? g_hideLabel : g_showLabel);
    }
    if (g_quitAction) {
        g_quitAction->setText(g_quitLabel);
    }
    updateTrayText();
}

void exmusic_tray_set_metadata(const char* title, const char* artist, int playing)
{
    const QString titleText = QString::fromUtf8(title ? title : "");
    const QString artistText = QString::fromUtf8(artist ? artist : "");
    if (titleText == g_lastTitle && artistText == g_lastArtist
        && playing == g_lastPlaying) {
        return;
    }
    g_lastTitle = titleText;
    g_lastArtist = artistText;
    g_lastPlaying = playing;
    updateTrayText();
}

} // extern "C"
