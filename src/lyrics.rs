use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use librespot_core::session::Session;
use librespot_core::spotify_id::SpotifyId;
use librespot_core::SpotifyUri;
use librespot_metadata::Lyrics;
use librespot_metadata::lyrics::SyncType;
use log::{debug, warn};

use crate::application::ASYNC_RUNTIME;
use crate::events::EventManager;
use crate::model::playable::Playable;
use crate::model::track::Track;
use crate::spotify::Spotify;
use crate::traits::ListItem;

#[derive(Clone, Debug)]
pub struct LyricLine {
    pub start_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct TrackLyrics {
    pub lines: Vec<LyricLine>,
    pub sync_type: SyncType,
    pub provider: String,
}

impl TrackLyrics {
    pub fn is_synced(&self) -> bool {
        matches!(self.sync_type, SyncType::LineSynced)
    }

    pub fn active_line_index(&self, position_ms: u64) -> usize {
        if self.lines.is_empty() {
            return 0;
        }

        let index = self
            .lines
            .partition_point(|line| line.start_ms <= position_ms)
            .saturating_sub(1);
        index.min(self.lines.len() - 1)
    }

    pub fn as_plain_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl From<Lyrics> for TrackLyrics {
    fn from(value: Lyrics) -> Self {
        let mut lines = value
            .lyrics
            .lines
            .into_iter()
            .filter_map(|line| {
                let start_ms = line.start_time_ms.parse::<u64>().ok()?;
                if line.words.is_empty() {
                    return None;
                }
                Some(LyricLine {
                    start_ms,
                    text: line.words,
                })
            })
            .collect::<Vec<_>>();
        lines.sort_by_key(|line| line.start_ms);
        Self {
            lines,
            sync_type: value.lyrics.sync_type,
            provider: value.lyrics.provider_display_name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LyricsDisplayStatus {
    Idle,
    Loading,
    Ready,
    NoLyrics,
    Error(String),
    NotAvailable,
}

#[derive(Clone, Debug)]
pub struct LyricsViewState {
    pub track_uri: Option<String>,
    pub title: String,
    pub artists: String,
    pub album: String,
    pub lyrics: Option<TrackLyrics>,
    pub message: String,
    pub status: LyricsDisplayStatus,
}

impl Default for LyricsViewState {
    fn default() -> Self {
        Self {
            track_uri: None,
            title: String::new(),
            artists: String::new(),
            album: String::new(),
            lyrics: None,
            message: String::new(),
            status: LyricsDisplayStatus::Idle,
        }
    }
}

pub struct LyricsManager {
    spotify: Spotify,
    events: EventManager,
    cache: Arc<RwLock<HashMap<String, Option<TrackLyrics>>>>,
    inflight: Arc<RwLock<HashSet<String>>>,
    state: Arc<RwLock<LyricsViewState>>,
}

impl LyricsManager {
    pub fn new(spotify: Spotify, events: EventManager) -> Self {
        Self {
            spotify,
            events,
            cache: Arc::new(RwLock::new(HashMap::new())),
            inflight: Arc::new(RwLock::new(HashSet::new())),
            state: Arc::new(RwLock::new(LyricsViewState::default())),
        }
    }

    pub fn state(&self) -> Arc<RwLock<LyricsViewState>> {
        self.state.clone()
    }

    pub fn sync_current_track(&self, playable: Option<Playable>) {
        let Some(playable) = playable else {
            let mut state = self.state.write().unwrap();
            *state = LyricsViewState {
                message: "No track is currently playing.".into(),
                status: LyricsDisplayStatus::Idle,
                ..LyricsViewState::default()
            };
            return;
        };

        if matches!(playable, Playable::Episode(_)) {
            let title = match &playable {
                Playable::Episode(episode) => episode.name.clone(),
                _ => String::new(),
            };
            let mut state = self.state.write().unwrap();
            *state = LyricsViewState {
                track_uri: Some(playable.uri()),
                title,
                artists: playable
                    .artists()
                    .map(|artists| {
                        artists
                            .iter()
                            .map(|artist| artist.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default(),
                album: String::new(),
                lyrics: None,
                message: "Lyrics are not available for podcasts.".into(),
                status: LyricsDisplayStatus::NotAvailable,
            };
            return;
        };

        let Playable::Track(track) = playable else {
            return;
        };

        let track_uri = track.uri.clone();
        {
            let state = self.state.read().unwrap();
            if state.track_uri.as_deref() == Some(track_uri.as_str())
                && !matches!(state.status, LyricsDisplayStatus::Idle)
            {
                return;
            }
        }

        self.set_track_metadata(&track);
        self.request_lyrics(track_uri);
    }

    fn set_track_metadata(&self, track: &Track) {
        let mut state = self.state.write().unwrap();
        state.track_uri = Some(track.uri.clone());
        state.title = track.title.clone();
        state.artists = track.artists.join(", ");
        state.album = track.album.clone().unwrap_or_default();
        state.lyrics = None;
        state.message = String::new();
        state.status = LyricsDisplayStatus::Loading;
    }

    fn request_lyrics(&self, track_uri: String) {
        if let Some(cached) = self.cache.read().unwrap().get(&track_uri).cloned() {
            self.apply_result(&track_uri, cached);
            return;
        }

        {
            let mut inflight = self.inflight.write().unwrap();
            if !inflight.insert(track_uri.clone()) {
                return;
            }
        }

        let Some(session) = self.spotify.get_session() else {
            self.inflight.write().unwrap().remove(&track_uri);
            self.set_error(&track_uri, "Spotify session is not available.");
            return;
        };

        let cache = self.cache.clone();
        let inflight = self.inflight.clone();
        let state = self.state.clone();
        let events = self.events.clone();

        ASYNC_RUNTIME.get().unwrap().spawn(async move {
            debug!("Fetching lyrics for {track_uri}");
            let result = fetch_lyrics(&session, &track_uri).await;
            inflight.write().unwrap().remove(&track_uri);

            let cached = match result {
                Ok(lyrics) => lyrics,
                Err(err) => {
                    warn!("Failed to fetch lyrics for {track_uri}: {err}");
                    {
                        let mut view_state = state.write().unwrap();
                        if view_state.track_uri.as_deref() == Some(track_uri.as_str()) {
                            view_state.lyrics = None;
                            view_state.message = format!("Failed to fetch lyrics: {err}");
                            view_state.status = LyricsDisplayStatus::Error(err.to_string());
                        }
                    }
                    events.trigger();
                    return;
                }
            };

            cache.write().unwrap().insert(track_uri.clone(), cached.clone());
            apply_lyrics_result(&state, &track_uri, cached);
            events.trigger();
        });
    }

    fn apply_result(&self, track_uri: &str, lyrics: Option<TrackLyrics>) {
        apply_lyrics_result(&self.state, track_uri, lyrics);
        self.events.trigger();
    }

    fn set_error(&self, track_uri: &str, message: &str) {
        let mut state = self.state.write().unwrap();
        if state.track_uri.as_deref() == Some(track_uri) {
            state.lyrics = None;
            state.message = message.to_string();
            state.status = LyricsDisplayStatus::Error(message.to_string());
        }
        self.events.trigger();
    }
}

fn apply_lyrics_result(
    state: &Arc<RwLock<LyricsViewState>>,
    track_uri: &str,
    lyrics: Option<TrackLyrics>,
) {
    let mut view_state = state.write().unwrap();
    if view_state.track_uri.as_deref() != Some(track_uri) {
        return;
    }

    match lyrics {
        Some(lyrics) if lyrics.lines.is_empty() => {
            view_state.lyrics = None;
            view_state.message = "No lyrics available for this track.".into();
            view_state.status = LyricsDisplayStatus::NoLyrics;
        }
        Some(lyrics) => {
            view_state.lyrics = Some(lyrics);
            view_state.message = String::new();
            view_state.status = LyricsDisplayStatus::Ready;
        }
        None => {
            view_state.lyrics = None;
            view_state.message = "No lyrics available for this track.".into();
            view_state.status = LyricsDisplayStatus::NoLyrics;
        }
    }
}

async fn fetch_lyrics(
    session: &Session,
    track_uri: &str,
) -> Result<Option<TrackLyrics>, librespot_core::Error> {
    let uri = SpotifyUri::from_uri(track_uri)?;
    let id = SpotifyId::try_from(&uri)?;
    match Lyrics::get(session, &id).await {
        Ok(lyrics) => Ok(Some(lyrics.into())),
        Err(err) if is_not_found(&err) => Ok(None),
        Err(err) => Err(err),
    }
}

fn is_not_found(err: &librespot_core::Error) -> bool {
    err.to_string().to_lowercase().contains("not found")
}
