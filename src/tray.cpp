#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtCore/QEvent>
#include <QtCore/QStandardPaths>
#include <QtCore/QString>
#include <QtCore/QTimer>
#include <QtGui/QAction>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QWindow>
#include <QtNetwork/QLocalServer>
#include <QtNetwork/QLocalSocket>
#include <QtWidgets/QApplication>
#include <QtWidgets/QMenu>
#include <QtWidgets/QSystemTrayIcon>

extern "C" void exmusic_tray_event(int action);
extern "C" void exmusic_window_raise();

namespace {

QApplication* g_application = nullptr;
QSystemTrayIcon* g_tray = nullptr;
QLocalServer* g_server = nullptr;
QWindow* g_mainWindow = nullptr;
bool g_wasMaximized = false;
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
    if (g_mainWindow) {
        return g_mainWindow;
    }
    const auto windows = QGuiApplication::topLevelWindows();
    for (QWindow* window : windows) {
        if (window && window->type() == Qt::Window
            && window->title() == QStringLiteral("EXmusic")) {
            g_mainWindow = window;
            QObject::connect(window, &QObject::destroyed, []() {
                g_mainWindow = nullptr;
            });
            return window;
        }
    }
    for (QWindow* window : windows) {
        if (window && window->type() == Qt::Window) {
            g_mainWindow = window;
            QObject::connect(window, &QObject::destroyed, []() {
                g_mainWindow = nullptr;
            });
            return window;
        }
    }
    return nullptr;
}

bool windowShown(QWindow* window)
{
    return window && window->isVisible()
        && !(window->windowState() & Qt::WindowMinimized);
}

bool trayActive()
{
    return g_tray && g_tray->isVisible();
}

void showMainWindow(QWindow* window)
{
    if (!window) {
        return;
    }
    window->setWindowState(Qt::WindowNoState);
    if (g_wasMaximized) {
        window->showMaximized();
    } else {
        window->showNormal();
    }
    window->raise();
    window->requestActivate();
}

void toggleWindow()
{
    QWindow* window = mainWindow();
    if (!window) {
        return;
    }
    if (windowShown(window)) {
        g_wasMaximized = window->windowState() & Qt::WindowMaximized;
        window->hide();
    } else {
        showMainWindow(window);
    }
}

void updateShowAction()
{
    if (!g_showAction) {
        return;
    }
    g_showAction->setText(windowShown(mainWindow()) ? g_hideLabel : g_showLabel);
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
        updateShowAction();
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

QString singleInstanceName()
{
#ifdef Q_OS_WIN
    QString user = qEnvironmentVariable("USERNAME");
#else
    QString user = qEnvironmentVariable("USER");
#endif
    if (user.isEmpty()) {
        user = QStringLiteral("default");
    }

#ifdef Q_OS_WIN
    return QStringLiteral("exmusic-single-instance-") + user;
#else
    QString directory = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (directory.isEmpty()) {
        directory = QDir::tempPath();
    }
    QDir().mkpath(directory);
    return directory + QStringLiteral("/exmusic-") + user + QStringLiteral(".sock");
#endif
}

bool notifyExistingInstance(const QString& name)
{
    QLocalSocket socket;
    socket.connectToServer(name, QIODevice::WriteOnly);
    if (!socket.waitForConnected(300)) {
        return false;
    }
    socket.write("raise");
    socket.flush();
    socket.waitForBytesWritten(300);
    socket.disconnectFromServer();
    return true;
}

bool startSingleInstanceServer(const QString& name)
{
    g_server = new QLocalServer();
    if (!g_server->listen(name)) {
        if (notifyExistingInstance(name)) {
            return false;
        }
        QLocalServer::removeServer(name);
        g_server->listen(name);
    }

    QObject::connect(g_server, &QLocalServer::newConnection, []() {
        while (QLocalSocket* socket = g_server->nextPendingConnection()) {
            QObject::connect(socket, &QLocalSocket::disconnected,
                             socket, &QObject::deleteLater);
            socket->readAll();
        }
        QTimer::singleShot(0, []() {
            exmusic_window_raise();
        });
    });
    return true;
}

class WindowEventFilter : public QObject
{
public:
    using QObject::QObject;

protected:
    bool eventFilter(QObject* watched, QEvent* event) override
    {
        QWindow* window = qobject_cast<QWindow*>(watched);
        if (!window || window != mainWindow()) {
            return false;
        }

        if (event->type() == QEvent::WindowStateChange) {
            if ((window->windowState() & Qt::WindowMinimized) && trayActive()) {
                g_wasMaximized = window->windowState() & Qt::WindowMaximized;
                QTimer::singleShot(0, window, [window]() {
                    if (window->windowState() & Qt::WindowMinimized) {
                        window->hide();
                    }
                });
            }
        } else if (event->type() == QEvent::Close) {
            if (trayActive()) {
                event->ignore();
                g_wasMaximized = window->windowState() & Qt::WindowMaximized;
                window->hide();
                return true;
            }
        }

        return false;
    }
};

} // namespace

extern "C" {

int exmusic_application_new()
{
    if (g_application) {
        return 0;
    }
    static int argc = 1;
    static char applicationName[] = "exmusic";
    static char* argv[] = { applicationName, nullptr };
    g_application = new QApplication(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("EXmusic"));
    QCoreApplication::setApplicationVersion(QStringLiteral("0.1.0"));

    const QString instanceName = singleInstanceName();
    if (notifyExistingInstance(instanceName)) {
        return 1;
    }

    if (!startSingleInstanceServer(instanceName)) {
        return 1;
    }

    g_application->installEventFilter(new WindowEventFilter(g_application));
    return 0;
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
    showMainWindow(mainWindow());
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
        QWindow* window = mainWindow();
        if (window && !windowShown(window)) {
            showMainWindow(window);
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
    updateShowAction();
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
