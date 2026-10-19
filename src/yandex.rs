use base64::Engine;
use md5::{Digest, Md5};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

const OAUTH_URL: &str = "https://oauth.yandex.ru";
const API_URL: &str = "https://api.music.yandex.net";
pub const DEFAULT_CLIENT_ID: &str = "23cabbbdc6cd418abb4b39c32c41195d";
pub const DEFAULT_CLIENT_SECRET: &str = "53bc75238f0c4d08a118e51fe9203300";

#[derive(Debug)]
pub enum Error {
    Request(reqwest::Error),
    Api(String),
    InvalidResponse(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(error) => write!(f, "network error: {error}"),
            Self::Api(error) | Self::InvalidResponse(error) => f.write_str(error),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct DeviceCode {
    #[serde(rename = "device_code", alias = "deviceCode")]
    pub device_code: String,
    #[serde(rename = "user_code", alias = "userCode")]
    pub user_code: String,
    #[serde(rename = "verification_url", alias = "verificationUrl")]
    pub verification_url: String,
    #[serde(rename = "expires_in", alias = "expiresIn")]
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
pub struct OAuthToken {
    #[serde(rename = "access_token", alias = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refresh_token", alias = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "expires_in", alias = "expiresIn")]
    pub expires_in: Option<u64>,
    #[serde(rename = "token_type", alias = "tokenType")]
    pub token_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrackItem {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: String,
    pub album: String,
    pub album_id: String,
    pub cover_url: String,
    pub cover_source_url: String,
    pub duration_ms: i64,
}

pub struct WaveBatch {
    pub session_id: String,
    pub batch_id: String,
    pub tracks: Vec<TrackItem>,
}

#[derive(Debug, Clone)]
pub struct AlbumDetail {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: String,
    pub year: i64,
    pub cover_url: String,
    pub duration_ms: i64,
    pub tracks: Vec<TrackItem>,
}

#[derive(Debug, Clone)]
pub struct ArtistDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cover_url: String,
    pub tracks: Vec<TrackItem>,
}

pub struct ActiveQueue {
    pub queue_id: String,
    pub track_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlaylistSummary {
    pub uuid: String,
    pub kind: String,
    pub title: String,
    pub track_count: i64,
    pub cover_url: String,
}

#[derive(Debug, Clone)]
pub struct PlaylistDetail {
    pub uuid: String,
    pub kind: String,
    pub title: String,
    pub owner: String,
    pub track_count: i64,
    pub cover_url: String,
    pub tracks: Vec<TrackItem>,
}

#[derive(Clone)]
pub struct YandexClient {
    http: Client,
    token: String,
}

impl YandexClient {
    pub fn new(token: impl Into<String>) -> Result<Self, Error> {
        let http = Client::builder()
            .user_agent("EXmusic/0.1")
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(Error::Request)?;
        Ok(Self {
            http,
            token: token.into(),
        })
    }

    pub fn request_device_code(&self, client_id: &str) -> Result<DeviceCode, Error> {
        let form = [
            ("device_name", "EXmusic"),
            (
                "client_id",
                if client_id.is_empty() {
                    DEFAULT_CLIENT_ID
                } else {
                    client_id
                },
            ),
        ];
        self.http
            .post(format!("{OAUTH_URL}/device/code"))
            .form(&form)
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json::<DeviceCode>()
            .map_err(Error::Request)
    }

    pub fn poll_device_token(
        &self,
        device_code: &str,
        client_id: &str,
        client_secret: &str,
    ) -> Result<OAuthToken, Error> {
        let form = [
            ("grant_type", "device_code"),
            ("code", device_code),
            (
                "client_id",
                if client_id.is_empty() {
                    DEFAULT_CLIENT_ID
                } else {
                    client_id
                },
            ),
            (
                "client_secret",
                if client_secret.is_empty() {
                    DEFAULT_CLIENT_SECRET
                } else {
                    client_secret
                },
            ),
        ];
        let response = self
            .http
            .post(format!("{OAUTH_URL}/token"))
            .form(&form)
            .send()
            .map_err(Error::Request)?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(Error::Api(format!("OAuth returned {status}: {body}")));
        }
        response.json::<OAuthToken>().map_err(Error::Request)
    }

    pub fn account_status(&self) -> Result<serde_json::Value, Error> {
        let response: Value = self
            .http
            .get(format!("{API_URL}/account/status"))
            .header("Authorization", format!("OAuth {}", self.token))
            .header("X-Yandex-Music-Client", "EXmusic/0.1")
            .header("Accept-Language", "en")
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;
        Ok(result_value(response))
    }

    pub fn account_uid(&self) -> Result<String, Error> {
        let status = self.account_status()?;
        Ok(status
            .get("account")
            .and_then(|account| account.get("uid"))
            .map(id_string)
            .unwrap_or_default())
    }

    pub fn search(&self, text: &str) -> Result<Vec<TrackItem>, Error> {
        let response: Value = self
            .http
            .get(format!("{API_URL}/search"))
            .query(&[
                ("text", text),
                ("type", "all"),
                ("page", "0"),
                ("playlist-in-best", "true"),
            ])
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;

        let result = result_value(response);
        let tracks = result
            .get("tracks")
            .and_then(|tracks| tracks.get("results").or(Some(tracks)))
            .and_then(Value::as_array)
            .ok_or_else(|| Error::InvalidResponse("search response contained no tracks".into()))?;
        Ok(tracks.iter().filter_map(track_item).collect())
    }

    pub fn start_wave(&self) -> Result<WaveBatch, Error> {
        self.start_wave_with_seed("user:onyourwave")
    }

    pub fn start_wave_with_seed(&self, seed: &str) -> Result<WaveBatch, Error> {
        let seeds: Vec<&str> = seed.split(',').filter(|seed| !seed.is_empty()).collect();
        let response: Value = self
            .http
            .post(format!("{API_URL}/rotor/session/new"))
            .json(&serde_json::json!({
                "seeds": seeds,
                "includeTracksInResponse": true,
                "interactive": true
            }))
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;
        let result = result_value(response);
        let session_id = result
            .get("radioSessionId")
            .or_else(|| result.get("radio_session_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| Error::InvalidResponse("wave session id was missing".into()))?
            .to_owned();
        let batch_id = result
            .get("batchId")
            .or_else(|| result.get("batch_id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let tracks: Vec<TrackItem> = result
            .get("sequence")
            .and_then(Value::as_array)
            .map(|sequence| {
                sequence
                    .iter()
                    .filter_map(|item| item.get("track").or(Some(item)))
                    .filter_map(track_item)
                    .collect()
            })
            .unwrap_or_default();
        if tracks.is_empty() {
            return Err(Error::InvalidResponse("wave returned no tracks".into()));
        }
        eprintln!("[yandex/wave] session {session_id} batch {batch_id}");
        Ok(WaveBatch {
            session_id,
            batch_id,
            tracks,
        })
    }

    pub fn wave_next(&self, session_id: &str, queue: &[String]) -> Result<WaveBatch, Error> {
        let response: Value = self
            .http
            .post(format!("{API_URL}/rotor/session/{session_id}/tracks"))
            .json(&serde_json::json!({ "queue": queue }))
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;
        let result = result_value(response);
        let batch_id = result
            .get("batchId")
            .or_else(|| result.get("batch_id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let tracks: Vec<TrackItem> = result
            .get("sequence")
            .and_then(Value::as_array)
            .map(|sequence| {
                sequence
                    .iter()
                    .filter_map(|item| item.get("track").or(Some(item)))
                    .filter_map(track_item)
                    .collect()
            })
            .unwrap_or_default();
        Ok(WaveBatch {
            session_id: session_id.to_owned(),
            batch_id,
            tracks,
        })
    }

    pub fn wave_feedback(
        &self,
        session_id: &str,
        event_type: &str,
        batch_id: &str,
        track_id: Option<&str>,
        total_played_seconds: Option<f64>,
    ) -> Result<(), Error> {
        let mut event = serde_json::json!({
            "type": event_type,
            "timestamp": timestamp_now_iso(),
        });
        if let Some(track_id) = track_id.filter(|track_id| !track_id.is_empty()) {
            event["trackId"] = Value::String(track_id.to_owned());
        }
        if let Some(seconds) = total_played_seconds {
            event["totalPlayedSeconds"] = serde_json::json!(seconds);
        }
        let response = self
            .http
            .post(format!("{API_URL}/rotor/session/{session_id}/feedback"))
            .json(&serde_json::json!({
                "event": event,
                "batchId": if batch_id.is_empty() {
                    Value::Null
                } else {
                    Value::String(batch_id.to_owned())
                },
            }))
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(Error::Api(format!("wave feedback returned {status}: {body}")));
        }
        Ok(())
    }

    pub fn play_audio(&self, track_id: &str) -> Result<(), Error> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let response = self
            .http
            .post(format!("{API_URL}/play-audio"))
            .form(&[
                ("track-id", track_id),
                ("from", "external"),
                ("timestamp", timestamp.to_string().as_str()),
                ("total-played-seconds", "0"),
                ("end-position-seconds", "0"),
            ])
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(Error::Api(format!("play-audio returned {status}: {body}")));
        }
        Ok(())
    }

    pub fn create_playlist(&self, uid: &str, title: &str) -> Result<PlaylistSummary, Error> {
        let response = self
            .http
            .post(format!("{API_URL}/users/{uid}/playlists/create"))
            .form(&[("title", title), ("visibility", "private")])
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;
        let result = result_value(response);
        let kind = result
            .get("kind")
            .and_then(Value::as_i64)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let playlist_uuid = result
            .get("playlistUuid")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{uid}:{kind}"));
        let cover_url = playlist_cover(&result);
        Ok(PlaylistSummary {
            uuid: playlist_uuid,
            kind,
            title: title.to_owned(),
            track_count: 0,
            cover_url,
        })
    }

    pub fn delete_playlist(&self, uid: &str, kind: &str) -> Result<(), Error> {
        let response = self
            .http
            .post(format!("{API_URL}/users/{uid}/playlists/{kind}/delete"))
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(Error::Api(format!("playlist delete returned {status}: {body}")));
        }
        Ok(())
    }

    pub fn landing3(&self) -> Result<Value, Error> {
        self.get_json(&format!("{API_URL}/landing3"))
    }

    pub fn landing3_chart(&self) -> Result<Value, Error> {
        self.get_json(&format!("{API_URL}/landing3/chart"))
    }

    pub fn landing3_new_releases(&self) -> Result<Value, Error> {
        self.get_json(&format!("{API_URL}/landing3/new-releases"))
    }

    pub fn landing3_new_playlists(&self) -> Result<Value, Error> {
        self.get_json(&format!("{API_URL}/landing3/new-playlists"))
    }

    pub fn track_similar(&self, track_id: &str) -> Result<Vec<TrackItem>, Error> {
        let response = self.get_json(&format!("{API_URL}/tracks/{track_id}/similar"))?;
        let result = result_value(response);
        let tracks = result
            .get("tracks")
            .or_else(|| result.get("similar"))
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(track_item).collect())
            .unwrap_or_default();
        Ok(tracks)
    }

    pub fn active_queue(&self) -> Result<Option<ActiveQueue>, Error> {
        let response: Value = self
            .http
            .get(format!("{API_URL}/queues"))
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;
        let result = result_value(response);
        let queue_id = result
            .get("queues")
            .and_then(Value::as_array)
            .and_then(|queues| queues.first())
            .and_then(|queue| queue.get("id"))
            .and_then(Value::as_str);
        let Some(queue_id) = queue_id else {
            return Ok(None);
        };
        let response: Value = self
            .http
            .get(format!("{API_URL}/queues/{queue_id}"))
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;
        let result = result_value(response);
        let index = result
            .get("currentIndex")
            .or_else(|| result.get("current_index"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let track_id = result
            .get("tracks")
            .and_then(Value::as_array)
            .and_then(|tracks| tracks.get(index))
            .and_then(|track| track.get("id").or_else(|| track.get("trackId")))
            .map(id_string);
        Ok(Some(ActiveQueue {
            queue_id: queue_id.to_owned(),
            track_id,
        }))
    }

    pub fn liked_tracks(&self, uid: &str) -> Result<Vec<TrackItem>, Error> {
        let response = self.get_json(&format!("{API_URL}/users/{uid}/likes/tracks"))?;
        let result = result_value(response);
        let shorts = result
            .get("library")
            .and_then(|library| library.get("tracks"))
            .or_else(|| result.get("tracks"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let ids: Vec<String> = shorts
            .iter()
            .filter_map(|item| item.get("id").map(id_string))
            .filter(|id| !id.is_empty())
            .collect();
        self.tracks_by_ids(&ids)
    }

    fn tracks_by_ids(&self, ids: &[String]) -> Result<Vec<TrackItem>, Error> {
        let mut tracks = Vec::new();
        for chunk in ids.chunks(100) {
            if chunk.is_empty() {
                continue;
            }
            let joined = chunk.join(",");
            let response = self.post_form_json(
                &format!("{API_URL}/tracks"),
                &[("track-ids", joined.as_str())],
            )?;
            let result = result_value(response);
            if let Some(items) = result.as_array() {
                tracks.extend(items.iter().filter_map(track_item));
            }
        }
        Ok(tracks)
    }

    pub fn add_likes(&self, uid: &str, track_ids: &[String]) -> Result<(), Error> {
        self.like_request(uid, track_ids, "add-multiple")
    }

    pub fn remove_likes(&self, uid: &str, track_ids: &[String]) -> Result<(), Error> {
        self.like_request(uid, track_ids, "remove")
    }

    fn like_request(&self, uid: &str, track_ids: &[String], action: &str) -> Result<(), Error> {
        let ids = track_ids.join(",");
        let response = self.post_form_json(
            &format!("{API_URL}/users/{uid}/likes/tracks/{action}"),
            &[("track-ids", ids.as_str())],
        )?;
        let result = result_value(response);
        if result.as_bool().unwrap_or(true) {
            Ok(())
        } else {
            Err(Error::Api("the like request was rejected".into()))
        }
    }

    pub fn album(&self, album_id: &str) -> Result<AlbumDetail, Error> {
        let response = self.get_json(&format!("{API_URL}/albums/{album_id}/with-tracks"))?;
        let result = result_value(response);
        let artist = result
            .get("artists")
            .and_then(Value::as_array)
            .and_then(|artists| artists.first());
        let tracks = result
            .get("volumes")
            .and_then(Value::as_array)
            .map(|volumes| {
                volumes
                    .iter()
                    .filter_map(Value::as_array)
                    .flatten()
                    .filter_map(track_item)
                    .collect()
            })
            .unwrap_or_default();
        Ok(AlbumDetail {
            id: album_id.to_owned(),
            title: string_field(&result, "title"),
            artist: artist
                .and_then(|artist| artist.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            artist_id: artist
                .and_then(|artist| artist.get("id"))
                .map(id_string)
                .unwrap_or_default(),
            year: result.get("year").and_then(Value::as_i64).unwrap_or(0),
            cover_url: cover_url_field(&result),
            duration_ms: result
                .get("durationMs")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            tracks,
        })
    }

    pub fn artist(&self, artist_id: &str) -> Result<ArtistDetail, Error> {
        let info_response = self.get_json(&format!("{API_URL}/artists/{artist_id}/info"))?;
        let info = result_value(info_response);
        let artist = info.get("artist").cloned().unwrap_or(Value::Null);
        let cover_url = {
            let primary = cover_url_field(&artist);
            if !primary.is_empty() {
                primary
            } else {
                info.get("covers")
                    .and_then(Value::as_array)
                    .and_then(|covers| covers.first())
                    .map(cover_url_field)
                    .unwrap_or_default()
            }
        };
        let tracks_response = self.get_json(&format!(
            "{API_URL}/artists/{artist_id}/tracks?page=0&page-size=100"
        ))?;
        let tracks = result_value(tracks_response)
            .get("tracks")
            .and_then(Value::as_array)
            .map(|tracks| tracks.iter().filter_map(track_item).collect())
            .unwrap_or_default();
        let description = {
            let direct = string_field(&info, "description");
            if !direct.is_empty() {
                direct
            } else {
                artist
                    .get("description")
                    .and_then(|description| description.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned()
            }
        };
        Ok(ArtistDetail {
            id: artist_id.to_owned(),
            name: string_field(&artist, "name"),
            description,
            cover_url,
            tracks,
        })
    }

    pub fn playlists(&self, uid: &str) -> Result<Vec<PlaylistSummary>, Error> {
        let response = self.get_json(&format!("{API_URL}/users/{uid}/playlists/list"))?;
        let result = result_value(response);
        let list = result.as_array().cloned().unwrap_or_default();
        Ok(list
            .iter()
            .filter_map(|playlist| {
                let kind = playlist.get("kind").and_then(Value::as_i64)?;
                let uuid = playlist
                    .get("playlistUuid")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{uid}:{kind}"));
                Some(PlaylistSummary {
                    uuid,
                    kind: kind.to_string(),
                    title: string_field(playlist, "title"),
                    track_count: playlist
                        .get("trackCount")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    cover_url: playlist_cover(playlist),
                })
            })
            .collect())
    }

    pub fn playlist(&self, uuid: &str) -> Result<PlaylistDetail, Error> {
        let response = self.get_json(&format!("{API_URL}/playlist/{uuid}"))?;
        let result = result_value(response);
        let playlist = result
            .as_array()
            .and_then(|items| items.first())
            .cloned()
            .unwrap_or(result);
        let tracks = playlist
            .get("tracks")
            .and_then(Value::as_array)
            .map(|tracks| tracks.iter().filter_map(track_item).collect())
            .unwrap_or_default();
        let owner = playlist
            .get("owner")
            .and_then(|owner| {
                owner
                    .get("name")
                    .or_else(|| owner.get("displayName"))
                    .or_else(|| owner.get("login"))
            })
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        Ok(PlaylistDetail {
            uuid: playlist
                .get("playlistUuid")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| uuid.to_owned()),
            kind: playlist.get("kind").map(id_string).unwrap_or_default(),
            title: string_field(&playlist, "title"),
            owner,
            track_count: playlist
                .get("trackCount")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            cover_url: playlist_cover(&playlist),
            tracks,
        })
    }

    pub fn lyrics(&self, track_id: &str) -> Result<Option<String>, Error> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let numeric: String = track_id
            .split(':')
            .next()
            .unwrap_or(track_id)
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        if numeric.is_empty() {
            return Ok(None);
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let mut mac = Hmac::<Sha256>::new_from_slice(b"p93jhgh689SBReK6ghtw62")
            .map_err(|error| Error::InvalidResponse(error.to_string()))?;
        mac.update(format!("{numeric}{timestamp}").as_bytes());
        let sign = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
        let timestamp_text = timestamp.to_string();
        let mut headers = self.api_headers();
        headers.insert(
            "X-Yandex-Music-Client",
            "YandexMusicAndroid/24023621".parse().unwrap(),
        );
        let response: Value = self
            .http
            .get(format!("{API_URL}/tracks/{track_id}/lyrics"))
            .query(&[
                ("format", "LRC"),
                ("timeStamp", timestamp_text.as_str()),
                ("sign", sign.as_str()),
            ])
            .headers(headers)
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;
        let result = result_value(response);
        let Some(url) = result.get("downloadUrl").and_then(Value::as_str) else {
            return Ok(None);
        };
        if url.is_empty() {
            return Ok(None);
        }
        let text = self
            .http
            .get(url)
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .text()
            .map_err(Error::Request)?;
        Ok(Some(strip_lrc(&text)))
    }

    pub fn feed_tracks(&self) -> Result<Vec<TrackItem>, Error> {
        let response = self.get_json(&format!("{API_URL}/feed"))?;
        let result = result_value(response);
        let mut tracks = Vec::new();
        if let Some(days) = result.get("days").and_then(Value::as_array) {
            for day in days {
                if let Some(day_tracks) = day.get("tracksToPlay").and_then(Value::as_array) {
                    tracks.extend(day_tracks.iter().filter_map(track_item));
                }
            }
        }
        if tracks.is_empty() {
            if let Some(generated) = result.get("generatedPlaylists").and_then(Value::as_array) {
                for playlist in generated {
                    if let Some(items) = playlist
                        .get("data")
                        .and_then(|data| data.get("tracks"))
                        .and_then(Value::as_array)
                    {
                        tracks.extend(items.iter().filter_map(track_item));
                    }
                }
            }
        }
        Ok(dedupe_tracks(tracks))
    }

    pub fn history_tracks(&self) -> Result<Vec<TrackItem>, Error> {
        let response = self.get_json(&format!("{API_URL}/music-history?fullModelsCount=50"))?;
        let result = result_value(response);
        let mut ids = Vec::new();
        if let Some(tabs) = result.get("historyTabs").and_then(Value::as_array) {
            for tab in tabs {
                if let Some(groups) = tab.get("items").and_then(Value::as_array) {
                    for group in groups {
                        if let Some(items) = group.get("tracks").and_then(Value::as_array) {
                            for item in items {
                                let data = item.get("data").unwrap_or(item);
                                if let Some(model) = data.get("fullModel").and_then(track_item) {
                                    ids.push(model.id);
                                    continue;
                                }
                                if let Some(track_id) = data
                                    .get("itemId")
                                    .and_then(|item_id| item_id.get("trackId"))
                                {
                                    ids.push(id_string(track_id));
                                }
                            }
                        }
                    }
                }
            }
        }
        ids.retain(|id| !id.is_empty() && id != "0");
        let mut tracks = self.tracks_by_ids(&ids)?;
        let mut seen = std::collections::HashSet::new();
        tracks.retain(|track| seen.insert(track.id.clone()));
        Ok(tracks)
    }

    pub fn update_queue_position(&self, queue_id: &str, index: i64) -> Result<(), Error> {
        self.post_form_json(
            &format!("{API_URL}/queues/{queue_id}/update-position?currentIndex={index}"),
            &[("isInteractive", "true")],
        )?;
        Ok(())
    }

    pub fn track(&self, track_id: &str) -> Result<TrackItem, Error> {
        let response: Value = self
            .http
            .post(format!("{API_URL}/tracks"))
            .form(&[("track-ids", track_id), ("with-positions", "true")])
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;
        result_value(response)
            .as_array()
            .and_then(|tracks| tracks.first())
            .and_then(track_item)
            .ok_or_else(|| Error::InvalidResponse("track was not found".into()))
    }

    pub fn stream_url(&self, track_id: &str) -> Result<String, Error> {
        eprintln!("[yandex/playback] requesting download URL for track {track_id}");
        let response: Value = self
            .http
            .get(format!("{API_URL}/tracks/{track_id}/download-info"))
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;
        let info_url = result_value(response)
            .as_array()
            .and_then(|variants| {
                variants
                    .iter()
                    .filter_map(|variant| {
                        let url = variant.get("downloadInfoUrl")?.as_str()?;
                        let bitrate = variant
                            .get("bitrateInKbps")
                            .and_then(Value::as_i64)
                            .unwrap_or_default();
                        Some((bitrate, url))
                    })
                    .max_by_key(|(bitrate, _)| *bitrate)
                    .map(|(_, url)| url)
            })
            .map(str::to_owned)
            .ok_or_else(|| Error::InvalidResponse("download info URL was missing".into()))?;
        let xml = self
            .http
            .get(&info_url)
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .text()
            .map_err(Error::Request)?;
        let host = xml_tag(&xml, "host")?;
        let path = xml_tag(&xml, "path")?;
        let timestamp = xml_tag(&xml, "ts")?;
        let salt = xml_tag(&xml, "s")?;
        let mut digest = Md5::new();
        digest.update(format!("XGRlBW9FXlekgbPrRHuSiA{}{}", &path[1..], salt));
        let signature = format!("{:x}", digest.finalize());
        let url = format!("https://{host}/get-mp3/{signature}/{timestamp}{path}");
        eprintln!("[yandex/playback] download URL resolved for track {track_id}");
        Ok(url)
        /*
        let codecs = "flac,flac-mp4,aac-mp4,he-aac-mp4,aac,he-aac,mp3";
        let signing_codecs = codecs.replace(',', "");
        let quality = "hq";
        let transport = "raw";
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| Error::InvalidResponse(error.to_string()))?
            .as_secs();
        let message = format!("{timestamp}{track_id}{quality}{signing_codecs}{transport}");
        let mut mac = Hmac::<Sha256>::new_from_slice(b"kzqU4XhfCaY6B6JTHODeq5")
            .map_err(|error| Error::InvalidResponse(error.to_string()))?;
        mac.update(message.as_bytes());
        let mut signature =
            base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
        signature.pop();

        let response: Value = self
            .http
            .get(format!("{API_URL}/get-file-info"))
            .query(&[
                ("ts", timestamp.to_string()),
                ("trackId", track_id.to_string()),
                ("quality", quality.to_string()),
                ("codecs", codecs.to_string()),
                ("transports", transport.to_string()),
                ("sign", signature),
            ])
            .headers(self.api_headers())
            .header(
                "X-Yandex-Music-Client",
                "YandexMusicDesktopAppWindows/5.95.0",
            )
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)?;
        result_value(response)
            .get("downloadInfo")
            .and_then(|info| info.get("url"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| Error::InvalidResponse("stream URL was missing".into()))
        */
    }

    pub fn open_stream(&self, url: &str) -> Result<reqwest::blocking::Response, Error> {
        let response = self
            .http
            .get(url)
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?;
        eprintln!("[yandex/playback] stream connection opened");
        Ok(response)
    }

    fn get_json(&self, url: &str) -> Result<Value, Error> {
        self.http
            .get(url)
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)
    }

    fn post_form_json(&self, url: &str, form: &[(&str, &str)]) -> Result<Value, Error> {
        self.http
            .post(url)
            .form(form)
            .headers(self.api_headers())
            .send()
            .map_err(Error::Request)?
            .error_for_status()
            .map_err(Error::Request)?
            .json()
            .map_err(Error::Request)
    }

    fn api_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Authorization",
            format!("OAuth {}", self.token).parse().unwrap(),
        );
        headers.insert("X-Yandex-Music-Client", "EXmusic/0.1".parse().unwrap());
        headers.insert("Accept-Language", "en".parse().unwrap());
        headers
    }
}

fn result_value(response: Value) -> Value {
    response.get("result").cloned().unwrap_or(response)
}

fn timestamp_now_iso() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs() as i64;
    let millis = now.subsec_millis();
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn id_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn cover_uri_to_url(uri: &str) -> String {
    cover_uri_to_url_sized(uri, "600x600")
}

fn cover_uri_to_url_sized(uri: &str, size: &str) -> String {
    if uri.is_empty() {
        return String::new();
    }
    let url = if uri.contains("%%") {
        uri.replace("%%", size)
    } else {
        uri.to_owned()
    };
    // Qt's HTTP/2 client gets REFUSED_STREAM from the Yandex image CDN when
    // several covers load at once, which made artwork appear intermittently.
    // The CDN serves the same public images over HTTP/1.1, so use plain HTTP
    // for artwork URLs handed to QML.
    let url = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("//");
    format!("http://{url}")
}

fn cover_uri_value(value: &Value) -> Option<&str> {
    value
        .get("coverUri")
        .or_else(|| value.get("cover_uri"))
        .and_then(Value::as_str)
        .or_else(|| {
            value.get("cover").and_then(|cover| {
                cover
                    .get("uri")
                    .or_else(|| {
                        cover
                            .get("itemsUri")
                            .and_then(Value::as_array)
                            .and_then(|items| items.first())
                    })
                    .and_then(Value::as_str)
            })
        })
}

fn cover_url_field(value: &Value) -> String {
    cover_uri_value(value)
        .map(cover_uri_to_url)
        .unwrap_or_default()
}

fn playlist_cover(playlist: &Value) -> String {
    let direct = cover_url_field(playlist);
    if !direct.is_empty() {
        return direct;
    }
    playlist
        .get("tracks")
        .and_then(Value::as_array)
        .and_then(|tracks| tracks.first())
        .map(|track| {
            track
                .get("track")
                .filter(|track| !track.is_null())
                .unwrap_or(track)
        })
        .map(cover_url_field)
        .unwrap_or_default()
}

fn strip_lrc(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut rest = line.trim_start();
            while rest.starts_with('[') {
                match rest.find(']') {
                    Some(end) => rest = rest[end + 1..].trim_start(),
                    None => break,
                }
            }
            rest.trim_end()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn dedupe_tracks(tracks: Vec<TrackItem>) -> Vec<TrackItem> {
    let mut seen = std::collections::HashSet::new();
    tracks
        .into_iter()
        .filter(|track| seen.insert(track.id.clone()))
        .collect()
}

fn xml_tag(xml: &str, tag: &str) -> Result<String, Error> {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    let value = xml
        .split_once(&start)
        .and_then(|(_, rest)| rest.split_once(&end).map(|(value, _)| value))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::InvalidResponse(format!("download XML is missing <{tag}>")))?;
    Ok(value.to_owned())
}

fn track_item(value: &Value) -> Option<TrackItem> {
    let value = value
        .get("track")
        .filter(|track| track.get("title").is_some())
        .unwrap_or(value);
    let id = value.get("id").map(id_string).filter(|id| !id.is_empty())?;
    let title = value.get("title")?.as_str()?.to_owned();
    let artist_node = value
        .get("artists")
        .and_then(Value::as_array)
        .and_then(|artists| artists.first());
    let artist = artist_node
        .and_then(|artist| artist.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("Unknown artist")
        .to_owned();
    let artist_id = artist_node.map(id_string).unwrap_or_default();
    let album_node = value
        .get("albums")
        .and_then(Value::as_array)
        .and_then(|albums| albums.first());
    let album = album_node
        .and_then(|album| album.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let album_id = album_node.map(id_string).unwrap_or_default();
    let cover_uri =
        cover_uri_value(value).or_else(|| album_node.and_then(cover_uri_value));
    let cover_url = cover_uri
        .map(|uri| cover_uri_to_url_sized(uri, "200x200"))
        .unwrap_or_default();
    let cover_source_url = cover_uri
        .map(|uri| cover_uri_to_url_sized(uri, "600x600"))
        .unwrap_or_default();
    Some(TrackItem {
        id,
        title,
        artist,
        artist_id,
        album,
        album_id,
        cover_url,
        cover_source_url,
        duration_ms: value
            .get("durationMs")
            .or_else(|| value.get("duration_ms"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
    })
}
