use crate::integrations::{self, IntegrationCommand};
use crate::yandex::YandexClient;
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};
use rustfft::{FftPlanner, num_complex::Complex};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

enum LoginEvent {
    Device { code: String, url: String },
    Success { token: String },
    Error(String),
}

enum BackendEvent {
    Account {
        name: String,
        avatar_url: String,
        uid: String,
    },
    Search(String),
    WaveTracks {
        session_id: String,
        batch_id: String,
        tracks: Vec<crate::yandex::TrackItem>,
        replace: bool,
        autoplay: bool,
    },
    QueueLoaded(Vec<crate::yandex::TrackItem>),
    QueueId(String),
    Playback {
        track: crate::yandex::TrackItem,
        response: reqwest::blocking::Response,
        autoplay: bool,
    },
    Collection(String),
    Playlists(String),
    Album(String),
    Artist(String),
    Playlist(String),
    Feed(String),
    History(String),
    LikedIds(Vec<String>),
    LikeState {
        track_id: String,
        liked: bool,
    },
    Error(String),
}

fn spawn_wave_feedback(
    token: String,
    session_id: String,
    batch_id: String,
    event: &'static str,
    track_id: String,
    total_played_seconds: Option<f64>,
) {
    if session_id.is_empty() || batch_id.is_empty() {
        return;
    }
    thread::spawn(move || {
        let Ok(client) = YandexClient::new(token) else {
            return;
        };
        let track = (!track_id.is_empty()).then_some(track_id.as_str());
        if let Err(error) =
            client.wave_feedback(&session_id, event, &batch_id, track, total_played_seconds)
        {
            eprintln!("[yandex/wave] feedback {event} failed: {error}");
        }
    });
}

fn track_to_value(track: crate::yandex::TrackItem) -> Value {
    json!({
        "id": track.id,
        "title": track.title,
        "artist": track.artist,
        "artistId": track.artist_id,
        "album": track.album,
        "albumId": track.album_id,
        "coverUrl": track.cover_url,
        "coverSourceUrl": track.cover_source_url,
        "durationMs": track.duration_ms,
    })
}

const WAVE_QUEUE_LIMIT: usize = 200;

fn external_cover_url(url: &str) -> String {
    match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_owned(),
    }
}

fn scrobble_threshold(duration_ms: i64) -> i64 {
    if duration_ms <= 0 {
        return 240_000;
    }
    if duration_ms < 30_000 {
        return 0;
    }
    (duration_ms / 2).min(240_000)
}

fn tracks_to_values(tracks: Vec<crate::yandex::TrackItem>) -> Vec<Value> {
    tracks.into_iter().map(track_to_value).collect()
}

fn tracks_from_json(json_text: &str) -> Vec<crate::yandex::TrackItem> {
    let value: Value = serde_json::from_str(json_text).unwrap_or(Value::Null);
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(crate::yandex::TrackItem {
                        id: item.get("id")?.as_str()?.to_owned(),
                        title: item
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        artist: item
                            .get("artist")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        artist_id: item
                            .get("artistId")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        album: item
                            .get("album")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        album_id: item
                            .get("albumId")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        cover_url: item
                            .get("coverUrl")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        cover_source_url: item
                            .get("coverSourceUrl")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        duration_ms: item.get("durationMs").and_then(Value::as_i64).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone, Copy, Default)]
struct AudioBands {
    bass: f32,
    mid: f32,
    high: f32,
    beat: f32,
}

const FFT_SIZE: usize = 2048;
const HOP_SIZE: usize = 512;
const SAMPLE_RATE: f32 = 44_100.0;

struct AnalyzingSource<S> {
    inner: S,
    channels: u16,
    frame_samples: u16,
    ring: Vec<f32>,
    write_pos: usize,
    filled: usize,
    samples_since_analysis: usize,
    fft: Arc<dyn rustfft::Fft<f32>>,
    spectrum: Vec<Complex<f32>>,
    audio: Arc<Mutex<AudioBands>>,
    previous_bass: f32,
    smoothed: AudioBands,
}

impl<S: Source<Item = f32>> AnalyzingSource<S> {
    fn new(inner: S, audio: Arc<Mutex<AudioBands>>) -> Self {
        let mut planner = FftPlanner::new();
        Self {
            channels: inner.channels(),
            inner,
            frame_samples: 0,
            ring: vec![0.0; FFT_SIZE],
            write_pos: 0,
            filled: 0,
            samples_since_analysis: 0,
            fft: planner.plan_fft_forward(FFT_SIZE),
            spectrum: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            audio,
            previous_bass: 0.0,
            smoothed: AudioBands::default(),
        }
    }

    fn analyze(&mut self) {
        if self.filled < FFT_SIZE {
            return;
        }
        let mut rms = 0.0;
        for index in 0..FFT_SIZE {
            let sample = self.ring[(self.write_pos + index) % FFT_SIZE];
            rms += sample * sample;
            let hann =
                0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / (FFT_SIZE - 1) as f32).cos();
            self.spectrum[index] = Complex::new(sample * hann, 0.0);
        }
        rms = (rms / FFT_SIZE as f32).sqrt();
        self.fft.process(&mut self.spectrum);
        let band = |spectrum: &[Complex<f32>], low: f32, high: f32| {
            let start = (low * FFT_SIZE as f32 / SAMPLE_RATE).floor() as usize;
            let end = ((high * FFT_SIZE as f32 / SAMPLE_RATE).ceil() as usize).min(FFT_SIZE / 2);
            let values = &spectrum[start.max(1)..end];
            if values.is_empty() {
                0.0
            } else {
                (values
                    .iter()
                    .map(|value| (value.norm() * 2.0 / FFT_SIZE as f32).powi(2))
                    .sum::<f32>()
                    / values.len() as f32)
                    .sqrt()
            }
        };
        let bass = (band(&self.spectrum, 35.0, 160.0) * 34.0 + rms * 0.7).clamp(0.0, 1.0);
        let mid = (band(&self.spectrum, 160.0, 2000.0) * 75.0).clamp(0.0, 1.0);
        let high = (band(&self.spectrum, 2000.0, 10000.0) * 115.0).clamp(0.0, 1.0);
        let smooth = |old: f32, target: f32, tau: f32| {
            old + (target - old) * (1.0 - (-(HOP_SIZE as f32) / SAMPLE_RATE / tau).exp())
        };
        self.smoothed.bass = smooth(
            self.smoothed.bass,
            bass,
            if bass > self.smoothed.bass {
                0.045
            } else {
                0.24
            },
        );
        self.smoothed.mid = smooth(
            self.smoothed.mid,
            mid,
            if mid > self.smoothed.mid { 0.055 } else { 0.20 },
        );
        self.smoothed.high = smooth(
            self.smoothed.high,
            high,
            if high > self.smoothed.high {
                0.025
            } else {
                0.16
            },
        );
        self.smoothed.beat = ((bass - self.previous_bass - 0.012) * 24.0)
            .clamp(0.0, 1.0)
            .max(self.smoothed.beat * (-(HOP_SIZE as f32) / SAMPLE_RATE / 0.19).exp());
        self.previous_bass = bass;
        if let Ok(mut audio) = self.audio.lock() {
            *audio = self.smoothed;
        }
        self.filled = 0;
    }
}

impl<S: Source<Item = f32>> Iterator for AnalyzingSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        if self.frame_samples == 0 {
            self.ring[self.write_pos] = sample;
        } else {
            self.ring[self.write_pos] = (self.ring[self.write_pos] + sample) * 0.5;
        }
        self.frame_samples += 1;
        if self.frame_samples >= self.channels {
            self.frame_samples = 0;
            self.write_pos = (self.write_pos + 1) % FFT_SIZE;
            self.filled = (self.filled + 1).min(FFT_SIZE);
            self.samples_since_analysis += 1;
            if self.filled == FFT_SIZE && self.samples_since_analysis >= HOP_SIZE {
                self.analyze();
                self.samples_since_analysis = 0;
            }
        }
        Some(sample)
    }
}

impl<S: Source<Item = f32>> Source for AnalyzingSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, position: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(position)?;
        self.ring.fill(0.0);
        self.write_pos = 0;
        self.filled = 0;
        self.samples_since_analysis = 0;
        self.previous_bass = 0.0;
        self.smoothed = AudioBands::default();
        if let Ok(mut audio) = self.audio.lock() {
            *audio = AudioBands::default();
        }
        Ok(())
    }
}

struct StreamingReader {
    response: Mutex<reqwest::blocking::Response>,
    buffer: Vec<u8>,
    position: usize,
    cache: Arc<Mutex<StreamCache>>,
}

#[derive(Default)]
struct StreamCache {
    bytes: Vec<u8>,
    complete: bool,
}

impl StreamingReader {
    fn new(response: reqwest::blocking::Response, cache: Arc<Mutex<StreamCache>>) -> Self {
        Self {
            response: Mutex::new(response),
            buffer: Vec::new(),
            position: 0,
            cache,
        }
    }
}

impl Read for StreamingReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.position == self.buffer.len() {
            let read = self.response.lock().unwrap().read(output)?;
            self.buffer.extend_from_slice(&output[..read]);
            if let Ok(mut cache) = self.cache.lock() {
                cache.bytes.extend_from_slice(&output[..read]);
                if read == 0 {
                    cache.complete = true;
                }
            }
            self.position += read;
            return Ok(read);
        }
        let count = output.len().min(self.buffer.len() - self.position);
        output[..count].copy_from_slice(&self.buffer[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

impl Seek for StreamingReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let target = match position {
            SeekFrom::Start(value) => value as i64,
            SeekFrom::Current(value) => self.position as i64 + value,
            SeekFrom::End(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "stream cannot seek from end",
                ));
            }
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "stream cannot seek before its start",
            ));
        }
        let target = target as usize;
        while self.buffer.len() < target {
            let remaining = target - self.buffer.len();
            let mut chunk = vec![0; remaining.min(64 * 1024)];
            let read = self.response.lock().unwrap().read(&mut chunk)?;
            if read == 0 {
                if let Ok(mut cache) = self.cache.lock() {
                    cache.complete = true;
                }
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "stream ended before the seek position",
                ));
            }
            self.buffer.extend_from_slice(&chunk[..read]);
            if let Ok(mut cache) = self.cache.lock() {
                cache.bytes.extend_from_slice(&chunk[..read]);
            }
        }
        self.position = target;
        Ok(target as u64)
    }
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, playing)]
        #[qproperty(bool, liked)]
        #[qproperty(bool, shuffle)]
        #[qproperty(bool, repeat)]
        #[qproperty(bool, authenticated)]
        #[qproperty(QString, artist)]
        #[qproperty(QString, lyric)]
        #[qproperty(QString, blurb)]
        #[qproperty(QString, quality)]
        #[qproperty(QString, title)]
        #[qproperty(QString, cover_url)]
        #[qproperty(QString, account_name)]
        #[qproperty(QString, avatar_url)]
        #[qproperty(QString, search_results_json)]
        #[qproperty(QString, wave_results_json)]
        #[qproperty(QString, collection_json)]
        #[qproperty(QString, playlists_json)]
        #[qproperty(QString, album_json)]
        #[qproperty(QString, artist_json)]
        #[qproperty(QString, playlist_json)]
        #[qproperty(QString, feed_json)]
        #[qproperty(QString, history_json)]
        #[qproperty(QString, queue_json)]
        #[qproperty(QString, current_track_id)]
        #[qproperty(i32, queue_position)]
        #[qproperty(QString, wave_mode)]
        #[qproperty(QString, error)]
        #[qproperty(bool, loading)]
        #[qproperty(i32, duration_ms)]
        #[qproperty(i32, position_ms)]
        #[qproperty(i32, volume_percent)]
        #[qproperty(QString, language)]
        #[qproperty(QString, login_state)]
        #[qproperty(f64, visual_bass)]
        #[qproperty(f64, visual_mid)]
        #[qproperty(f64, visual_high)]
        #[qproperty(f64, visual_beat)]
        #[qproperty(bool, visualizer_enabled)]
        #[qproperty(QString, user_code)]
        #[qproperty(QString, verification_url)]
        #[qproperty(QString, login_error)]
        #[qproperty(bool, discord_enabled)]
        #[qproperty(bool, lastfm_enabled)]
        #[qproperty(bool, tray_enabled)]
        #[qproperty(bool, media_controls_enabled)]
        #[qproperty(QString, integration_status)]
        #[qproperty(QString, integration_config_json)]
        #[qproperty(QString, languages_json)]
        type PlayerBackend = super::PlayerBackendRust;

        #[qinvokable]
        fn tr(self: Pin<&mut PlayerBackend>, language: QString, key: QString) -> QString;
        #[qinvokable]
        fn tracks(self: Pin<&mut PlayerBackend>, language: QString, count: i32) -> QString;
        #[qinvokable]
        fn favorites(self: Pin<&mut PlayerBackend>, language: QString, count: i32) -> QString;

        #[qinvokable]
        fn save_integration_config(self: Pin<&mut PlayerBackend>, config_json: QString);

        #[qinvokable]
        fn begin_login(self: Pin<&mut PlayerBackend>, client_id: QString, client_secret: QString);
        #[qinvokable]
        fn poll_login(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn initialize(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn poll_backend(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn search(self: Pin<&mut PlayerBackend>, text: QString);
        #[qinvokable]
        fn load_wave(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn select_wave_mode(self: Pin<&mut PlayerBackend>, mode: QString);
        #[qinvokable]
        fn play_track(self: Pin<&mut PlayerBackend>, track_id: QString);
        #[qinvokable]
        fn seek(self: Pin<&mut PlayerBackend>, position_ms: i32);
        #[qinvokable]
        fn set_volume(self: Pin<&mut PlayerBackend>, volume_percent: i32);
        #[qinvokable]
        fn set_visualizer(self: Pin<&mut PlayerBackend>, enabled: bool);
        #[qinvokable]
        fn set_ui_language(self: Pin<&mut PlayerBackend>, language: QString);
        #[qinvokable]
        fn set_tray(self: Pin<&mut PlayerBackend>, enabled: bool);
        #[qinvokable]
        fn set_media_controls(self: Pin<&mut PlayerBackend>, enabled: bool);
        #[qinvokable]
        fn toggle_play(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn toggle_like(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn toggle_shuffle(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn toggle_repeat(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn next(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn previous(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn load_collection(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn load_playlists(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn open_album(self: Pin<&mut PlayerBackend>, album_id: QString);
        #[qinvokable]
        fn open_artist(self: Pin<&mut PlayerBackend>, artist_id: QString);
        #[qinvokable]
        fn open_playlist(self: Pin<&mut PlayerBackend>, playlist_uuid: QString);
        #[qinvokable]
        fn load_feed(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn load_history(self: Pin<&mut PlayerBackend>);
        #[qinvokable]
        fn load_lyrics(self: Pin<&mut PlayerBackend>, track_id: QString);
        #[qinvokable]
        fn play_list(self: Pin<&mut PlayerBackend>, tracks_json: QString, start_index: i32);
        #[qinvokable]
        fn jump_to(self: Pin<&mut PlayerBackend>, index: i32);
    }
}

pub struct PlayerBackendRust {
    playing: bool,
    liked: bool,
    shuffle: bool,
    repeat: bool,
    authenticated: bool,
    artist: QString,
    lyric: QString,
    blurb: QString,
    quality: QString,
    title: QString,
    cover_url: QString,
    account_name: QString,
    avatar_url: QString,
    search_results_json: QString,
    wave_results_json: QString,
    collection_json: QString,
    playlists_json: QString,
    album_json: QString,
    artist_json: QString,
    playlist_json: QString,
    feed_json: QString,
    history_json: QString,
    queue_json: QString,
    current_track_id: QString,
    queue_position: i32,
    wave_mode: QString,
    error: QString,
    loading: bool,
    duration_ms: i32,
    position_ms: i32,
    volume_percent: i32,
    language: QString,
    visual_bass: f64,
    visual_mid: f64,
    visual_high: f64,
    visual_beat: f64,
    visualizer_enabled: bool,
    login_state: QString,
    user_code: QString,
    verification_url: QString,
    login_error: QString,
    discord_enabled: bool,
    lastfm_enabled: bool,
    tray_enabled: bool,
    media_controls_enabled: bool,
    integration_status: QString,
    integration_config_json: QString,
    languages_json: QString,
    translations: crate::i18n::Translations,
    integration_config: integrations::IntegrationConfig,
    integrations: Option<integrations::Integrations>,
    settings: integrations::AppSettings,
    settings_dirty: bool,
    current_album: String,
    current_cover_source_url: String,
    current_duration_ms: i64,
    max_position_ms: i64,
    scrobbled: bool,
    login_events: Option<Receiver<LoginEvent>>,
    backend_events: Option<Receiver<BackendEvent>>,
    backend_sender: Option<Sender<BackendEvent>>,
    lyrics_events: Option<Receiver<String>>,
    audio_stream: Option<OutputStream>,
    audio_sink: Option<Sink>,
    audio_monitor: Arc<Mutex<AudioBands>>,
    queue: Vec<crate::yandex::TrackItem>,
    queue_index: usize,
    queue_id: String,
    wave_session_id: String,
    wave_batch_id: String,
    wave_active: bool,
    wave_fetching: bool,
    wave_pending_autoplay: bool,
    wave_seen: HashSet<String>,
    wave_batches: HashMap<String, String>,
    wave_tracks: Vec<crate::yandex::TrackItem>,
    user_id: String,
    liked_ids: HashSet<String>,
    stream_cache: Option<Arc<Mutex<StreamCache>>>,
}

impl Default for PlayerBackendRust {
    fn default() -> Self {
        let integration_config = integrations::load_config();
        let integration_config_json =
            serde_json::to_string(&integration_config).unwrap_or_else(|_| "{}".to_owned());
        let settings = integrations::load_settings();
        let integrations_handle =
            integrations::spawn(integration_config.clone(), settings.media_controls_enabled);
        let translations = crate::i18n::Translations::load();
        let languages_json = translations.languages_json();
        let language = integrations::load_language();
        let labels = crate::tray::Labels::new(&translations, &language);
        let tray_enabled = crate::tray::configure(settings.tray_enabled, &labels);
        Self {
            playing: false,
            liked: false,
            shuffle: false,
            repeat: false,
            authenticated: load_token().is_some(),
            artist: QString::from("EXmusic"),
            lyric: QString::from("Sign in to start listening"),
            blurb: QString::from("A Yandex Music desktop client"),
            quality: QString::from(""),
            title: QString::from(""),
            cover_url: QString::from(""),
            account_name: QString::from(""),
            avatar_url: QString::from(""),
            search_results_json: QString::from("[]"),
            wave_results_json: QString::from("[]"),
            collection_json: QString::from("[]"),
            playlists_json: QString::from("[]"),
            album_json: QString::from(""),
            artist_json: QString::from(""),
            playlist_json: QString::from(""),
            feed_json: QString::from("[]"),
            history_json: QString::from("[]"),
            queue_json: QString::from("[]"),
            current_track_id: QString::from(""),
            queue_position: 0,
            wave_mode: QString::from("user:onyourwave"),
            error: QString::from(""),
            loading: false,
            duration_ms: 0,
            position_ms: 0,
            volume_percent: settings.volume_percent.clamp(0, 100),
            language: QString::from(language.as_str()),
            visual_bass: 0.0,
            visual_mid: 0.0,
            visual_high: 0.0,
            visual_beat: 0.0,
            visualizer_enabled: settings.visualizer_enabled,
            login_state: QString::from("idle"),
            user_code: QString::from(""),
            verification_url: QString::from(""),
            login_error: QString::from(""),
            discord_enabled: integration_config.discord_enabled,
            lastfm_enabled: integration_config.lastfm_enabled,
            tray_enabled,
            media_controls_enabled: settings.media_controls_enabled,
            integration_status: QString::from(""),
            integration_config_json: QString::from(integration_config_json),
            languages_json: QString::from(languages_json),
            translations,
            integration_config,
            integrations: Some(integrations_handle),
            settings,
            settings_dirty: false,
            current_album: String::new(),
            current_cover_source_url: String::new(),
            current_duration_ms: 0,
            max_position_ms: 0,
            scrobbled: false,
            login_events: None,
            backend_events: None,
            backend_sender: None,
            lyrics_events: None,
            audio_stream: None,
            audio_sink: None,
            audio_monitor: Arc::new(Mutex::new(AudioBands::default())),
            queue: Vec::new(),
            queue_index: 0,
            queue_id: String::new(),
            wave_session_id: String::new(),
            wave_batch_id: String::new(),
            wave_active: false,
            wave_fetching: false,
            wave_pending_autoplay: false,
            wave_seen: HashSet::new(),
            wave_batches: HashMap::new(),
            wave_tracks: Vec::new(),
            user_id: String::new(),
            liked_ids: HashSet::new(),
            stream_cache: None,
        }
    }
}

impl qobject::PlayerBackend {
    fn event_sender(mut self: Pin<&mut Self>) -> Sender<BackendEvent> {
        let rust = self.as_mut().rust_mut().get_mut();
        if let Some(sender) = rust.backend_sender.clone() {
            return sender;
        }
        let (sender, receiver) = mpsc::channel();
        rust.backend_events = Some(receiver);
        rust.backend_sender = Some(sender.clone());
        sender
    }

    fn persist_settings(mut self: Pin<&mut Self>) {
        let (dirty, settings) = {
            let backend = self.as_ref().get_ref().rust();
            (backend.settings_dirty, backend.settings.clone())
        };
        if !dirty {
            return;
        }
        if let Err(error) = integrations::save_settings(&settings) {
            self.as_mut().set_integration_status(QString::from(error));
            return;
        }
        self.as_mut().rust_mut().get_mut().settings_dirty = false;
    }

    fn tray_labels(self: Pin<&Self>) -> crate::tray::Labels {
        let backend = self.as_ref().get_ref().rust();
        let language = backend.language.to_string();
        crate::tray::Labels::new(&backend.translations, &language)
    }

    fn handle_media_event(mut self: Pin<&mut Self>, action: &str) {
        match action {
            "play" => {
                if !*self.as_ref().playing() {
                    self.as_mut().toggle_play();
                }
            }
            "pause" => {
                if *self.as_ref().playing() {
                    self.as_mut().toggle_play();
                }
            }
            "toggle" => self.as_mut().toggle_play(),
            "next" => self.as_mut().next(),
            "previous" => self.as_mut().previous(),
            "stop" => {
                if *self.as_ref().playing() {
                    self.as_mut().toggle_play();
                }
                self.as_mut().seek(0);
            }
            "raise" => crate::tray::raise_window(),
            "quit" => crate::tray::quit(),
            _ => {
                if let Some(position) = action.strip_prefix("position:") {
                    if let Ok(position) = position.parse::<i64>() {
                        let duration = *self.as_ref().duration_ms() as i64;
                        let position = if duration > 0 {
                            position.clamp(0, duration)
                        } else {
                            position.max(0)
                        };
                        self.as_mut().seek(position.min(i32::MAX as i64) as i32);
                    }
                } else if let Some(seek) = action.strip_prefix("seek:") {
                    let Some((direction, amount)) = seek.split_once(':') else {
                        return;
                    };
                    let Ok(amount) = amount.parse::<i64>() else {
                        return;
                    };
                    let current = *self.as_ref().position_ms() as i64;
                    let target = if direction == "forward" {
                        current.saturating_add(amount)
                    } else {
                        current.saturating_sub(amount)
                    };
                    let duration = *self.as_ref().duration_ms() as i64;
                    let target = if duration > 0 {
                        target.clamp(0, duration)
                    } else {
                        target.max(0)
                    };
                    self.as_mut().seek(target.min(i32::MAX as i64) as i32);
                } else if let Some(volume) = action.strip_prefix("volume:") {
                    if let Ok(volume) = volume.parse::<f64>() {
                        self.as_mut()
                            .set_volume((volume * 100.0).round().clamp(0.0, 100.0) as i32);
                    }
                }
            }
        }
    }

    fn save_integration_config(mut self: Pin<&mut Self>, config_json: QString) {
        let text = config_json.to_string();
        let config: integrations::IntegrationConfig = match serde_json::from_str(&text) {
            Ok(config) => config,
            Err(error) => {
                self.as_mut()
                    .set_integration_status(QString::from(format!("Invalid config: {error}")));
                return;
            }
        };
        let json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_owned());
        self.as_mut().set_discord_enabled(config.discord_enabled);
        self.as_mut().set_lastfm_enabled(config.lastfm_enabled);
        self.as_mut()
            .set_integration_config_json(QString::from(json));
        {
            let backend = self.as_mut().rust_mut().get_mut();
            backend.integration_config = config.clone();
            if let Err(error) = integrations::save_config(&backend.integration_config) {
                backend.integration_status = QString::from(error);
            }
        }
        if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
            handle.send(IntegrationCommand::Reconfigure(config));
        }
    }

    fn begin_login(mut self: Pin<&mut Self>, client_id: QString, client_secret: QString) {
        let client_id = if client_id.to_string().is_empty() {
            std::env::var("YANDEX_CLIENT_ID").unwrap_or_default()
        } else {
            client_id.to_string()
        };
        let client_secret = if client_secret.to_string().is_empty() {
            std::env::var("YANDEX_CLIENT_SECRET").unwrap_or_default()
        } else {
            client_secret.to_string()
        };

        self.as_mut().set_login_state(QString::from("requesting"));
        self.as_mut().set_login_error(QString::from(""));
        self.as_mut().set_user_code(QString::from(""));

        let (sender, receiver) = mpsc::channel();
        self.as_mut().rust_mut().get_mut().login_events = Some(receiver);
        thread::spawn(move || {
            let client = match YandexClient::new("") {
                Ok(client) => client,
                Err(error) => {
                    let _ = sender.send(LoginEvent::Error(error.to_string()));
                    return;
                }
            };
            let device = match client.request_device_code(&client_id) {
                Ok(device) => device,
                Err(error) => {
                    let _ = sender.send(LoginEvent::Error(error.to_string()));
                    return;
                }
            };
            let device_code = device.device_code;
            let _ = sender.send(LoginEvent::Device {
                code: device.user_code,
                url: device.verification_url,
            });
            let interval = Duration::from_secs(device.interval.max(1));
            let deadline = std::time::Instant::now() + Duration::from_secs(device.expires_in);
            while std::time::Instant::now() < deadline {
                thread::sleep(interval);
                match client.poll_device_token(&device_code, &client_id, &client_secret) {
                    Ok(token) => {
                        let _ = sender.send(LoginEvent::Success {
                            token: token.access_token,
                        });
                        return;
                    }
                    Err(error) if error.to_string().contains("authorization_pending") => {}
                    Err(error) if error.to_string().contains("slow_down") => {
                        thread::sleep(Duration::from_secs(5));
                    }
                    Err(error) => {
                        let _ = sender.send(LoginEvent::Error(error.to_string()));
                        return;
                    }
                }
            }
            let _ = sender.send(LoginEvent::Error(
                "The device code expired. Please try again.".into(),
            ));
        });
    }

    fn poll_login(mut self: Pin<&mut Self>) {
        let event = match self.as_ref().get_ref().rust().login_events.as_ref() {
            Some(receiver) => receiver.try_recv(),
            None => return,
        };
        match event {
            Ok(LoginEvent::Device { code, url }) => {
                self.as_mut().set_user_code(QString::from(code));
                self.as_mut().set_verification_url(QString::from(url));
                self.as_mut().set_login_state(QString::from("waiting"));
            }
            Ok(LoginEvent::Success { token }) => {
                if let Err(error) = save_token(&token) {
                    self.as_mut().set_login_error(QString::from(error));
                }
                self.as_mut().set_authenticated(true);
                self.as_mut()
                    .set_login_state(QString::from("authenticated"));
                self.as_mut().set_login_error(QString::from(""));
                self.as_mut().rust_mut().get_mut().login_events = None;
                self.as_mut().initialize();
            }
            Ok(LoginEvent::Error(error)) => {
                eprintln!("[yandex/oauth] {error}");
                self.as_mut().set_login_state(QString::from("error"));
                self.as_mut().set_login_error(QString::from(error));
                self.as_mut().rust_mut().get_mut().login_events = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                eprintln!("[yandex/oauth] Login worker stopped unexpectedly.");
                self.as_mut().set_login_state(QString::from("error"));
                self.as_mut()
                    .set_login_error(QString::from("Login worker stopped unexpectedly."));
                self.as_mut().rust_mut().get_mut().login_events = None;
            }
        }
    }

    fn initialize(mut self: Pin<&mut Self>) {
        if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
            handle.send(IntegrationCommand::WindowHandle(
                crate::tray::window_handle(),
            ));
        }
        let Some(token) = load_token() else {
            return;
        };
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let client = match YandexClient::new(token) {
                Ok(client) => client,
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                    return;
                }
            };
            match client.account_status() {
                Ok(status) => {
                    let name = status
                        .get("account")
                        .and_then(|account| {
                            account.get("login").or_else(|| account.get("displayName"))
                        })
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Yandex Music")
                        .to_owned();
                    let avatar_url = status
                        .get("account")
                        .and_then(|account| {
                            account.get("avatarId").or_else(|| account.get("avatar_id"))
                        })
                        .and_then(serde_json::Value::as_str)
                        .map(|avatar_id| {
                            format!("http://avatars.yandex.net/get-yapic/{avatar_id}/islands-200")
                        })
                        .unwrap_or_default();
                    let uid = status
                        .get("account")
                        .and_then(|account| account.get("uid"))
                        .map(|uid| uid.to_string().trim_matches('"').to_owned())
                        .unwrap_or_default();
                    let _ = sender.send(BackendEvent::Account {
                        name,
                        avatar_url,
                        uid: uid.clone(),
                    });
                    if !uid.is_empty() {
                        match client.liked_tracks(&uid) {
                            Ok(tracks) => {
                                let ids: Vec<String> =
                                    tracks.iter().map(|track| track.id.clone()).collect();
                                let _ = sender.send(BackendEvent::LikedIds(ids));
                            }
                            Err(error) => eprintln!("[yandex/likes] {error}"),
                        }
                    }
                    let mut initial_queue = Vec::new();
                    let mut queue_id = String::new();
                    let mut wave_session: Option<(String, String)> = None;
                    let initial = (|| {
                        let active = client.active_queue().ok().flatten();
                        let track_id = if let Some(active) = active {
                            queue_id = active.queue_id;
                            match active.track_id {
                                Some(track_id) => track_id,
                                None => {
                                    let batch = client.start_wave().ok()?;
                                    initial_queue = batch.tracks;
                                    wave_session = Some((batch.session_id, batch.batch_id));
                                    queue_id.clear();
                                    initial_queue.first().map(|track| track.id.clone())?
                                }
                            }
                        } else {
                            let batch = client.start_wave().ok()?;
                            initial_queue = batch.tracks;
                            wave_session = Some((batch.session_id, batch.batch_id));
                            initial_queue.first().map(|track| track.id.clone())?
                        };
                        let track = client.track(&track_id).ok()?;
                        let url = client.stream_url(&track_id).ok()?;
                        let response = client.open_stream(&url).ok()?;
                        Some((track, response))
                    })();
                    if !queue_id.is_empty() {
                        let _ = sender.send(BackendEvent::QueueId(queue_id.clone()));
                    }
                    if let Some((session_id, batch_id)) = wave_session {
                        if !initial_queue.is_empty() {
                            let _ = sender.send(BackendEvent::WaveTracks {
                                session_id,
                                batch_id,
                                tracks: initial_queue.clone(),
                                replace: true,
                                autoplay: false,
                            });
                        }
                    } else if !initial_queue.is_empty() {
                        let _ = sender.send(BackendEvent::QueueLoaded(initial_queue));
                    }
                    if let Some((track, response)) = initial {
                        let _ = sender.send(BackendEvent::Playback {
                            track,
                            response,
                            autoplay: false,
                        });
                    } else {
                        let _ = sender.send(BackendEvent::Error(
                            "No active queue or My Wave track was available.".into(),
                        ));
                    }
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn search(mut self: Pin<&mut Self>, text: QString) {
        let text = text.to_string();
        if text.trim().is_empty() {
            self.as_mut().set_search_results_json(QString::from("[]"));
            return;
        }
        let Some(token) = load_token() else {
            self.as_mut()
                .set_error(QString::from("Please sign in first."));
            return;
        };
        self.as_mut().set_loading(true);
        self.as_mut().set_error(QString::from(""));
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = YandexClient::new(token).and_then(|client| client.search(&text));
            match result {
                Ok(tracks) => {
                    let items = tracks_to_values(tracks);
                    let _ = sender.send(BackendEvent::Search(
                        serde_json::Value::Array(items).to_string(),
                    ));
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn load_wave(self: Pin<&mut Self>) {
        let mode = self.as_ref().wave_mode().to_string();
        self.load_wave_seed(mode);
    }

    fn select_wave_mode(mut self: Pin<&mut Self>, mode: QString) {
        let mode = mode.to_string();
        self.as_mut().set_wave_mode(QString::from(mode.clone()));
        self.load_wave_seed(mode);
    }

    fn load_wave_seed(mut self: Pin<&mut Self>, mode: String) {
        let Some(token) = load_token() else {
            self.as_mut()
                .set_error(QString::from("Please sign in first."));
            return;
        };
        self.as_mut().set_loading(true);
        self.as_mut().set_error(QString::from(""));
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = YandexClient::new(token).and_then(|client| {
                client
                    .start_wave_with_seed(&mode)
                    .map(|batch| (client, batch))
            });
            match result {
                Ok((client, batch)) => {
                    let session_id = batch.session_id.clone();
                    let batch_id = batch.batch_id.clone();
                    let wave_tracks = batch.tracks;
                    let first_track_id = wave_tracks.first().map(|track| track.id.clone());
                    let _ = sender.send(BackendEvent::WaveTracks {
                        session_id: session_id.clone(),
                        batch_id: batch_id.clone(),
                        tracks: wave_tracks,
                        replace: true,
                        autoplay: false,
                    });
                    if let Err(error) =
                        client.wave_feedback(&session_id, "radioStarted", &batch_id, None, None)
                    {
                        eprintln!("[yandex/wave] radioStarted failed: {error}");
                    }
                    if let Some(track_id) = first_track_id {
                        let result = (|| {
                            let track = client.track(&track_id)?;
                            let url = client.stream_url(&track_id)?;
                            let response = client.open_stream(&url)?;
                            Ok::<_, crate::yandex::Error>((track, response))
                        })();
                        match result {
                            Ok((track, response)) => {
                                let _ = sender.send(BackendEvent::Playback {
                                    track,
                                    response,
                                    autoplay: true,
                                });
                            }
                            Err(error) => {
                                let _ = sender.send(BackendEvent::Error(error.to_string()));
                            }
                        }
                    }
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn set_visualizer(mut self: Pin<&mut Self>, enabled: bool) {
        self.as_mut().set_visualizer_enabled(enabled);
        {
            let backend = self.as_mut().rust_mut().get_mut();
            backend.settings.visualizer_enabled = enabled;
            backend.settings_dirty = true;
        }
        self.as_mut().persist_settings();
    }

    fn set_tray(mut self: Pin<&mut Self>, enabled: bool) {
        let labels = self.as_ref().tray_labels();
        let active = crate::tray::configure(enabled, &labels);
        if enabled && !active {
            self.as_mut()
                .set_integration_status(QString::from("System tray is not available"));
        }
        self.as_mut().set_tray_enabled(active);
        {
            let backend = self.as_mut().rust_mut().get_mut();
            backend.settings.tray_enabled = active;
            backend.settings_dirty = true;
        }
        self.as_mut().persist_settings();
    }

    fn set_media_controls(mut self: Pin<&mut Self>, enabled: bool) {
        self.as_mut().set_media_controls_enabled(enabled);
        {
            let backend = self.as_mut().rust_mut().get_mut();
            backend.settings.media_controls_enabled = enabled;
            backend.settings_dirty = true;
        }
        self.as_mut().persist_settings();
        if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
            handle.send(IntegrationCommand::MediaControlsEnabled(enabled));
        }
    }

    fn tr(self: Pin<&mut Self>, language: QString, key: QString) -> QString {
        let language = language.to_string();
        let key = key.to_string();
        QString::from(
            self.as_ref()
                .get_ref()
                .rust()
                .translations
                .translate(&language, &key),
        )
    }

    fn tracks(self: Pin<&mut Self>, language: QString, count: i32) -> QString {
        QString::from(crate::i18n::Translations::track_count(
            &language.to_string(),
            count as i64,
        ))
    }

    fn favorites(self: Pin<&mut Self>, language: QString, count: i32) -> QString {
        QString::from(crate::i18n::Translations::favorite_count(
            &language.to_string(),
            count as i64,
        ))
    }

    fn set_ui_language(mut self: Pin<&mut Self>, language: QString) {
        let language = language.to_string().trim().to_ascii_lowercase();
        let language = if !language.is_empty()
            && language.chars().all(|character| {
                character.is_ascii_alphanumeric() || character == '-' || character == '_'
            }) {
            language
        } else {
            "en".to_owned()
        };
        self.as_mut().set_language(QString::from(language.as_str()));
        if let Err(error) = integrations::save_language(&language) {
            self.as_mut().rust_mut().get_mut().integration_status = QString::from(error);
        }
        let enabled = *self.as_ref().tray_enabled();
        let labels = {
            let backend = self.as_ref().get_ref().rust();
            crate::tray::Labels::new(&backend.translations, &language)
        };
        let active = crate::tray::configure(enabled, &labels);
        if enabled != active {
            self.as_mut().set_tray_enabled(active);
            let backend = self.as_mut().rust_mut().get_mut();
            backend.settings.tray_enabled = active;
            backend.settings_dirty = true;
            self.as_mut().persist_settings();
        }
    }

    fn play_track(mut self: Pin<&mut Self>, track_id: QString) {
        let track_id = track_id.to_string();
        let existing_index = self
            .as_ref()
            .get_ref()
            .rust()
            .queue
            .iter()
            .position(|track| track.id == track_id);
        if let Some(index) = existing_index {
            self.as_mut().rust_mut().get_mut().queue_index = index;
            self.as_mut().publish_queue();
            self.as_mut().sync_queue_position();
            self.request_playback(track_id);
            return;
        }
        {
            let backend = self.as_mut().rust_mut().get_mut();
            backend.queue.clear();
            backend.queue_index = 0;
            backend.wave_active = false;
            backend.wave_fetching = false;
            backend.wave_pending_autoplay = false;
            backend.wave_session_id.clear();
            backend.wave_batch_id.clear();
            backend.wave_seen.clear();
            backend.wave_batches.clear();
            backend.wave_tracks.clear();
        }
        self.request_playback(track_id);
    }

    fn request_playback(mut self: Pin<&mut Self>, track_id: String) {
        let Some(token) = load_token() else {
            self.as_mut()
                .set_error(QString::from("Please sign in first."));
            return;
        };
        self.as_mut().set_loading(true);
        self.as_mut().set_error(QString::from(""));
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = (|| {
                let client = YandexClient::new(token)?;
                let track = client.track(&track_id)?;
                let url = client.stream_url(&track_id)?;
                let response = client.open_stream(&url)?;
                Ok::<_, crate::yandex::Error>((track, response))
            })();
            match result {
                Ok((track, response)) => {
                    let _ = sender.send(BackendEvent::Playback {
                        track,
                        response,
                        autoplay: true,
                    });
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn poll_backend(mut self: Pin<&mut Self>) {
        self.as_mut().persist_settings();
        for action in crate::tray::drain_actions() {
            match action {
                crate::tray::TrayAction::TogglePlayback => self.as_mut().toggle_play(),
                crate::tray::TrayAction::Next => self.as_mut().next(),
                crate::tray::TrayAction::Previous => self.as_mut().previous(),
            }
        }
        let mut integration_events = Vec::new();
        if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
            while let Some(event) = handle.try_event() {
                integration_events.push(event);
            }
        }
        for event in integration_events {
            if let Some(session_key) = event.strip_prefix("lastfm:session:") {
                let session_key = session_key.to_owned();
                let json = {
                    let backend = self.as_mut().rust_mut().get_mut();
                    backend.integration_config.lastfm_session_key = session_key;
                    let _ = integrations::save_config(&backend.integration_config);
                    serde_json::to_string(&backend.integration_config)
                        .unwrap_or_else(|_| "{}".to_owned())
                };
                self.as_mut()
                    .set_integration_config_json(QString::from(json));
            } else if let Some(action) = event.strip_prefix("media:") {
                let action = action.to_owned();
                self.as_mut().handle_media_event(&action);
            } else {
                self.as_mut().set_integration_status(QString::from(event));
            }
        }
        let bands = self
            .as_ref()
            .get_ref()
            .rust()
            .audio_monitor
            .lock()
            .map(|bands| *bands)
            .unwrap_or_default();
        self.as_mut().set_visual_bass(bands.bass as f64);
        self.as_mut().set_visual_mid(bands.mid as f64);
        self.as_mut().set_visual_high(bands.high as f64);
        self.as_mut().set_visual_beat(bands.beat as f64);
        let tray_title = self.as_ref().title().to_string();
        let tray_artist = self.as_ref().artist().to_string();
        crate::tray::set_metadata(&tray_title, &tray_artist, *self.as_ref().playing());
        let position_ms = self
            .as_ref()
            .get_ref()
            .rust()
            .audio_sink
            .as_ref()
            .map(|sink| sink.get_pos().as_millis().min(i32::MAX as u128) as i32);
        if let Some(position_ms) = position_ms {
            self.as_mut().set_position_ms(position_ms);
            if *self.as_ref().playing() {
                let mut album = None;
                {
                    let backend = self.as_mut().rust_mut().get_mut();
                    backend.max_position_ms = backend.max_position_ms.max(position_ms as i64);
                    let threshold = scrobble_threshold(backend.current_duration_ms);
                    if !backend.scrobbled && threshold > 0 && backend.max_position_ms >= threshold
                    {
                        backend.scrobbled = true;
                        album = Some(backend.current_album.clone());
                    }
                }
                if let Some(album) = album {
                    let title = self.as_ref().title().to_string();
                    let artist = self.as_ref().artist().to_string();
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_secs() as i64)
                        .unwrap_or(0);
                    if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
                        handle.send(IntegrationCommand::Scrobble {
                            title,
                            artist,
                            album,
                            timestamp,
                        });
                    }
                }
            }
        }
        if *self.as_ref().playing() {
            let (wave_active, fetching, remaining) = {
                let backend = self.as_ref().get_ref().rust();
                (
                    backend.wave_active,
                    backend.wave_fetching,
                    backend.queue.len().saturating_sub(backend.queue_index + 1),
                )
            };
            if wave_active && !fetching && remaining <= 2 {
                self.as_mut().request_wave_batch(false);
            }
        }
        let finished = self
            .as_ref()
            .get_ref()
            .rust()
            .audio_sink
            .as_ref()
            .map(|sink| sink.empty() && *self.as_ref().playing())
            .unwrap_or(false);
        if finished {
            if *self.as_ref().repeat() && self.as_mut().replay_cached() {
                return;
            }
            self.as_mut().set_playing(false);
            let position_ms = *self.as_ref().duration_ms() as i64;
            if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
                handle.send(IntegrationCommand::PlayingChanged {
                    playing: false,
                    position_ms,
                });
            }
            self.as_mut().advance(true);
            return;
        }
        let lyrics = self
            .as_ref()
            .get_ref()
            .rust()
            .lyrics_events
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some(lyrics) = lyrics {
            self.as_mut().set_lyric(QString::from(lyrics));
            self.as_mut().rust_mut().get_mut().lyrics_events = None;
        }
        let event = match self.as_ref().get_ref().rust().backend_events.as_ref() {
            Some(receiver) => receiver.try_recv(),
            None => return,
        };
        match event {
            Ok(BackendEvent::Account {
                name,
                avatar_url,
                uid,
            }) => {
                self.as_mut().set_account_name(QString::from(name));
                self.as_mut().set_avatar_url(QString::from(avatar_url));
                self.as_mut().rust_mut().get_mut().user_id = uid;
            }
            Ok(BackendEvent::Search(results)) => {
                self.as_mut()
                    .set_search_results_json(QString::from(results));
                self.as_mut().set_loading(false);
            }
            Ok(BackendEvent::WaveTracks {
                session_id,
                batch_id,
                tracks,
                replace,
                autoplay,
            }) => {
                let mut first_new_index = None;
                let should_autoplay;
                {
                    let backend = self.as_mut().rust_mut().get_mut();
                    backend.wave_fetching = false;
                    if !replace && backend.wave_session_id != session_id {
                        return;
                    }
                    should_autoplay = autoplay || backend.wave_pending_autoplay;
                    backend.wave_pending_autoplay = false;
                    if replace {
                        backend.wave_session_id = session_id.clone();
                        backend.wave_active = true;
                        backend.wave_batch_id = batch_id.clone();
                        backend.wave_seen.clear();
                        backend.wave_batches.clear();
                        backend.wave_tracks.clear();
                        backend.queue.clear();
                        backend.queue_index = 0;
                    }
                    for track in tracks {
                        if backend.wave_seen.insert(track.id.clone()) {
                            backend
                                .wave_batches
                                .insert(track.id.clone(), batch_id.clone());
                            backend.wave_tracks.push(track.clone());
                            if first_new_index.is_none() {
                                first_new_index = Some(backend.queue.len());
                            }
                            backend.queue.push(track);
                        }
                    }
                }
                let wave_json = {
                    let backend = self.as_ref().get_ref().rust();
                    Value::Array(
                        backend
                            .wave_tracks
                            .iter()
                            .cloned()
                            .map(track_to_value)
                            .collect(),
                    )
                    .to_string()
                };
                self.as_mut()
                    .set_wave_results_json(QString::from(wave_json));
                if should_autoplay {
                    if let Some(index) = first_new_index {
                        let track_id = self.as_ref().get_ref().rust().queue[index].id.clone();
                        self.as_mut().rust_mut().get_mut().queue_index = index;
                        self.as_mut().set_loading(true);
                        self.as_mut().publish_queue();
                        self.as_mut().request_playback(track_id);
                    } else if !self.as_ref().get_ref().rust().wave_session_id.is_empty() {
                        self.as_mut().request_wave_batch(true);
                    }
                } else {
                    self.as_mut().publish_queue();
                }
            }
            Ok(BackendEvent::QueueLoaded(tracks)) => {
                {
                    let backend = self.as_mut().rust_mut().get_mut();
                    backend.queue = tracks;
                    backend.queue_index = 0;
                    backend.wave_active = false;
                }
                self.as_mut().publish_queue();
            }
            Ok(BackendEvent::QueueId(queue_id)) => {
                self.as_mut().rust_mut().get_mut().queue_id = queue_id;
            }
            Ok(BackendEvent::Collection(results)) => {
                self.as_mut().set_collection_json(QString::from(results));
                self.as_mut().set_loading(false);
            }
            Ok(BackendEvent::Playlists(results)) => {
                self.as_mut().set_playlists_json(QString::from(results));
            }
            Ok(BackendEvent::Album(results)) => {
                self.as_mut().set_album_json(QString::from(results));
                self.as_mut().set_loading(false);
            }
            Ok(BackendEvent::Artist(results)) => {
                self.as_mut().set_artist_json(QString::from(results));
                self.as_mut().set_loading(false);
            }
            Ok(BackendEvent::Playlist(results)) => {
                self.as_mut().set_playlist_json(QString::from(results));
                self.as_mut().set_loading(false);
            }
            Ok(BackendEvent::Feed(results)) => {
                self.as_mut().set_feed_json(QString::from(results));
                self.as_mut().set_loading(false);
            }
            Ok(BackendEvent::History(results)) => {
                self.as_mut().set_history_json(QString::from(results));
                self.as_mut().set_loading(false);
            }
            Ok(BackendEvent::LikedIds(ids)) => {
                self.as_mut().rust_mut().get_mut().liked_ids = ids.into_iter().collect();
                let current = self.as_ref().get_ref().rust().current_track_id.to_string();
                let liked = self.as_ref().get_ref().rust().liked_ids.contains(&current);
                self.as_mut().set_liked(liked);
            }
            Ok(BackendEvent::LikeState { track_id, liked }) => {
                if track_id == self.as_ref().get_ref().rust().current_track_id.to_string() {
                    self.as_mut().set_liked(liked);
                }
            }
            Ok(BackendEvent::Playback {
                track,
                response,
                autoplay,
            }) => {
                if self.as_ref().get_ref().rust().queue.is_empty() {
                    self.as_mut().rust_mut().get_mut().queue.push(track.clone());
                    self.as_mut().rust_mut().get_mut().queue_index = 0;
                }
                let cache = Arc::new(Mutex::new(StreamCache::default()));
                let result = (|| -> Result<(), String> {
                    // Stop the previous sink before replacing its stream owner.
                    if let Some(old_sink) = self.as_ref().get_ref().rust().audio_sink.as_ref() {
                        old_sink.stop();
                    }
                    self.as_mut().rust_mut().get_mut().audio_sink = None;
                    self.as_mut().rust_mut().get_mut().audio_stream = None;

                    let mut stream =
                        OutputStreamBuilder::open_default_stream().map_err(|error| {
                            eprintln!("[audio] failed to open default output: {error}");
                            error.to_string()
                        })?;
                    stream.log_on_drop(false);
                    let sink = Sink::connect_new(stream.mixer());
                    sink.set_volume(*self.as_ref().volume_percent() as f32 / 100.0);
                    let source = Decoder::builder()
                        .with_data(StreamingReader::new(response, cache.clone()))
                        .with_hint("mp3")
                        .with_seekable(true)
                        .build()
                        .map_err(|error| {
                            eprintln!("[audio] failed to decode stream: {error}");
                            error.to_string()
                        })?;
                    sink.append(AnalyzingSource::new(
                        source,
                        self.as_ref().get_ref().rust().audio_monitor.clone(),
                    ));
                    if autoplay {
                        sink.play();
                    } else {
                        sink.pause();
                    }
                    if autoplay {
                        eprintln!("[audio] playback started");
                    } else {
                        eprintln!("[audio] track prepared and paused");
                    }
                    self.as_mut().rust_mut().get_mut().audio_stream = Some(stream);
                    self.as_mut().rust_mut().get_mut().audio_sink = Some(sink);
                    self.as_mut().rust_mut().get_mut().stream_cache = Some(cache.clone());
                    self.as_mut().set_title(QString::from(track.title.clone()));
                    self.as_mut()
                        .set_artist(QString::from(track.artist.clone()));
                    let cover_url = if track.cover_source_url.is_empty() {
                        track.cover_url.clone()
                    } else {
                        track.cover_source_url.clone()
                    };
                    self.as_mut().set_cover_url(QString::from(cover_url));
                    self.as_mut()
                        .set_duration_ms(track.duration_ms.min(i32::MAX as i64) as i32);
                    self.as_mut().set_position_ms(0);
                    self.as_mut().set_playing(autoplay);
                    self.as_mut().set_loading(false);
                    self.as_mut()
                        .set_current_track_id(QString::from(track.id.clone()));
                    self.as_mut().set_lyric(QString::from(""));
                    let liked = self.as_ref().get_ref().rust().liked_ids.contains(&track.id);
                    self.as_mut().set_liked(liked);
                    self.as_mut().publish_queue();
                    Ok(())
                })();
                let playback_ok = result.is_ok();
                if let Err(error) = result {
                    eprintln!("[audio] {error}");
                    self.as_mut().set_error(QString::from(error));
                    self.as_mut().set_loading(false);
                }
                if autoplay && playback_ok {
                    let feedback = {
                        let backend = self.as_ref().get_ref().rust();
                        if backend.wave_active {
                            backend
                                .wave_batches
                                .get(&track.id)
                                .cloned()
                                .map(|batch_id| (backend.wave_session_id.clone(), batch_id))
                        } else {
                            None
                        }
                    };
                    if let (Some(token), Some((session_id, batch_id))) = (load_token(), feedback) {
                        spawn_wave_feedback(
                            token,
                            session_id,
                            batch_id,
                            "trackStarted",
                            track.id.clone(),
                            None,
                        );
                    }
                }
                {
                    let backend = self.as_mut().rust_mut().get_mut();
                    backend.current_album = track.album.clone();
                    backend.current_cover_source_url = track.cover_source_url.clone();
                    backend.current_duration_ms = track.duration_ms;
                    backend.max_position_ms = 0;
                    backend.scrobbled = false;
                }
                if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
                    handle.send(IntegrationCommand::NowPlaying {
                        title: track.title.clone(),
                        artist: track.artist.clone(),
                        album: track.album.clone(),
                        cover_url: external_cover_url(&track.cover_source_url),
                        duration_ms: track.duration_ms,
                        position_ms: 0,
                        playing: autoplay,
                    });
                }
                self.as_mut().load_lyrics(QString::from(track.id));
            }
            Ok(BackendEvent::Error(error)) => {
                eprintln!("[yandex/api] {error}");
                {
                    let backend = self.as_mut().rust_mut().get_mut();
                    backend.wave_fetching = false;
                    backend.wave_pending_autoplay = false;
                }
                self.as_mut().set_error(QString::from(error));
                self.as_mut().set_loading(false);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                eprintln!("[yandex/api] Background request stopped unexpectedly.");
                self.as_mut()
                    .set_error(QString::from("Background request stopped unexpectedly."));
                self.as_mut().set_loading(false);
            }
        }
    }

    fn seek(mut self: Pin<&mut Self>, position_ms: i32) {
        if position_ms < 0 {
            return;
        }
        if let Some(sink) = self.as_ref().get_ref().rust().audio_sink.as_ref() {
            if let Err(error) = sink.try_seek(Duration::from_millis(position_ms as u64)) {
                eprintln!("[audio] seek failed: {error}");
            }
        }
        self.as_mut().set_position_ms(position_ms);
        if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
            handle.send(IntegrationCommand::PlayingChanged {
                playing: *self.as_ref().playing(),
                position_ms: position_ms as i64,
            });
        }
    }

    fn set_volume(mut self: Pin<&mut Self>, volume_percent: i32) {
        let volume_percent = volume_percent.clamp(0, 100);
        self.as_mut().set_volume_percent(volume_percent);
        if let Some(sink) = self.as_ref().get_ref().rust().audio_sink.as_ref() {
            sink.set_volume(volume_percent as f32 / 100.0);
        }
        {
            let backend = self.as_mut().rust_mut().get_mut();
            backend.settings.volume_percent = volume_percent;
            backend.settings_dirty = true;
        }
        if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
            handle.send(IntegrationCommand::VolumeChanged(
                volume_percent as f64 / 100.0,
            ));
        }
    }

    fn toggle_play(mut self: Pin<&mut Self>) {
        let mut changed = None;
        if let Some(sink) = self.as_ref().get_ref().rust().audio_sink.as_ref() {
            if sink.is_paused() {
                sink.play();
                changed = Some(true);
            } else {
                sink.pause();
                changed = Some(false);
            }
        }
        if let Some(playing) = changed {
            self.as_mut().set_playing(playing);
            let position_ms = self
                .as_ref()
                .get_ref()
                .rust()
                .audio_sink
                .as_ref()
                .map(|sink| sink.get_pos().as_millis().min(i64::MAX as u128) as i64)
                .unwrap_or(0);
            if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
                handle.send(IntegrationCommand::PlayingChanged {
                    playing,
                    position_ms,
                });
            }
        }
    }

    fn toggle_like(mut self: Pin<&mut Self>) {
        let track_id = self.as_ref().get_ref().rust().current_track_id.to_string();
        if track_id.is_empty() {
            return;
        }
        let target = !*self.as_ref().liked();
        self.as_mut().set_liked(target);
        {
            let backend = self.as_mut().rust_mut().get_mut();
            if target {
                backend.liked_ids.insert(track_id.clone());
            } else {
                backend.liked_ids.remove(&track_id);
            }
        }
        let uid = self.as_ref().get_ref().rust().user_id.clone();
        let Some(token) = load_token() else {
            return;
        };
        if uid.is_empty() {
            return;
        }
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = YandexClient::new(token).and_then(|client| {
                if target {
                    client.add_likes(&uid, std::slice::from_ref(&track_id))
                } else {
                    client.remove_likes(&uid, std::slice::from_ref(&track_id))
                }
            });
            match result {
                Ok(()) => {
                    let _ = sender.send(BackendEvent::LikeState {
                        track_id,
                        liked: target,
                    });
                }
                Err(error) => eprintln!("[yandex/likes] {error}"),
            }
        });
    }

    fn toggle_shuffle(mut self: Pin<&mut Self>) {
        let value = !*self.as_ref().shuffle();
        self.as_mut().set_shuffle(value);
    }

    fn toggle_repeat(mut self: Pin<&mut Self>) {
        let value = !*self.as_ref().repeat();
        self.as_mut().set_repeat(value);
    }

    fn replay_cached(mut self: Pin<&mut Self>) -> bool {
        let bytes = self
            .as_ref()
            .get_ref()
            .rust()
            .stream_cache
            .as_ref()
            .and_then(|cache| cache.lock().ok())
            .and_then(|cache| cache.complete.then(|| cache.bytes.clone()));
        let Some(bytes) = bytes.filter(|bytes| !bytes.is_empty()) else {
            return false;
        };
        let Some(stream) = self.as_ref().get_ref().rust().audio_stream.as_ref() else {
            return false;
        };
        if let Some(old_sink) = self.as_ref().get_ref().rust().audio_sink.as_ref() {
            old_sink.stop();
        }
        let sink = Sink::connect_new(stream.mixer());
        sink.set_volume(*self.as_ref().volume_percent() as f32 / 100.0);
        let source = match Decoder::builder()
            .with_data(Cursor::new(bytes))
            .with_hint("mp3")
            .with_seekable(true)
            .build()
        {
            Ok(source) => source,
            Err(error) => {
                eprintln!("[audio] cached repeat decode failed: {error}");
                return false;
            }
        };
        sink.append(AnalyzingSource::new(
            source,
            self.as_ref().get_ref().rust().audio_monitor.clone(),
        ));
        sink.play();
        self.as_mut().rust_mut().get_mut().audio_sink = Some(sink);
        self.as_mut().set_position_ms(0);
        self.as_mut().set_playing(true);
        {
            let backend = self.as_mut().rust_mut().get_mut();
            backend.scrobbled = false;
            backend.max_position_ms = 0;
        }
        let title = self.as_ref().title().to_string();
        let artist = self.as_ref().artist().to_string();
        let album = self.as_ref().get_ref().rust().current_album.clone();
        let cover_url =
            external_cover_url(&self.as_ref().get_ref().rust().current_cover_source_url);
        let duration_ms = self.as_ref().get_ref().rust().current_duration_ms;
        if let Some(handle) = self.as_ref().get_ref().rust().integrations.as_ref() {
            handle.send(IntegrationCommand::NowPlaying {
                title,
                artist,
                album,
                cover_url,
                duration_ms,
                position_ms: 0,
                playing: true,
            });
        }
        eprintln!("[audio] replayed cached track");
        true
    }

    fn next(mut self: Pin<&mut Self>) {
        self.as_mut().advance(false);
    }

    fn advance(mut self: Pin<&mut Self>, finished: bool) {
        let (next, wave_active, current_index, current_track_id, session_id, batch_id, seconds) = {
            let backend = self.as_ref().get_ref().rust();
            let next = if backend.queue.is_empty() {
                None
            } else if *self.as_ref().repeat() {
                Some(backend.queue_index)
            } else if *self.as_ref().shuffle() {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.subsec_nanos() as usize)
                    .unwrap_or(0);
                Some(stamp % backend.queue.len())
            } else if backend.queue_index + 1 < backend.queue.len() {
                Some(backend.queue_index + 1)
            } else {
                None
            };
            let current_track_id = backend.current_track_id.to_string();
            let batch_id = backend
                .wave_batches
                .get(&current_track_id)
                .cloned()
                .unwrap_or_else(|| backend.wave_batch_id.clone());
            let seconds = backend
                .audio_sink
                .as_ref()
                .map(|sink| sink.get_pos().as_secs_f64())
                .unwrap_or(0.0);
            (
                next,
                backend.wave_active,
                backend.queue_index,
                current_track_id,
                backend.wave_session_id.clone(),
                batch_id,
                seconds,
            )
        };
        let advancing = match next {
            Some(index) => index != current_index,
            None => true,
        };
        if wave_active && advancing && !current_track_id.is_empty() && !batch_id.is_empty() {
            let event = if finished { "trackFinished" } else { "skip" };
            if let Some(token) = load_token() {
                spawn_wave_feedback(
                    token,
                    session_id,
                    batch_id,
                    event,
                    current_track_id,
                    Some(seconds),
                );
            }
        }
        if let Some(index) = next {
            let track_id = self.as_ref().get_ref().rust().queue[index].id.clone();
            self.as_mut().rust_mut().get_mut().queue_index = index;
            self.as_mut().publish_queue();
            self.as_mut().sync_queue_position();
            self.request_playback(track_id);
        } else if wave_active {
            self.as_mut().request_wave_batch(true);
        }
    }

    fn request_wave_batch(mut self: Pin<&mut Self>, autoplay: bool) {
        let (session_id, queue_ids, fetching) = {
            let backend = self.as_ref().get_ref().rust();
            let start = backend.queue.len().saturating_sub(WAVE_QUEUE_LIMIT);
            (
                backend.wave_session_id.clone(),
                backend.queue[start..]
                    .iter()
                    .map(|track| track.id.clone())
                    .collect::<Vec<_>>(),
                backend.wave_fetching,
            )
        };
        if session_id.is_empty() {
            return;
        }
        if fetching {
            if autoplay {
                self.as_mut().rust_mut().get_mut().wave_pending_autoplay = true;
            }
            return;
        }
        let Some(token) = load_token() else {
            return;
        };
        {
            let backend = self.as_mut().rust_mut().get_mut();
            backend.wave_fetching = true;
            if autoplay {
                backend.wave_pending_autoplay = true;
            }
        }
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = YandexClient::new(token)
                .and_then(|client| client.wave_next(&session_id, &queue_ids));
            match result {
                Ok(batch) if batch.tracks.is_empty() => {
                    let _ = sender.send(BackendEvent::Error(
                        "My Wave returned no more tracks.".into(),
                    ));
                }
                Ok(batch) => {
                    let _ = sender.send(BackendEvent::WaveTracks {
                        session_id: batch.session_id,
                        batch_id: batch.batch_id,
                        tracks: batch.tracks,
                        replace: false,
                        autoplay: false,
                    });
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn previous(mut self: Pin<&mut Self>) {
        let previous = {
            let backend = self.as_ref().get_ref().rust();
            if backend.queue.is_empty() {
                None
            } else if backend.queue_index > 0 {
                Some(backend.queue_index - 1)
            } else if *self.as_ref().repeat() {
                Some(backend.queue.len() - 1)
            } else {
                Some(0)
            }
        };
        if let Some(index) = previous {
            let track_id = self.as_ref().get_ref().rust().queue[index].id.clone();
            self.as_mut().rust_mut().get_mut().queue_index = index;
            self.as_mut().publish_queue();
            self.as_mut().sync_queue_position();
            self.request_playback(track_id);
        }
    }

    fn publish_queue(mut self: Pin<&mut Self>) {
        let (json_text, position) = {
            let backend = self.as_ref().get_ref().rust();
            let items: Vec<Value> = backend
                .queue
                .iter()
                .map(|track| {
                    json!({
                        "id": track.id,
                        "title": track.title,
                        "artist": track.artist,
                        "artistId": track.artist_id,
                        "album": track.album,
                        "albumId": track.album_id,
                        "coverUrl": track.cover_url,
                        "coverSourceUrl": track.cover_source_url,
                        "durationMs": track.duration_ms,
                    })
                })
                .collect();
            (
                Value::Array(items).to_string(),
                backend.queue_index.min(i32::MAX as usize) as i32,
            )
        };
        self.as_mut().set_queue_json(QString::from(json_text));
        self.as_mut().set_queue_position(position);
    }

    fn sync_queue_position(self: Pin<&mut Self>) {
        let (queue_id, index) = {
            let backend = self.as_ref().get_ref().rust();
            (backend.queue_id.clone(), backend.queue_index as i64)
        };
        if queue_id.is_empty() {
            return;
        }
        let Some(token) = load_token() else {
            return;
        };
        thread::spawn(move || {
            if let Ok(client) = YandexClient::new(token) {
                if let Err(error) = client.update_queue_position(&queue_id, index) {
                    eprintln!("[yandex/queue] {error}");
                }
            }
        });
    }

    fn jump_to(mut self: Pin<&mut Self>, index: i32) {
        if index < 0 {
            return;
        }
        let index = index as usize;
        let Some(track_id) = self
            .as_ref()
            .get_ref()
            .rust()
            .queue
            .get(index)
            .map(|track| track.id.clone())
        else {
            return;
        };
        self.as_mut().rust_mut().get_mut().queue_index = index;
        self.as_mut().publish_queue();
        self.as_mut().sync_queue_position();
        self.request_playback(track_id);
    }

    fn play_list(mut self: Pin<&mut Self>, tracks_json: QString, start_index: i32) {
        let tracks = tracks_from_json(&tracks_json.to_string());
        if tracks.is_empty() {
            return;
        }
        let index = if start_index < 0 {
            0
        } else {
            (start_index as usize).min(tracks.len() - 1)
        };
        let track_id = tracks[index].id.clone();
        {
            let backend = self.as_mut().rust_mut().get_mut();
            backend.queue = tracks;
            backend.queue_index = index;
            backend.wave_active = false;
            backend.wave_fetching = false;
            backend.wave_pending_autoplay = false;
            backend.wave_session_id.clear();
            backend.wave_batch_id.clear();
            backend.wave_seen.clear();
            backend.wave_batches.clear();
            backend.wave_tracks.clear();
        }
        self.as_mut().publish_queue();
        self.request_playback(track_id);
    }

    fn load_collection(mut self: Pin<&mut Self>) {
        let Some(token) = load_token() else {
            self.as_mut()
                .set_error(QString::from("Please sign in first."));
            return;
        };
        let uid = self.as_ref().get_ref().rust().user_id.clone();
        self.as_mut().set_loading(true);
        self.as_mut().set_error(QString::from(""));
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = YandexClient::new(token).and_then(|client| {
                let uid = if uid.is_empty() {
                    client.account_uid()?
                } else {
                    uid
                };
                client.liked_tracks(&uid)
            });
            match result {
                Ok(tracks) => {
                    let items = tracks_to_values(tracks);
                    let ids: Vec<String> = items
                        .iter()
                        .filter_map(|item| {
                            item.get("id").and_then(Value::as_str).map(str::to_owned)
                        })
                        .collect();
                    let _ = sender.send(BackendEvent::LikedIds(ids));
                    let _ = sender.send(BackendEvent::Collection(Value::Array(items).to_string()));
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn load_playlists(mut self: Pin<&mut Self>) {
        let Some(token) = load_token() else {
            return;
        };
        let uid = self.as_ref().get_ref().rust().user_id.clone();
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = YandexClient::new(token).and_then(|client| {
                let uid = if uid.is_empty() {
                    client.account_uid()?
                } else {
                    uid
                };
                client.playlists(&uid)
            });
            match result {
                Ok(playlists) => {
                    let items: Vec<Value> = playlists
                        .into_iter()
                        .map(|playlist| {
                            json!({
                                "uuid": playlist.uuid,
                                "kind": playlist.kind,
                                "title": playlist.title,
                                "trackCount": playlist.track_count,
                                "coverUrl": playlist.cover_url,
                            })
                        })
                        .collect();
                    let _ = sender.send(BackendEvent::Playlists(Value::Array(items).to_string()));
                }
                Err(error) => eprintln!("[yandex/playlists] {error}"),
            }
        });
    }

    fn open_album(mut self: Pin<&mut Self>, album_id: QString) {
        let album_id = album_id.to_string();
        let Some(token) = load_token() else {
            return;
        };
        self.as_mut().set_loading(true);
        self.as_mut().set_error(QString::from(""));
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = YandexClient::new(token).and_then(|client| client.album(&album_id));
            match result {
                Ok(detail) => {
                    let cover = detail.cover_url.clone();
                    let tracks = tracks_to_values(detail.tracks);
                    let payload = json!({
                        "id": detail.id,
                        "title": detail.title,
                        "artist": detail.artist,
                        "artistId": detail.artist_id,
                        "year": detail.year,
                        "durationMs": detail.duration_ms,
                        "coverUrl": cover,
                        "tracks": tracks,
                    });
                    let _ = sender.send(BackendEvent::Album(payload.to_string()));
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn open_artist(mut self: Pin<&mut Self>, artist_id: QString) {
        let artist_id = artist_id.to_string();
        let Some(token) = load_token() else {
            return;
        };
        self.as_mut().set_loading(true);
        self.as_mut().set_error(QString::from(""));
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = YandexClient::new(token).and_then(|client| client.artist(&artist_id));
            match result {
                Ok(detail) => {
                    let cover = detail.cover_url.clone();
                    let tracks = tracks_to_values(detail.tracks);
                    let payload = json!({
                        "id": detail.id,
                        "name": detail.name,
                        "description": detail.description,
                        "coverUrl": cover,
                        "tracks": tracks,
                    });
                    let _ = sender.send(BackendEvent::Artist(payload.to_string()));
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn open_playlist(mut self: Pin<&mut Self>, playlist_uuid: QString) {
        let playlist_uuid = playlist_uuid.to_string();
        let Some(token) = load_token() else {
            return;
        };
        self.as_mut().set_loading(true);
        self.as_mut().set_error(QString::from(""));
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result =
                YandexClient::new(token).and_then(|client| client.playlist(&playlist_uuid));
            match result {
                Ok(detail) => {
                    let cover = detail.cover_url.clone();
                    let tracks = tracks_to_values(detail.tracks);
                    let payload = json!({
                        "uuid": detail.uuid,
                        "kind": detail.kind,
                        "title": detail.title,
                        "owner": detail.owner,
                        "trackCount": detail.track_count,
                        "coverUrl": cover,
                        "tracks": tracks,
                    });
                    let _ = sender.send(BackendEvent::Playlist(payload.to_string()));
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn load_feed(mut self: Pin<&mut Self>) {
        let Some(token) = load_token() else {
            return;
        };
        self.as_mut().set_loading(true);
        self.as_mut().set_error(QString::from(""));
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = YandexClient::new(token).and_then(|client| client.feed_tracks());
            match result {
                Ok(tracks) => {
                    let items = tracks_to_values(tracks);
                    let _ = sender.send(BackendEvent::Feed(Value::Array(items).to_string()));
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn load_history(mut self: Pin<&mut Self>) {
        let Some(token) = load_token() else {
            return;
        };
        self.as_mut().set_loading(true);
        self.as_mut().set_error(QString::from(""));
        let sender = self.as_mut().event_sender();
        thread::spawn(move || {
            let result = YandexClient::new(token).and_then(|client| client.history_tracks());
            match result {
                Ok(tracks) => {
                    let items = tracks_to_values(tracks);
                    let _ = sender.send(BackendEvent::History(Value::Array(items).to_string()));
                }
                Err(error) => {
                    let _ = sender.send(BackendEvent::Error(error.to_string()));
                }
            }
        });
    }

    fn load_lyrics(mut self: Pin<&mut Self>, track_id: QString) {
        let track_id = track_id.to_string();
        if track_id.is_empty() {
            return;
        }
        let Some(token) = load_token() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.as_mut().rust_mut().get_mut().lyrics_events = Some(receiver);
        thread::spawn(move || {
            let result = YandexClient::new(token).and_then(|client| client.lyrics(&track_id));
            let text = match result {
                Ok(Some(text)) if !text.trim().is_empty() => text,
                _ => "Lyrics are not available for this track.".to_owned(),
            };
            let _ = sender.send(text);
        });
    }
}

fn save_token(token: &str) -> Result<(), String> {
    let token_path = token_path()?;
    let directory = token_path.parent().unwrap();
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    std::fs::write(token_path, token).map_err(|error| error.to_string())
}

fn token_path() -> Result<std::path::PathBuf, String> {
    Ok(dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| "Could not locate a writable data directory".to_string())?
        .join("exmusic")
        .join("token"))
}

fn load_token() -> Option<String> {
    token_path()
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
}
