use std::ffi::c_void;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, ActivityType, Assets, Button, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use rustfm_scrobble::{Scrobble, Scrobbler};
use serde::{Deserialize, Serialize};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};

pub const DEFAULT_DISCORD_CLIENT_ID: &str = "1561784686110310470";

fn default_discord_client_id() -> String {
    DEFAULT_DISCORD_CLIENT_ID.to_owned()
}

pub fn system_language() -> String {
    let mut candidates: Vec<String> = Vec::new();

    #[cfg(target_os = "windows")]
    if let Some(locale) = windows_preferred_locale() {
        candidates.push(locale);
    }

    for key in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            for part in value.split(':') {
                let part = part.trim();
                if !part.is_empty() {
                    candidates.push(part.to_owned());
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    if let Some(locale) = macos_preferred_locale() {
        candidates.push(locale);
    }

    candidates
        .iter()
        .find_map(|locale| language_code(locale))
        .unwrap_or_else(|| "en".to_owned())
}

fn language_code(locale: &str) -> Option<String> {
    let normalized = locale.trim().to_ascii_lowercase().replace('_', "-");
    let primary = normalized
        .split(|c| c == '.' || c == '@' || c == '-')
        .next()
        .unwrap_or("");
    match primary {
        "ru" => Some("ru".to_owned()),
        "en" => Some("en".to_owned()),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn windows_preferred_locale() -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    const LOCALE_NAME_MAX_LENGTH: usize = 85;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultLocaleName(locale_name: *mut u16, locale_name_length: i32) -> i32;
    }

    let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
    let length =
        unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), LOCALE_NAME_MAX_LENGTH as i32) };
    if length <= 1 {
        return None;
    }
    OsString::from_wide(&buffer[..length as usize - 1])
        .into_string()
        .ok()
}

#[cfg(target_os = "macos")]
fn macos_preferred_locale() -> Option<String> {
    let output = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct IntegrationConfig {
    #[serde(default)]
    pub discord_enabled: bool,
    #[serde(default = "default_discord_client_id")]
    pub discord_client_id: String,
    #[serde(default)]
    pub lastfm_enabled: bool,
    #[serde(default)]
    pub lastfm_api_key: String,
    #[serde(default)]
    pub lastfm_api_secret: String,
    #[serde(default)]
    pub lastfm_session_key: String,
    #[serde(default)]
    pub lastfm_username: String,
    #[serde(default)]
    pub lastfm_password: String,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            discord_enabled: false,
            discord_client_id: default_discord_client_id(),
            lastfm_enabled: false,
            lastfm_api_key: String::new(),
            lastfm_api_secret: String::new(),
            lastfm_session_key: String::new(),
            lastfm_username: String::new(),
            lastfm_password: String::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_volume_percent() -> i32 {
    100
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_true")]
    pub visualizer_enabled: bool,
    #[serde(default = "default_volume_percent")]
    pub volume_percent: i32,
    #[serde(default = "default_true")]
    pub tray_enabled: bool,
    #[serde(default = "default_true")]
    pub media_controls_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            visualizer_enabled: true,
            volume_percent: 100,
            tray_enabled: true,
            media_controls_enabled: true,
        }
    }
}

pub enum IntegrationCommand {
    NowPlaying {
        title: String,
        artist: String,
        album: String,
        cover_url: String,
        duration_ms: i64,
        position_ms: i64,
        playing: bool,
    },
    PlayingChanged {
        playing: bool,
        position_ms: i64,
    },
    Scrobble {
        title: String,
        artist: String,
        album: String,
        timestamp: i64,
    },
    Reconfigure(IntegrationConfig),
    WindowHandle(u64),
    MediaControlsEnabled(bool),
    VolumeChanged(f64),
}

pub struct Integrations {
    sender: Sender<IntegrationCommand>,
    events: Receiver<String>,
}

impl Integrations {
    pub fn send(&self, command: IntegrationCommand) {
        let _ = self.sender.send(command);
    }

    pub fn try_event(&self) -> Option<String> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }
}

pub fn spawn(config: IntegrationConfig, media_controls_enabled: bool) -> Integrations {
    let (sender, receiver) = mpsc::channel();
    let (event_sender, event_receiver) = mpsc::channel();
    thread::spawn(move || worker(config, media_controls_enabled, receiver, event_sender));
    Integrations {
        sender,
        events: event_receiver,
    }
}

fn config_path() -> Option<std::path::PathBuf> {
    Some(
        dirs::data_local_dir()
            .or_else(dirs::home_dir)?
            .join("exmusic")
            .join("integrations.json"),
    )
}

fn language_path() -> Option<std::path::PathBuf> {
    Some(
        dirs::data_local_dir()
            .or_else(dirs::home_dir)?
            .join("exmusic")
            .join("language"),
    )
}

fn settings_path() -> Option<std::path::PathBuf> {
    Some(
        dirs::data_local_dir()
            .or_else(dirs::home_dir)?
            .join("exmusic")
            .join("settings.json"),
    )
}

pub fn load_settings() -> AppSettings {
    settings_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<AppSettings>(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path =
        settings_path().ok_or_else(|| "Could not locate a writable data directory".to_string())?;
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

pub fn load_language() -> String {
    language_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| {
            !value.is_empty()
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '_'
                })
        })
        .unwrap_or_else(system_language)
}

pub fn save_language(language: &str) -> Result<(), String> {
    let path =
        language_path().ok_or_else(|| "Could not locate a writable data directory".to_string())?;
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, language).map_err(|error| error.to_string())
}

pub fn load_config() -> IntegrationConfig {
    let mut config = config_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<IntegrationConfig>(&text).ok())
        .unwrap_or_default();
    if config.discord_client_id.trim().is_empty() {
        config.discord_client_id = default_discord_client_id();
    }
    apply_env(&mut config);
    config
}

pub fn save_config(config: &IntegrationConfig) -> Result<(), String> {
    let path =
        config_path().ok_or_else(|| "Could not locate a writable data directory".to_string())?;
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

fn apply_env(config: &mut IntegrationConfig) {
    fn set(target: &mut String, key: &str) {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                *target = value;
            }
        }
    }
    set(&mut config.discord_client_id, "DISCORD_CLIENT_ID");
    set(&mut config.lastfm_api_key, "LASTFM_API_KEY");
    set(&mut config.lastfm_api_secret, "LASTFM_API_SECRET");
    set(&mut config.lastfm_session_key, "LASTFM_SESSION_KEY");
    set(&mut config.lastfm_username, "LASTFM_USERNAME");
    set(&mut config.lastfm_password, "LASTFM_PASSWORD");
}

struct Current {
    title: String,
    artist: String,
    album: String,
    cover_url: String,
    duration_ms: i64,
    position_ms: i64,
    playing: bool,
}

fn worker(
    mut config: IntegrationConfig,
    media_controls_enabled: bool,
    receiver: Receiver<IntegrationCommand>,
    events: Sender<String>,
) {
    let mut discord: Option<DiscordIpcClient> = None;
    let mut discord_cooldown: Option<Instant> = None;
    let mut scrobbler: Option<Scrobbler> = None;
    let mut media: Option<MediaControls> = None;
    let mut media_enabled = media_controls_enabled;
    let mut media_cooldown: Option<Instant> = None;
    let mut window_handle: u64 = 0;
    let mut current: Option<Current> = None;
    let mut last_tick = Instant::now();

    if media_enabled {
        ensure_media(&mut media, &mut media_cooldown, window_handle, &events);
    }

    loop {
        let command = match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(command) => Some(command),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };

        if let Some(command) = command {
            match command {
                IntegrationCommand::NowPlaying {
                    title,
                    artist,
                    album,
                    cover_url,
                    duration_ms,
                    position_ms,
                    playing,
                } => {
                    current = Some(Current {
                        title,
                        artist,
                        album,
                        cover_url,
                        duration_ms,
                        position_ms,
                        playing,
                    });
                    update_discord(
                        &config,
                        &mut discord,
                        &mut discord_cooldown,
                        current.as_ref(),
                        &events,
                    );
                    if media_enabled {
                        update_media_metadata(
                            &mut media,
                            &mut media_cooldown,
                            window_handle,
                            current.as_ref(),
                            &events,
                        );
                    }
                    if config.lastfm_enabled && playing {
                        if let Some(state) = current.as_ref() {
                            publish_now_playing(&config, &mut scrobbler, state, &events);
                        }
                    }
                }
                IntegrationCommand::PlayingChanged {
                    playing,
                    position_ms,
                } => {
                    if let Some(state) = current.as_mut() {
                        state.playing = playing;
                        state.position_ms = position_ms;
                    }
                    update_discord(
                        &config,
                        &mut discord,
                        &mut discord_cooldown,
                        current.as_ref(),
                        &events,
                    );
                    if media_enabled {
                        update_media_playback(
                            &mut media,
                            &mut media_cooldown,
                            window_handle,
                            current.as_ref(),
                            &events,
                        );
                    }
                }
                IntegrationCommand::Scrobble {
                    title,
                    artist,
                    album,
                    timestamp,
                } => {
                    if config.lastfm_enabled {
                        send_scrobble(
                            &config,
                            &mut scrobbler,
                            &title,
                            &artist,
                            &album,
                            timestamp,
                            &events,
                        );
                    }
                }
                IntegrationCommand::Reconfigure(new_config) => {
                    let discord_changed = new_config.discord_enabled != config.discord_enabled
                        || new_config.discord_client_id != config.discord_client_id;
                    let lastfm_changed = new_config.lastfm_enabled != config.lastfm_enabled
                        || new_config.lastfm_api_key != config.lastfm_api_key
                        || new_config.lastfm_api_secret != config.lastfm_api_secret
                        || new_config.lastfm_session_key != config.lastfm_session_key
                        || new_config.lastfm_username != config.lastfm_username
                        || new_config.lastfm_password != config.lastfm_password;
                    config = new_config;
                    if discord_changed {
                        clear_discord(&mut discord);
                    }
                    if lastfm_changed {
                        scrobbler = None;
                    }
                    if config.discord_enabled {
                        update_discord(
                            &config,
                            &mut discord,
                            &mut discord_cooldown,
                            current.as_ref(),
                            &events,
                        );
                    }
                }
                IntegrationCommand::WindowHandle(handle) => {
                    window_handle = handle;
                    media_cooldown = None;
                    if media_enabled {
                        ensure_media(&mut media, &mut media_cooldown, window_handle, &events);
                        update_media_metadata(
                            &mut media,
                            &mut media_cooldown,
                            window_handle,
                            current.as_ref(),
                            &events,
                        );
                    }
                }
                IntegrationCommand::MediaControlsEnabled(enabled) => {
                    media_enabled = enabled;
                    if enabled {
                        media_cooldown = None;
                        ensure_media(&mut media, &mut media_cooldown, window_handle, &events);
                        update_media_metadata(
                            &mut media,
                            &mut media_cooldown,
                            window_handle,
                            current.as_ref(),
                            &events,
                        );
                    } else {
                        media = None;
                    }
                }
                IntegrationCommand::VolumeChanged(volume) => {
                    if let Some(controls) = media.as_mut() {
                        let _ = controls.set_volume(volume);
                    }
                }
            }
        }

        let now = Instant::now();
        let elapsed = now.duration_since(last_tick);
        last_tick = now;
        if let Some(state) = current.as_mut() {
            if state.playing {
                state.position_ms = state
                    .position_ms
                    .saturating_add(elapsed.as_millis().min(i64::MAX as u128) as i64);
                if state.duration_ms > 0 {
                    state.position_ms = state.position_ms.min(state.duration_ms);
                }
            }
        }

        if media_enabled {
            update_media_playback(
                &mut media,
                &mut media_cooldown,
                window_handle,
                current.as_ref(),
                &events,
            );
        }
    }

    clear_discord(&mut discord);
}

fn media_event(event: MediaControlEvent) -> String {
    let seek_direction = |direction: SeekDirection| match direction {
        SeekDirection::Forward => "forward",
        SeekDirection::Backward => "backward",
    };
    let body = match event {
        MediaControlEvent::Play => "play".to_owned(),
        MediaControlEvent::Pause => "pause".to_owned(),
        MediaControlEvent::Toggle => "toggle".to_owned(),
        MediaControlEvent::Next => "next".to_owned(),
        MediaControlEvent::Previous => "previous".to_owned(),
        MediaControlEvent::Stop => "stop".to_owned(),
        MediaControlEvent::Seek(direction) => {
            format!("seek:{}:10000", seek_direction(direction))
        }
        MediaControlEvent::SeekBy(direction, duration) => {
            format!(
                "seek:{}:{}",
                seek_direction(direction),
                duration.as_millis()
            )
        }
        MediaControlEvent::SetPosition(MediaPosition(position)) => {
            format!("position:{}", position.as_millis())
        }
        MediaControlEvent::SetVolume(volume) => format!("volume:{volume}"),
        MediaControlEvent::Raise => "raise".to_owned(),
        MediaControlEvent::Quit => "quit".to_owned(),
        MediaControlEvent::OpenUri(uri) => format!("open:{uri}"),
    };
    format!("media:{body}")
}

fn ensure_media(
    media: &mut Option<MediaControls>,
    cooldown: &mut Option<Instant>,
    window_handle: u64,
    events: &Sender<String>,
) -> bool {
    if media.is_some() {
        return true;
    }
    if let Some(until) = *cooldown {
        if Instant::now() < until {
            return false;
        }
    }
    #[cfg(target_os = "windows")]
    if window_handle == 0 {
        return false;
    }
    let hwnd = if window_handle == 0 {
        None
    } else {
        Some(window_handle as *mut c_void)
    };
    let config = PlatformConfig {
        display_name: "EXmusic",
        dbus_name: "exmusic",
        hwnd,
    };
    let mut controls = match MediaControls::new(config) {
        Ok(controls) => controls,
        Err(error) => {
            *cooldown = Some(Instant::now() + Duration::from_secs(30));
            let _ = events.send(format!("Media controls: {error}"));
            return false;
        }
    };
    let media_events = events.clone();
    if let Err(error) = controls.attach(move |event| {
        let _ = media_events.send(media_event(event));
    }) {
        *cooldown = Some(Instant::now() + Duration::from_secs(30));
        let _ = events.send(format!("Media controls: {error}"));
        return false;
    }
    *cooldown = None;
    *media = Some(controls);
    true
}

fn update_media_metadata(
    media: &mut Option<MediaControls>,
    cooldown: &mut Option<Instant>,
    window_handle: u64,
    current: Option<&Current>,
    events: &Sender<String>,
) {
    let Some(state) = current else {
        return;
    };
    if !ensure_media(media, cooldown, window_handle, events) {
        return;
    }
    let cover_url = (state.cover_url.starts_with("http://")
        || state.cover_url.starts_with("https://"))
    .then_some(state.cover_url.as_str());
    let duration = (state.duration_ms > 0).then(|| Duration::from_millis(state.duration_ms as u64));
    let metadata = MediaMetadata {
        title: (!state.title.is_empty()).then_some(state.title.as_str()),
        album: (!state.album.is_empty()).then_some(state.album.as_str()),
        artist: (!state.artist.is_empty()).then_some(state.artist.as_str()),
        cover_url,
        duration,
    };
    if let Some(controls) = media.as_mut() {
        if let Err(error) = controls.set_metadata(metadata) {
            let _ = events.send(format!("Media controls: {error}"));
        }
    }
    update_media_playback(media, cooldown, window_handle, current, events);
}

fn update_media_playback(
    media: &mut Option<MediaControls>,
    cooldown: &mut Option<Instant>,
    window_handle: u64,
    current: Option<&Current>,
    events: &Sender<String>,
) {
    let Some(state) = current else {
        return;
    };
    if state.title.is_empty() {
        return;
    }
    if !ensure_media(media, cooldown, window_handle, events) {
        return;
    }
    let progress = Some(MediaPosition(Duration::from_millis(
        state.position_ms.max(0) as u64,
    )));
    let playback = if state.playing {
        MediaPlayback::Playing { progress }
    } else {
        MediaPlayback::Paused { progress }
    };
    if let Some(controls) = media.as_mut() {
        if let Err(error) = controls.set_playback(playback) {
            let _ = events.send(format!("Media controls: {error}"));
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn ensure_discord(
    config: &IntegrationConfig,
    discord: &mut Option<DiscordIpcClient>,
    cooldown: &mut Option<Instant>,
    events: &Sender<String>,
) -> bool {
    if discord.is_some() {
        return true;
    }
    if config.discord_client_id.is_empty() {
        return false;
    }
    if let Some(until) = *cooldown {
        if Instant::now() < until {
            return false;
        }
    }
    let mut client = DiscordIpcClient::new(&config.discord_client_id);
    match client.connect() {
        Ok(()) => {
            *discord = Some(client);
            *cooldown = None;
            let _ = events.send("Discord: connected".to_owned());
            true
        }
        Err(error) => {
            *cooldown = Some(Instant::now() + Duration::from_secs(15));
            let _ = events.send(format!("Discord: {error}"));
            false
        }
    }
}

fn update_discord(
    config: &IntegrationConfig,
    discord: &mut Option<DiscordIpcClient>,
    cooldown: &mut Option<Instant>,
    current: Option<&Current>,
    events: &Sender<String>,
) {
    if !config.discord_enabled {
        return;
    }
    let Some(state) = current else {
        return;
    };
    if state.title.is_empty() {
        return;
    }
    if !ensure_discord(config, discord, cooldown, events) {
        return;
    }
    let mut buttons: Vec<Button> = Vec::new();
    buttons.push(Button::new("Yandex Music Native Client", "https://github.com/BrightCat14/exmusic"));

    let start = now_ms() - state.position_ms.max(0);
    let mut activity = Activity::new()
        .details(state.title.as_str())
        .state(state.artist.as_str())
        .activity_type(ActivityType::Listening)
        .buttons(buttons);
    if state.cover_url.starts_with("http://") || state.cover_url.starts_with("https://") {
        let mut assets = Assets::new().large_image(state.cover_url.as_str());
        if !state.album.is_empty() {
            assets = assets.large_text(state.album.as_str());
        }
        activity = activity.assets(assets);
    }
    if state.playing {
        let mut timestamps = Timestamps::new().start(start);
        if state.duration_ms > 0 {
            timestamps = timestamps.end(start + state.duration_ms);
        }
        activity = activity.timestamps(timestamps);
    }

    let result = discord
        .as_mut()
        .map(|client| client.set_activity(activity));
    if let Some(Err(error)) = result {
        let _ = events.send(format!("Discord: {error}"));
        *discord = None;
    }
}

fn clear_discord(discord: &mut Option<DiscordIpcClient>) {
    if let Some(client) = discord.as_mut() {
        let _ = client.clear_activity();
        let _ = client.close();
    }
    *discord = None;
}

fn ensure_scrobbler(
    config: &IntegrationConfig,
    scrobbler: &mut Option<Scrobbler>,
    events: &Sender<String>,
) -> bool {
    if scrobbler.is_some() {
        return true;
    }
    if config.lastfm_api_key.is_empty() || config.lastfm_api_secret.is_empty() {
        return false;
    }
    let mut client = Scrobbler::new(&config.lastfm_api_key, &config.lastfm_api_secret);
    if !config.lastfm_session_key.is_empty() {
        client.authenticate_with_session_key(&config.lastfm_session_key);
        *scrobbler = Some(client);
        true
    } else if !config.lastfm_username.is_empty() && !config.lastfm_password.is_empty() {
        match client.authenticate_with_password(&config.lastfm_username, &config.lastfm_password) {
            Ok(response) => {
                let _ = events.send(format!("lastfm:session:{}", response.key));
                *scrobbler = Some(client);
                true
            }
            Err(error) => {
                let _ = events.send(format!("Last.fm: {error}"));
                false
            }
        }
    } else {
        let _ = events.send("Last.fm: missing session key or credentials".to_owned());
        false
    }
}

fn publish_now_playing(
    config: &IntegrationConfig,
    scrobbler: &mut Option<Scrobbler>,
    state: &Current,
    events: &Sender<String>,
) {
    if state.title.is_empty() {
        return;
    }
    if !ensure_scrobbler(config, scrobbler, events) {
        return;
    }
    if let Some(client) = scrobbler.as_ref() {
        let track = Scrobble::new(&state.artist, &state.title, &state.album);
        if let Err(error) = client.now_playing(&track) {
            let _ = events.send(format!("Last.fm: {error}"));
        }
    }
}

fn send_scrobble(
    config: &IntegrationConfig,
    scrobbler: &mut Option<Scrobbler>,
    title: &str,
    artist: &str,
    album: &str,
    timestamp: i64,
    events: &Sender<String>,
) {
    if title.is_empty() {
        return;
    }
    if !ensure_scrobbler(config, scrobbler, events) {
        return;
    }
    if let Some(client) = scrobbler.as_ref() {
        let mut track = Scrobble::new(artist, title, album);
        track.with_timestamp(timestamp.max(0) as u64);
        if let Err(error) = client.scrobble(&track) {
            let _ = events.send(format!("Last.fm: {error}"));
        }
    }
}
