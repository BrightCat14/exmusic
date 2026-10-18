use crate::yandex::YandexClient;
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};
use rustfft::{FftPlanner, num_complex::Complex};
use serde_json::{Value, json};
use std::collections::HashSet;
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
    SearchCovers(String),
    Wave(String),
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

fn track_to_value(track: crate::yandex::TrackItem) -> Value {
    json!({
        "id": track.id,
        "title": track.title,
        "artist": track.artist,
        "artistId": track.artist_id,
        "album": track.album,
        "albumId": track.album_id,
        "coverUrl": track.cover_url,
        "durationMs": track.duration_ms,
    })
}

const COVER_FETCH_LIMIT: usize = 80;
const COVER_WORKERS: usize = 8;

fn tracks_to_values(client: &YandexClient, tracks: Vec<crate::yandex::TrackItem>) -> Vec<Value> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut tracks = tracks;
    let count = tracks.len().min(COVER_FETCH_LIMIT);
    for track in tracks.iter_mut().skip(COVER_FETCH_LIMIT) {
        track.cover_url = String::new();
    }

    let mut covers: Vec<Mutex<Option<String>>> = (0..count).map(|_| Mutex::new(None)).collect();
    if count > 0 {
        let next = AtomicUsize::new(0);
        let workers = COVER_WORKERS.min(count);
        thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= count {
                            break;
                        }
                        let url = tracks[index].cover_url.clone();
                        if url.is_empty() {
                            continue;
                        }
                        let data = client.image_data_url(&url).unwrap_or_default();
                        *covers[index].lock().unwrap() = Some(data);
                    }
                });
            }
        });
    }

    tracks
        .into_iter()
        .enumerate()
        .map(|(index, mut track)| {
            if index < count {
                track.cover_url = covers[index].get_mut().unwrap().take().unwrap_or_default();
            }
            track_to_value(track)
        })
        .collect()
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
        #[qproperty(QString, login_state)]
        #[qproperty(f64, visual_bass)]
        #[qproperty(f64, visual_mid)]
        #[qproperty(f64, visual_high)]
        #[qproperty(f64, visual_beat)]
        #[qproperty(bool, visualizer_enabled)]
        #[qproperty(QString, user_code)]
        #[qproperty(QString, verification_url)]
        #[qproperty(QString, login_error)]
        type PlayerBackend = super::PlayerBackendRust;

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
    visual_bass: f64,
    visual_mid: f64,
    visual_high: f64,
    visual_beat: f64,
    visualizer_enabled: bool,
    login_state: QString,
    user_code: QString,
    verification_url: QString,
    login_error: QString,
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
    user_id: String,
    liked_ids: HashSet<String>,
    stream_cache: Option<Arc<Mutex<StreamCache>>>,
}

impl Default for PlayerBackendRust {
    fn default() -> Self {
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
            volume_percent: 100,
            visual_bass: 0.0,
            visual_mid: 0.0,
            visual_high: 0.0,
            visual_beat: 0.0,
            visualizer_enabled: true,
            login_state: QString::from("idle"),
            user_code: QString::from(""),
            verification_url: QString::from(""),
            login_error: QString::from(""),
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
                            format!("https://avatars.yandex.net/get-yapic/{avatar_id}/islands-200")
                        })
                        .and_then(|url| match client.image_data_url(&url) {
                            Ok(image) => Some(image),
                            Err(error) => {
                                eprintln!("[yandex/image] avatar: {error}");
                                None
                            }
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
                    let initial = (|| {
                        let active = client.active_queue().ok().flatten();
                        let track_id = if let Some(active) = active {
                            queue_id = active.queue_id;
                            match active.track_id {
                                Some(track_id) => track_id,
                                None => {
                                    let batch = client.start_wave().ok()?;
                                    initial_queue = batch.tracks;
                                    queue_id.clear();
                                    initial_queue.first().map(|track| track.id.clone())?
                                }
                            }
                        } else {
                            let batch = client.start_wave().ok()?;
                            initial_queue = batch.tracks;
                            initial_queue.first().map(|track| track.id.clone())?
                        };
                        let mut track = client.track(&track_id).ok()?;
                        let url = client.stream_url(&track_id).ok()?;
                        let response = client.open_stream(&url).ok()?;
                        if !track.cover_url.is_empty() {
                            track.cover_url =
                                client.image_data_url(&track.cover_url).unwrap_or_default();
                        }
                        Some((track, response))
                    })();
                    if !queue_id.is_empty() {
                        let _ = sender.send(BackendEvent::QueueId(queue_id.clone()));
                    }
                    if !initial_queue.is_empty() {
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
            let result = YandexClient::new(token)
                .and_then(|client| client.search(&text).map(|tracks| (client, tracks)));
            match result {
                Ok((client, tracks)) => {
                    let mut items: Vec<_> = tracks
                        .into_iter()
                        .map(|track| {
                            json!({
                                "id": track.id,
                                "title": track.title,
                                "artist": track.artist,
                                "artistId": track.artist_id,
                                "album": track.album,
                                "albumId": track.album_id,
                                "coverUrl": track.cover_url,
                                "durationMs": track.duration_ms,
                            })
                        })
                        .collect();
                    let _ = sender.send(BackendEvent::Search(
                        serde_json::Value::Array(items.clone()).to_string(),
                    ));
                    for item in &mut items {
                        let Some(url) = item.get("coverUrl").and_then(|value| value.as_str())
                        else {
                            continue;
                        };
                        if url.is_empty() {
                            continue;
                        }
                        match client.image_data_url(url) {
                            Ok(image) => item["coverUrl"] = image.into(),
                            Err(error) => eprintln!("[yandex/image] cover: {error}"),
                        }
                    }
                    let _ = sender.send(BackendEvent::SearchCovers(
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
                    let wave_tracks = batch.tracks;
                    let first_track_id = wave_tracks.first().map(|track| track.id.clone());
                    let _ = sender.send(BackendEvent::QueueLoaded(wave_tracks.clone()));
                    let items: Vec<_> = wave_tracks
                        .into_iter()
                        .map(|track| {
                            json!({
                                "id": track.id,
                                "title": track.title,
                                "artist": track.artist,
                                "artistId": track.artist_id,
                                "album": track.album,
                                "albumId": track.album_id,
                                "coverUrl": track.cover_url,
                                "durationMs": track.duration_ms,
                            })
                        })
                        .collect();
                    let _ = sender.send(BackendEvent::Wave(
                        serde_json::Value::Array(items).to_string(),
                    ));
                    if let Some(track_id) = first_track_id {
                        let result = (|| {
                            let mut track = client.track(&track_id)?;
                            let url = client.stream_url(&track_id)?;
                            let response = client.open_stream(&url)?;
                            if !track.cover_url.is_empty() {
                                track.cover_url =
                                    client.image_data_url(&track.cover_url).unwrap_or_default();
                            }
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
    }

    fn play_track(mut self: Pin<&mut Self>, track_id: QString) {
        let track_id = track_id.to_string();
        self.as_mut().rust_mut().get_mut().queue.clear();
        self.as_mut().rust_mut().get_mut().queue_index = 0;
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
                let mut track = client.track(&track_id)?;
                let url = client.stream_url(&track_id)?;
                let response = client.open_stream(&url)?;
                if !track.cover_url.is_empty() {
                    track.cover_url = match client.image_data_url(&track.cover_url) {
                        Ok(image) => image,
                        Err(error) => {
                            eprintln!("[yandex/image] cover {}: {error}", track.id);
                            String::new()
                        }
                    };
                }
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
        let position_ms = self
            .as_ref()
            .get_ref()
            .rust()
            .audio_sink
            .as_ref()
            .map(|sink| sink.get_pos().as_millis().min(i32::MAX as u128) as i32);
        if let Some(position_ms) = position_ms {
            self.as_mut().set_position_ms(position_ms);
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
            self.next();
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
            Ok(BackendEvent::SearchCovers(results)) => {
                self.as_mut()
                    .set_search_results_json(QString::from(results));
            }
            Ok(BackendEvent::Wave(results)) => {
                self.as_mut().set_wave_results_json(QString::from(results));
            }
            Ok(BackendEvent::QueueLoaded(tracks)) => {
                self.as_mut().rust_mut().get_mut().queue = tracks;
                self.as_mut().rust_mut().get_mut().queue_index = 0;
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
                    self.as_mut()
                        .set_cover_url(QString::from(track.cover_url.clone()));
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
                if let Err(error) = result {
                    eprintln!("[audio] {error}");
                    self.as_mut().set_error(QString::from(error));
                    self.as_mut().set_loading(false);
                }
                self.as_mut().load_lyrics(QString::from(track.id));
            }
            Ok(BackendEvent::Error(error)) => {
                eprintln!("[yandex/api] {error}");
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

    fn seek(self: Pin<&mut Self>, position_ms: i32) {
        if position_ms < 0 {
            return;
        }
        if let Some(sink) = self.as_ref().get_ref().rust().audio_sink.as_ref() {
            if let Err(error) = sink.try_seek(Duration::from_millis(position_ms as u64)) {
                eprintln!("[audio] seek failed: {error}");
            }
        }
    }

    fn set_volume(mut self: Pin<&mut Self>, volume_percent: i32) {
        let volume_percent = volume_percent.clamp(0, 100);
        self.as_mut().set_volume_percent(volume_percent);
        if let Some(sink) = self.as_ref().get_ref().rust().audio_sink.as_ref() {
            sink.set_volume(volume_percent as f32 / 100.0);
        }
    }

    fn toggle_play(mut self: Pin<&mut Self>) {
        if let Some(sink) = self.as_ref().get_ref().rust().audio_sink.as_ref() {
            if sink.is_paused() {
                sink.play();
                self.as_mut().set_playing(true);
            } else {
                sink.pause();
                self.as_mut().set_playing(false);
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
        let cover_url = self.as_ref().cover_url().clone();
        self.as_mut().set_cover_url(QString::from(""));
        self.as_mut().set_cover_url(cover_url);
        self.as_mut().set_playing(true);
        eprintln!("[audio] replayed cached track");
        true
    }

    fn next(mut self: Pin<&mut Self>) {
        let next = {
            let backend = self.as_ref().get_ref().rust();
            if backend.queue.is_empty() {
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
            } else if *self.as_ref().repeat() {
                Some(0)
            } else {
                None
            }
        };
        if let Some(index) = next {
            let track_id = self.as_ref().get_ref().rust().queue[index].id.clone();
            self.as_mut().rust_mut().get_mut().queue_index = index;
            self.as_mut().publish_queue();
            self.as_mut().sync_queue_position();
            self.request_playback(track_id);
        }
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
                let tracks = client.liked_tracks(&uid)?;
                Ok::<_, crate::yandex::Error>(tracks_to_values(&client, tracks))
            });
            match result {
                Ok(items) => {
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
                client.playlists(&uid).map(|playlists| (client, playlists))
            });
            match result {
                Ok((client, playlists)) => {
                    let items: Vec<Value> = playlists
                        .into_iter()
                        .map(|playlist| {
                            let cover = if playlist.cover_url.is_empty() {
                                String::new()
                            } else {
                                client
                                    .image_data_url(&playlist.cover_url)
                                    .unwrap_or_default()
                            };
                            json!({
                                "uuid": playlist.uuid,
                                "kind": playlist.kind,
                                "title": playlist.title,
                                "trackCount": playlist.track_count,
                                "coverUrl": cover,
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
            let result = YandexClient::new(token).and_then(|client| {
                let detail = client.album(&album_id)?;
                Ok::<_, crate::yandex::Error>((client, detail))
            });
            match result {
                Ok((client, detail)) => {
                    let cover = if detail.cover_url.is_empty() {
                        String::new()
                    } else {
                        client.image_data_url(&detail.cover_url).unwrap_or_default()
                    };
                    let tracks = tracks_to_values(&client, detail.tracks);
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
            let result = YandexClient::new(token).and_then(|client| {
                let detail = client.artist(&artist_id)?;
                Ok::<_, crate::yandex::Error>((client, detail))
            });
            match result {
                Ok((client, detail)) => {
                    let cover = if detail.cover_url.is_empty() {
                        String::new()
                    } else {
                        client.image_data_url(&detail.cover_url).unwrap_or_default()
                    };
                    let tracks = tracks_to_values(&client, detail.tracks);
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
            let result = YandexClient::new(token).and_then(|client| {
                let detail = client.playlist(&playlist_uuid)?;
                Ok::<_, crate::yandex::Error>((client, detail))
            });
            match result {
                Ok((client, detail)) => {
                    let cover = if detail.cover_url.is_empty() {
                        String::new()
                    } else {
                        client.image_data_url(&detail.cover_url).unwrap_or_default()
                    };
                    let tracks = tracks_to_values(&client, detail.tracks);
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
            let result = YandexClient::new(token).and_then(|client| {
                let tracks = client.feed_tracks()?;
                Ok::<_, crate::yandex::Error>(tracks_to_values(&client, tracks))
            });
            match result {
                Ok(items) => {
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
            let result = YandexClient::new(token).and_then(|client| {
                let tracks = client.history_tracks()?;
                Ok::<_, crate::yandex::Error>(tracks_to_values(&client, tracks))
            });
            match result {
                Ok(items) => {
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
