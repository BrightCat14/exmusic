<p align="center">
  <img src="assets/exmusic-with-no-bg.svg" alt="EXmusic icon" width="112">
</p>

<h3 align="center">A native desktop player for Yandex Music</h3>

<p align="center">
  A Rust-powered music player with a Qt Quick interface, personalized listening, and optional Discord and Last.fm integrations.
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-8b78d6.svg"></a>
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2024%20edition-orange.svg">
  <img alt="Qt Quick" src="https://img.shields.io/badge/UI-Qt%20Quick-41a58d.svg">
</p>

## Features

- **Yandex Music playback** - search for tracks, artists, and albums, and play tracks or lists.
- **My Wave** - listen to personalized recommendations and adjust diversity, mood, and language preferences.
- **Your music** - browse your collection, playlists, recommendations, listening history, and artist or album pages.
- **Playback controls** - manage the queue, seek through tracks, adjust volume, and view a reactive audio visualizer.
- **Discord Rich Presence** - optionally show playback activity in Discord.
- **Last.fm scrobbling** - optionally send now-playing updates and scrobbles to Last.fm.
- **Native desktop integration** - a Qt Quick UI and an installer that adds an application-menu entry and icons.

## Requirements

EXmusic is built with Rust and Qt 6. You will need:

- A stable Rust toolchain with Rust 2024 edition support.
- Qt 6 development packages, including Qt Quick and Qt SVG.
- A C++ build toolchain and `pkg-config`.
- ALSA development headers for audio output on Linux.

On Ubuntu or Debian, install the system dependencies with:

```bash
sudo apt update
sudo apt install build-essential pkg-config cmake qt6-base-dev qt6-declarative-dev qt6-svg-dev qt6-tools-dev qt6-tools-dev-tools libasound2-dev
```

Install Rust using [rustup](https://rustup.rs/) if it is not already available.

## Build and install

Clone the repository and enter the project directory:

```bash
git clone https://github.com/BrightCat14/exmusic.git
cd exmusic
```

Build a release binary:

```bash
cargo build --release
```

Install EXmusic for the current user. The installer builds the release binary by default:

```bash
./install.sh
```

The default installation prefix is `~/.local`. The installer places the executable in `bin`, installs desktop and icon files, and adds EXmusic to your application menu.

To install under a custom prefix:

```bash
./install.sh install --prefix "$HOME/.local"
```

To install system-wide under `/usr/local`:

```bash
./install.sh install --system
```

The system-wide option uses `sudo`. To uninstall, use the same scope or prefix as the installation:

```bash
./install.sh uninstall

# For a system-wide installation:
./install.sh uninstall --system
```

For packaging, the installer also supports `--destdir PATH`. Run `./install.sh --help` to see all available options.

## Sign in

1. Launch **EXmusic** from your application menu.
2. Choose **Continue with Yandex**.
3. Open the verification page shown by the app and enter the displayed device code.
4. Wait for the app to finish signing in.

The OAuth client ID and client secret fields are optional. Leaving them empty uses the default Yandex Music client credentials configured by the application.

## Integrations

Discord Rich Presence and Last.fm are optional and can be configured in the app's **Settings** page. Integration settings are stored locally in the user's application data directory, in `exmusic/integrations.json`.

Last.fm requires the relevant API credentials and session information. Keep credentials private and do not commit local configuration files containing secrets.

## Tech stack

- **Rust** for the application backend and playback logic.
- **Qt 6 / QML** for the desktop interface.
- **CXX-Qt** to connect Rust and Qt.
- **Rodio** for audio playback and **rustfft** for audio visualization.
- **Reqwest, Serde, and Serde JSON** for API requests and data handling.

## Project layout

```text
exmusic/
├── assets/       # Application icons and bundled resources
├── qml/          # Qt Quick interface
├── src/
│   ├── main.rs   # Application entry point
│   ├── player.rs # Player backend and playback state
│   ├── yandex.rs # Yandex Music API client
│   └── integrations.rs
├── build.rs      # Qt/QML build configuration
├── install.sh    # Install and uninstall script
└── Cargo.toml
```

## License

EXmusic is released under the [MIT License](LICENSE).

EXmusic is an independent project and is not affiliated with or endorsed by Yandex.
