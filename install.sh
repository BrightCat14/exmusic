#!/usr/bin/env bash
set -euo pipefail

APP_ID="exmusic"
APP_NAME="EXmusic"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

COMMAND="install"
PREFIX="${HOME}/.local"
BUILD=1
PROFILE="release"
USE_SUDO=0
DESTDIR="${DESTDIR:-}"

usage() {
    cat <<EOF
Usage: ./install.sh [install|uninstall] [options]

Build and install ${APP_NAME} as a desktop application, or remove it.

Commands:
  install              Build and install (default)
  uninstall            Remove installed files

Options:
  --system             Install to /usr/local using sudo
  --prefix PATH        Install under PATH (default: \$HOME/.local)
  --no-build           Skip cargo build and use the existing binary
  --debug              Use the debug build instead of release
  --destdir PATH       Stage files under PATH (for packaging)
  -u, --uninstall      Alias for the uninstall command
  -h, --help           Show this help

Examples:
  ./install.sh
  ./install.sh uninstall
  ./install.sh install --system
  ./install.sh install --prefix "\$HOME/.local" --debug
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        install|uninstall)
            COMMAND="$1"
            shift
            ;;
        -u|--uninstall)
            COMMAND="uninstall"
            shift
            ;;
        --system)
            PREFIX="/usr/local"
            USE_SUDO=1
            shift
            ;;
        --prefix)
            PREFIX="${2:?--prefix requires a path}"
            shift 2
            ;;
        --prefix=*)
            PREFIX="${1#*=}"
            shift
            ;;
        --destdir)
            DESTDIR="${2:?--destdir requires a path}"
            shift 2
            ;;
        --destdir=*)
            DESTDIR="${1#*=}"
            shift
            ;;
        --no-build)
            BUILD=0
            shift
            ;;
        --debug)
            PROFILE="debug"
            shift
            ;;
        --release)
            PROFILE="release"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

run() {
    if (( USE_SUDO )) && [[ -z "${DESTDIR}" ]]; then
        sudo "$@"
    else
        "$@"
    fi
}

refresh_caches() {
    local applications_dir="$1"
    local hicolor_dir="$2"

    if [[ -n "${DESTDIR}" ]]; then
        return
    fi
    if command -v update-desktop-database >/dev/null 2>&1; then
        run update-desktop-database "${applications_dir}" >/dev/null 2>&1 || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        run gtk-update-icon-cache -f -t "${hicolor_dir}" >/dev/null 2>&1 || true
    fi
}

BIN_DIR="${DESTDIR}${PREFIX}/bin"
APPLICATIONS_DIR="${DESTDIR}${PREFIX}/share/applications"
ICONS_DIR="${DESTDIR}${PREFIX}/share/icons/hicolor/scalable/apps"
ICONS_PNG_DIR="${DESTDIR}${PREFIX}/share/icons/hicolor/256x256/apps"
DESKTOP_FILE="${APPLICATIONS_DIR}/${APP_ID}.desktop"
INSTALLED_BIN="${BIN_DIR}/${APP_ID}"
INSTALLED_ICON="${ICONS_DIR}/${APP_ID}.svg"
INSTALLED_PNG_ICON="${ICONS_PNG_DIR}/${APP_ID}.png"

if [[ "${COMMAND}" == "uninstall" ]]; then
    echo "Uninstalling ${APP_NAME} from ${PREFIX}..."
    run rm -f "${INSTALLED_BIN}" "${DESKTOP_FILE}" "${INSTALLED_ICON}" "${INSTALLED_PNG_ICON}"
    refresh_caches "${APPLICATIONS_DIR}" "${DESTDIR}${PREFIX}/share/icons/hicolor"
    echo "Done."
    exit 0
fi

if (( BUILD )); then
    echo "Building ${APP_NAME} (${PROFILE})..."
    if [[ "${PROFILE}" == "release" ]]; then
        (cd "${SCRIPT_DIR}" && cargo build --release)
    else
        (cd "${SCRIPT_DIR}" && cargo build)
    fi
fi

BUILT_BIN="${SCRIPT_DIR}/target/${PROFILE}/${APP_ID}"
if [[ ! -x "${BUILT_BIN}" ]]; then
    echo "Binary not found: ${BUILT_BIN}" >&2
    echo "Run without --no-build to build it first." >&2
    exit 1
fi
if [[ ! -f "${SCRIPT_DIR}/assets/exmusic.svg" || ! -f "${SCRIPT_DIR}/assets/exmusic.png" ]]; then
    echo "Icon files not found in ${SCRIPT_DIR}/assets" >&2
    exit 1
fi

echo "Installing ${APP_NAME} to ${PREFIX}..."
run mkdir -p "${BIN_DIR}" "${APPLICATIONS_DIR}" "${ICONS_DIR}" "${ICONS_PNG_DIR}"
run install -m755 "${BUILT_BIN}" "${INSTALLED_BIN}"
run install -m644 "${SCRIPT_DIR}/assets/exmusic.svg" "${INSTALLED_ICON}"
run install -m644 "${SCRIPT_DIR}/assets/exmusic.png" "${INSTALLED_PNG_ICON}"

DESKTOP_TMP="$(mktemp)"
trap 'rm -f "${DESKTOP_TMP}"' EXIT
cat >"${DESKTOP_TMP}" <<EOF
[Desktop Entry]
Type=Application
Name=${APP_NAME}
Comment=Music player for Yandex Music
Exec=${PREFIX}/bin/${APP_ID}
TryExec=${PREFIX}/bin/${APP_ID}
Icon=${APP_ID}
Terminal=false
Categories=AudioVideo;Audio;Player;
Keywords=music;audio;player;yandex;
StartupNotify=true
StartupWMClass=${APP_ID}
EOF
run install -m644 "${DESKTOP_TMP}" "${DESKTOP_FILE}"

refresh_caches "${APPLICATIONS_DIR}" "${DESTDIR}${PREFIX}/share/icons/hicolor"
echo "Installed. Launch ${APP_NAME} from your application menu."
if [[ -z "${DESTDIR}" && ":${PATH}:" != *":${PREFIX}/bin:"* ]]; then
    echo "Note: ${PREFIX}/bin is not in PATH; the desktop entry still works."
fi
