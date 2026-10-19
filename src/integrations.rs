use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, ActivityType, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use rustfm_scrobble::{Scrobble, Scrobbler};
use serde::{Deserialize, Serialize};

pub const DEFAULT_DISCORD_CLIENT_ID: &str = "1561784686110310470";

fn default_discord_client_id() -> String {
    DEFAULT_DISCORD_CLIENT_ID.to_owned()
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

pub fn spawn(config: IntegrationConfig) -> Integrations {
    let (sender, receiver) = mpsc::channel();
    let (event_sender, event_receiver) = mpsc::channel();
    thread::spawn(move || worker(config, receiver, event_sender));
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
    receiver: Receiver<IntegrationCommand>,
    events: Sender<String>,
) {
    let mut discord: Option<DiscordIpcClient> = None;
    let mut discord_cooldown: Option<Instant> = None;
    let mut scrobbler: Option<Scrobbler> = None;
    let mut current: Option<Current> = None;

    while let Ok(command) = receiver.recv() {
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
        }
    }

    clear_discord(&mut discord);
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

    let start = now_ms() - state.position_ms.max(0);
    let mut activity = Activity::new()
        .details(state.title.as_str())
        .state(state.artist.as_str())
        .activity_type(ActivityType::Listening);
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
