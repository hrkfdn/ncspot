use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::config::SessionConfig;
use librespot_core::keymaster::Token;
use librespot_core::mercury::MercuryError;
use librespot_core::session::Session;
use librespot_playback::config::PlayerConfig;

use librespot_playback::audio_backend;
use librespot_playback::config::Bitrate;
use librespot_playback::player::Player;

use rspotify::blocking::client::Spotify as SpotifyAPI;
use rspotify::model::album::{FullAlbum, SavedAlbum};
use rspotify::model::artist::FullArtist;
use rspotify::model::page::{CursorBasedPage, Page};
use rspotify::model::playlist::FullPlaylist;
use rspotify::model::search::SearchResult;
use rspotify::model::track::{FullTrack, SavedTrack, SimplifiedTrack};
use rspotify::model::user::PrivateUser;
use rspotify::senum::{AlbumType, SearchType};
use rspotify::{blocking::client::ApiError, senum::Country};

use serde_json::{json, Map};

use failure::Error;

use futures_01::future::Future as v01_Future;

use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::compat::Future01CompatExt;
use futures::Future;

use tokio_core::reactor::Core;
use url::Url;

use std::pin::Pin;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, SystemTime};
use std::{env, io};

use crate::artist::Artist;
use crate::config;
use crate::events::{Event, EventManager};
use crate::playable::Playable;
use crate::spotify_worker::{Worker, WorkerCommand};
use crate::track::Track;

use crate::album::Album;
use crate::episode::Episode;
use crate::playlist::Playlist;
use crate::ui::pagination::{ApiPage, ApiResult};
use rspotify::model::recommend::Recommendations;
use rspotify::model::show::{FullEpisode, FullShow, Show};

pub const VOLUME_PERCENT: u16 = ((u16::max_value() as f64) * 1.0 / 100.0) as u16;

#[derive(Clone, Debug, PartialEq)]
pub enum PlayerEvent {
    Playing(SystemTime),
    Paused(Duration),
    Stopped,
    FinishedTrack,
}

#[derive(Clone)]
pub struct Spotify {
    events: EventManager,
    credentials: Credentials,
    cfg: Arc<config::Config>,
    status: Arc<RwLock<PlayerEvent>>,
    api: Arc<RwLock<SpotifyAPI>>,
    elapsed: Arc<RwLock<Option<Duration>>>,
    since: Arc<RwLock<Option<SystemTime>>>,
    token_issued: Arc<RwLock<Option<SystemTime>>>,
    channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
    user: Option<String>,
    country: Option<Country>,
}

impl Spotify {
    pub fn new(
        events: EventManager,
        credentials: Credentials,
        cfg: Arc<config::Config>,
    ) -> Spotify {
        let mut spotify = Spotify {
            events,
            credentials,
            cfg: cfg.clone(),
            status: Arc::new(RwLock::new(PlayerEvent::Stopped)),
            api: Arc::new(RwLock::new(SpotifyAPI::default())),
            elapsed: Arc::new(RwLock::new(None)),
            since: Arc::new(RwLock::new(None)),
            token_issued: Arc::new(RwLock::new(None)),
            channel: Arc::new(RwLock::new(None)),
            user: None,
            country: None,
        };

        let (user_tx, user_rx) = oneshot::channel();
        spotify.start_worker(Some(user_tx));
        spotify.user = futures::executor::block_on(user_rx).ok();
        let volume = cfg.state().volume;
        spotify.set_volume(volume);

        spotify.country = spotify
            .current_user()
            .and_then(|u| u.country)
            .and_then(|c| c.parse().ok());

        spotify
    }

    pub fn start_worker(&self, user_tx: Option<oneshot::Sender<String>>) {
        let (tx, rx) = mpsc::unbounded();
        *self
            .channel
            .write()
            .expect("can't writelock worker channel") = Some(tx);
        {
            let cfg = self.cfg.clone();
            let events = self.events.clone();
            let volume = self.volume();
            let credentials = self.credentials.clone();
            thread::spawn(move || {
                Self::worker(
                    events,
                    Box::pin(rx),
                    cfg.clone(),
                    credentials,
                    user_tx,
                    volume,
                )
            });
        }

        // acquire token for web api usage
        self.refresh_token();
    }

    pub fn session_config() -> SessionConfig {
        let mut session_config = SessionConfig::default();
        match env::var("http_proxy") {
            Ok(proxy) => {
                info!("Setting HTTP proxy {}", proxy);
                session_config.proxy = Url::parse(&proxy).ok();
            }
            Err(_) => debug!("No HTTP proxy set"),
        }
        session_config
    }

    pub fn test_credentials(credentials: Credentials) -> Result<Session, std::io::Error> {
        let jh = thread::spawn(move || {
            let mut core = Core::new().unwrap();
            let config = Self::session_config();
            let handle = core.handle();

            core.run(Session::connect(config, credentials, None, handle))
        });
        match jh.join() {
            Ok(session) => session,
            Err(e) => Err(io::Error::new(
                io::ErrorKind::Other,
                e.downcast_ref::<String>()
                    .unwrap_or(&"N/A".to_string())
                    .to_string(),
            )),
        }
    }

    fn create_session(core: &mut Core, cfg: &config::Config, credentials: Credentials) -> Session {
        let session_config = Self::session_config();
        let cache = Cache::new(
            config::cache_path("librespot"),
            cfg.values().audio_cache.unwrap_or(true),
        );
        let handle = core.handle();
        debug!("opening spotify session");
        println!("Connecting to Spotify..");
        core.run(Session::connect(
            session_config,
            credentials,
            Some(cache),
            handle,
        ))
        .expect("could not open spotify session")
    }

    pub(crate) fn get_token(
        session: &Session,
        sender: oneshot::Sender<Token>,
    ) -> Pin<Box<dyn Future<Output = Result<(), MercuryError>>>> {
        let client_id = config::CLIENT_ID;
        let scopes = "user-read-private,playlist-read-private,playlist-read-collaborative,playlist-modify-public,playlist-modify-private,user-follow-modify,user-follow-read,user-library-read,user-library-modify,user-top-read,user-read-recently-played";
        let url = format!(
            "hm://keymaster/token/authenticated?client_id={}&scope={}",
            client_id, scopes
        );
        Box::pin(
            session
                .mercury()
                .get(url)
                .map(move |response| {
                    let data = response.payload.first().expect("Empty payload");
                    let data = String::from_utf8(data.clone()).unwrap();
                    let token: Token = serde_json::from_str(&data).unwrap();
                    info!("new token received: {:?}", token);
                    token
                })
                .map(|token| sender.send(token).unwrap())
                .compat(),
        )
    }

    fn worker(
        events: EventManager,
        commands: Pin<Box<mpsc::UnboundedReceiver<WorkerCommand>>>,
        cfg: Arc<config::Config>,
        credentials: Credentials,
        user_tx: Option<oneshot::Sender<String>>,
        volume: u16,
    ) {
        let bitrate_str = cfg.values().bitrate.unwrap_or(320).to_string();
        let bitrate = Bitrate::from_str(&bitrate_str);
        if bitrate.is_err() {
            error!("invalid bitrate, will use 320 instead")
        }

        let player_config = PlayerConfig {
            gapless: cfg.values().gapless.unwrap_or(false),
            bitrate: bitrate.unwrap_or(Bitrate::Bitrate320),
            normalisation: cfg.values().volnorm.unwrap_or(false),
            normalisation_pregain: cfg.values().volnorm_pregain.unwrap_or(0.0),
        };

        let mut core = Core::new().unwrap();

        let session = Self::create_session(&mut core, &cfg, credentials);
        user_tx.map(|tx| tx.send(session.username()));

        let create_mixer = librespot_playback::mixer::find(Some("softvol".to_owned()))
            .expect("could not create softvol mixer");
        let mixer = create_mixer(None);
        mixer.set_volume(volume);

        let backend = audio_backend::find(cfg.values().backend.clone()).unwrap();
        let (player, player_events) = Player::new(
            player_config,
            session.clone(),
            mixer.get_audio_filter(),
            move || (backend)(cfg.values().backend_device.clone()),
        );

        let worker = Worker::new(
            events.clone(),
            player_events,
            commands,
            session,
            player,
            mixer,
        );
        debug!("worker thread ready.");
        if core.run(futures::compat::Compat::new(worker)).is_err() {
            error!("worker thread died, requesting restart");
            events.send(Event::SessionDied)
        }
    }

    pub fn get_current_status(&self) -> PlayerEvent {
        let status = self
            .status
            .read()
            .expect("could not acquire read lock on playback status");
        (*status).clone()
    }

    pub fn get_current_progress(&self) -> Duration {
        self.get_elapsed().unwrap_or_else(|| Duration::from_secs(0))
            + self
                .get_since()
                .map(|t| t.elapsed().unwrap())
                .unwrap_or_else(|| Duration::from_secs(0))
    }

    fn set_elapsed(&self, new_elapsed: Option<Duration>) {
        let mut elapsed = self
            .elapsed
            .write()
            .expect("could not acquire write lock on elapsed time");
        *elapsed = new_elapsed;
    }

    fn get_elapsed(&self) -> Option<Duration> {
        let elapsed = self
            .elapsed
            .read()
            .expect("could not acquire read lock on elapsed time");
        *elapsed
    }

    fn set_since(&self, new_since: Option<SystemTime>) {
        let mut since = self
            .since
            .write()
            .expect("could not acquire write lock on since time");
        *since = new_since;
    }

    fn get_since(&self) -> Option<SystemTime> {
        let since = self
            .since
            .read()
            .expect("could not acquire read lock on since time");
        *since
    }

    pub fn refresh_token(&self) {
        {
            let expiry = self.token_issued.read().unwrap();
            if let Some(time) = *expiry {
                if time.elapsed().unwrap() < Duration::from_secs(3000) {
                    return;
                }
            }
        }

        let (token_tx, token_rx) = oneshot::channel();
        self.send_worker(WorkerCommand::RequestToken(token_tx));
        let token = futures::executor::block_on(token_rx).unwrap();

        // update token used by web api calls
        self.api.write().expect("can't writelock api").access_token = Some(token.access_token);
        self.token_issued
            .write()
            .unwrap()
            .replace(SystemTime::now());
    }

    /// retries once when rate limits are hit
    fn api_with_retry<F, R>(&self, cb: F) -> Option<R>
    where
        F: Fn(&SpotifyAPI) -> Result<R, Error>,
    {
        let result = {
            let api = self.api.read().expect("can't read api");
            cb(&api)
        };
        match result {
            Ok(v) => Some(v),
            Err(e) => {
                debug!("api error: {:?}", e);
                if let Ok(apierror) = e.downcast::<ApiError>() {
                    match apierror {
                        ApiError::RateLimited(d) => {
                            debug!("rate limit hit. waiting {:?} seconds", d);
                            thread::sleep(Duration::from_secs(d.unwrap_or(0) as u64));
                            let api = self.api.read().expect("can't read api");
                            cb(&api).ok()
                        }
                        ApiError::Unauthorized => {
                            debug!("token unauthorized. trying refresh..");
                            self.refresh_token();
                            let api = self.api.read().expect("can't read api");
                            cb(&api).ok()
                        }
                        e => {
                            error!("unhandled api error: {}", e);
                            None
                        }
                    }
                } else {
                    None
                }
            }
        }
    }

    pub fn append_tracks(
        &self,
        playlist_id: &str,
        tracks: &[String],
        position: Option<i32>,
    ) -> bool {
        self.api_with_retry(|api| {
            api.user_playlist_add_tracks(
                self.user.as_ref().unwrap(),
                playlist_id,
                &tracks,
                position,
            )
        })
        .is_some()
    }

    pub fn delete_tracks(
        &self,
        playlist_id: &str,
        snapshot_id: &str,
        track_pos_pairs: &[(&Track, usize)],
    ) -> bool {
        let mut tracks = Vec::new();
        for (track, pos) in track_pos_pairs {
            let track_occurrence = json!({
                "uri": format!("spotify:track:{}", track.id.clone().unwrap()),
                "positions": [pos]
            });
            let track_occurrence_object = track_occurrence.as_object();
            tracks.push(track_occurrence_object.unwrap().clone());
        }
        self.api_with_retry(|api| {
            api.user_playlist_remove_specific_occurrenes_of_tracks(
                self.user.as_ref().unwrap(),
                playlist_id,
                tracks.clone(),
                Some(snapshot_id.to_string()),
            )
        })
        .is_some()
    }

    pub fn overwrite_playlist(&self, id: &str, tracks: &[Playable]) {
        // extract only track IDs
        let mut tracks: Vec<String> = tracks.iter().filter_map(|track| track.id()).collect();

        // we can only send 100 tracks per request
        let mut remainder = if tracks.len() > 100 {
            Some(tracks.split_off(100))
        } else {
            None
        };

        if let Some(()) = self.api_with_retry(|api| {
            api.user_playlist_replace_tracks(self.user.as_ref().unwrap(), id, &tracks)
        }) {
            debug!("saved {} tracks to playlist {}", tracks.len(), id);
            while let Some(ref mut tracks) = remainder.clone() {
                // grab the next set of 100 tracks
                remainder = if tracks.len() > 100 {
                    Some(tracks.split_off(100))
                } else {
                    None
                };

                debug!("adding another {} tracks to playlist", tracks.len());
                if self.append_tracks(id, tracks, None) {
                    debug!("{} tracks successfully added", tracks.len());
                } else {
                    error!("error saving tracks to playlists {}", id);
                    return;
                }
            }
        } else {
            error!("error saving tracks to playlist {}", id);
        }
    }

    pub fn delete_playlist(&self, id: &str) -> bool {
        self.api_with_retry(|api| api.user_playlist_unfollow(self.user.as_ref().unwrap(), id))
            .is_some()
    }

    pub fn create_playlist(
        &self,
        name: &str,
        public: Option<bool>,
        description: Option<String>,
    ) -> Option<String> {
        let result = self.api_with_retry(|api| {
            api.user_playlist_create(
                self.user.as_ref().unwrap(),
                name,
                public,
                description.clone(),
            )
        });
        result.map(|r| r.id)
    }

    pub fn album(&self, album_id: &str) -> Option<FullAlbum> {
        self.api_with_retry(|api| api.album(album_id))
    }

    pub fn artist(&self, artist_id: &str) -> Option<FullArtist> {
        self.api_with_retry(|api| api.artist(artist_id))
    }

    pub fn playlist(&self, playlist_id: &str) -> Option<FullPlaylist> {
        self.api_with_retry(|api| api.playlist(playlist_id, None, self.country))
    }

    pub fn track(&self, track_id: &str) -> Option<FullTrack> {
        self.api_with_retry(|api| api.track(track_id))
    }

    pub fn get_show(&self, show_id: &str) -> Option<FullShow> {
        self.api_with_retry(|api| api.get_a_show(show_id.to_string(), self.country))
    }

    pub fn episode(&self, episode_id: &str) -> Option<FullEpisode> {
        self.api_with_retry(|api| api.get_an_episode(episode_id.to_string(), self.country))
    }

    pub fn recommendations(
        &self,
        seed_artists: Option<Vec<String>>,
        seed_genres: Option<Vec<String>>,
        seed_tracks: Option<Vec<String>>,
    ) -> Option<Recommendations> {
        self.api_with_retry(|api| {
            api.recommendations(
                seed_artists.clone(),
                seed_genres.clone(),
                seed_tracks.clone(),
                100,
                self.country,
                &Map::new(),
            )
        })
    }

    pub fn search(
        &self,
        searchtype: SearchType,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Option<SearchResult> {
        self.api_with_retry(|api| api.search(query, searchtype, limit, offset, self.country, None))
            .take()
    }

    pub fn current_user_playlist(&self) -> ApiResult<Playlist> {
        const MAX_LIMIT: u32 = 50;
        let spotify = self.clone();
        let fetch_page = move |offset: u32| {
            debug!("fetching user playlists, offset: {}", offset);
            spotify.api_with_retry(|api| match api.current_user_playlists(MAX_LIMIT, offset) {
                Ok(page) => Ok(ApiPage {
                    offset: page.offset,
                    total: page.total,
                    items: page.items.iter().map(|sp| sp.into()).collect(),
                }),
                Err(e) => Err(e),
            })
        };
        ApiResult::new(MAX_LIMIT, Arc::new(fetch_page))
    }

    pub fn user_playlist_tracks(&self, playlist_id: &str) -> ApiResult<Track> {
        const MAX_LIMIT: u32 = 100;
        let spotify = self.clone();
        let playlist_id = playlist_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!(
                "fetching playlist {} tracks, offset: {}",
                playlist_id, offset
            );
            spotify.api_with_retry(|api| {
                match api.user_playlist_tracks(
                    spotify.user.as_ref().unwrap(),
                    &playlist_id,
                    None,
                    MAX_LIMIT,
                    offset,
                    spotify.country,
                ) {
                    Ok(page) => Ok(ApiPage {
                        offset: page.offset,
                        total: page.total,
                        items: page
                            .items
                            .iter()
                            .enumerate()
                            .flat_map(|(index, pt)| {
                                pt.track.as_ref().map(|t| {
                                    let mut track: Track = t.into();
                                    track.added_at = Some(pt.added_at);
                                    track.list_index = page.offset as usize + index;
                                    track
                                })
                            })
                            .collect(),
                    }),
                    Err(e) => Err(e),
                }
            })
        };
        ApiResult::new(MAX_LIMIT, Arc::new(fetch_page))
    }

    pub fn full_album(&self, album_id: &str) -> Option<FullAlbum> {
        self.api_with_retry(|api| api.album(album_id))
    }

    pub fn album_tracks(
        &self,
        album_id: &str,
        limit: u32,
        offset: u32,
    ) -> Option<Page<SimplifiedTrack>> {
        self.api_with_retry(|api| api.album_track(album_id, limit, offset))
    }

    pub fn artist_albums(
        &self,
        artist_id: &str,
        album_type: Option<AlbumType>,
    ) -> ApiResult<Album> {
        const MAX_SIZE: u32 = 50;
        let spotify = self.clone();
        let artist_id = artist_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!("fetching artist {} albums, offset: {}", artist_id, offset);
            spotify.api_with_retry(|api| {
                match api.artist_albums(
                    &artist_id,
                    album_type,
                    spotify.country,
                    Some(MAX_SIZE),
                    Some(offset),
                ) {
                    Ok(page) => {
                        let mut albums: Vec<Album> =
                            page.items.iter().map(|sa| sa.into()).collect();
                        albums.sort_by(|a, b| b.year.cmp(&a.year));
                        Ok(ApiPage {
                            offset: page.offset,
                            total: page.total,
                            items: albums,
                        })
                    }
                    Err(e) => Err(e),
                }
            })
        };

        ApiResult::new(MAX_SIZE, Arc::new(fetch_page))
    }

    pub fn show_episodes(&self, show_id: &str) -> ApiResult<Episode> {
        const MAX_SIZE: u32 = 50;
        let spotify = self.clone();
        let show_id = show_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!("fetching show {} episodes, offset: {}", &show_id, offset);
            spotify.api_with_retry(|api| {
                match api.get_shows_episodes(show_id.clone(), MAX_SIZE, offset, spotify.country) {
                    Ok(page) => Ok(ApiPage {
                        offset: page.offset,
                        total: page.total,
                        items: page.items.iter().map(|se| se.into()).collect(),
                    }),
                    Err(e) => Err(e),
                }
            })
        };

        ApiResult::new(MAX_SIZE, Arc::new(fetch_page))
    }

    pub fn get_saved_shows(&self, offset: u32) -> Option<Page<Show>> {
        self.api_with_retry(|api| api.get_saved_show(50, offset))
    }

    pub fn save_shows(&self, ids: Vec<String>) -> bool {
        self.api_with_retry(|api| api.save_shows(ids.clone()))
            .is_some()
    }

    pub fn unsave_shows(&self, ids: Vec<String>) -> bool {
        self.api_with_retry(|api| api.remove_users_saved_shows(ids.clone(), self.country))
            .is_some()
    }

    pub fn current_user_followed_artists(
        &self,
        last: Option<String>,
    ) -> Option<CursorBasedPage<FullArtist>> {
        self.api_with_retry(|api| api.current_user_followed_artists(50, last.clone()))
            .map(|cp| cp.artists)
    }

    pub fn user_follow_artists(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.user_follow_artists(&ids))
    }

    pub fn user_unfollow_artists(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.user_unfollow_artists(&ids))
    }

    pub fn current_user_saved_albums(&self, offset: u32) -> Option<Page<SavedAlbum>> {
        self.api_with_retry(|api| api.current_user_saved_albums(50, offset))
    }

    pub fn current_user_saved_albums_add(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.current_user_saved_albums_add(&ids))
    }

    pub fn current_user_saved_albums_delete(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.current_user_saved_albums_delete(&ids))
    }

    pub fn current_user_saved_tracks(&self, offset: u32) -> Option<Page<SavedTrack>> {
        self.api_with_retry(|api| api.current_user_saved_tracks(50, offset))
    }

    pub fn current_user_saved_tracks_add(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.current_user_saved_tracks_add(&ids))
    }

    pub fn current_user_saved_tracks_delete(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.current_user_saved_tracks_delete(&ids))
    }

    pub fn user_playlist_follow_playlist(&self, owner_id: String, id: String) -> Option<()> {
        self.api_with_retry(|api| api.user_playlist_follow_playlist(&owner_id, &id, true))
    }

    pub fn artist_top_tracks(&self, id: &str) -> Option<Vec<Track>> {
        self.api_with_retry(|api| api.artist_top_tracks(id, self.country))
            .map(|ft| ft.tracks.iter().map(|t| t.into()).collect())
    }

    pub fn artist_related_artists(&self, id: String) -> Option<Vec<Artist>> {
        self.api_with_retry(|api| api.artist_related_artists(&id))
            .map(|fa| fa.artists.iter().map(|a| a.into()).collect())
    }

    pub fn current_user(&self) -> Option<PrivateUser> {
        self.api_with_retry(|api| api.current_user())
    }

    pub fn load(&self, track: &Playable, start_playing: bool, position_ms: u32) {
        info!("loading track: {:?}", track);
        self.send_worker(WorkerCommand::Load(
            track.clone(),
            start_playing,
            position_ms,
        ));
    }

    pub fn update_status(&self, new_status: PlayerEvent) {
        match new_status {
            PlayerEvent::Paused(position) => {
                self.set_elapsed(Some(position));
                self.set_since(None);
            }
            PlayerEvent::Playing(playback_start) => {
                self.set_since(Some(playback_start));
                self.set_elapsed(None);
            }
            PlayerEvent::Stopped | PlayerEvent::FinishedTrack => {
                self.set_elapsed(None);
                self.set_since(None);
            }
        }

        let mut status = self
            .status
            .write()
            .expect("could not acquire write lock on player status");
        *status = new_status;
    }

    pub fn update_track(&self) {
        self.set_elapsed(None);
        self.set_since(None);
    }

    pub fn play(&self) {
        info!("play()");
        self.send_worker(WorkerCommand::Play);
    }

    pub fn toggleplayback(&self) {
        match self.get_current_status() {
            PlayerEvent::Playing(_) => self.pause(),
            PlayerEvent::Paused(_) => self.play(),
            _ => (),
        }
    }

    fn send_worker(&self, cmd: WorkerCommand) {
        let channel = self.channel.read().expect("can't readlock worker channel");
        match channel.as_ref() {
            Some(channel) => channel
                .unbounded_send(cmd)
                .expect("can't send message to worker"),
            None => error!("no channel to worker available"),
        }
    }

    pub fn pause(&self) {
        info!("pause()");
        self.send_worker(WorkerCommand::Pause);
    }

    pub fn stop(&self) {
        info!("stop()");
        self.send_worker(WorkerCommand::Stop);
    }

    pub fn seek(&self, position_ms: u32) {
        self.send_worker(WorkerCommand::Seek(position_ms));
    }

    pub fn seek_relative(&self, delta: i32) {
        let progress = self.get_current_progress();
        let new = (progress.as_secs() * 1000) as i32 + progress.subsec_millis() as i32 + delta;
        self.seek(std::cmp::max(0, new) as u32);
    }

    pub fn volume(&self) -> u16 {
        self.cfg.state().volume
    }

    fn log_scale(volume: u16) -> u16 {
        // https://www.dr-lex.be/info-stuff/volumecontrols.html#ideal2
        // a * exp(b * x)
        const A: f64 = 1.0 / 1000.0;
        const B: f64 = 6.908;
        let volume_percent = volume as f64 / u16::max_value() as f64;
        let log_volume = A * (B * volume_percent).exp();
        let result = log_volume * u16::max_value() as f64;

        // u16 overflow check
        if result > u16::max_value() as f64 {
            u16::max_value()
        } else {
            result as u16
        }
    }

    pub fn set_volume(&self, volume: u16) {
        info!("setting volume to {}", volume);
        self.cfg.with_state_mut(|mut s| s.volume = volume);
        self.send_worker(WorkerCommand::SetVolume(Self::log_scale(volume)));
    }

    pub fn preload(&self, track: &Playable) {
        self.send_worker(WorkerCommand::Preload(track.clone()));
    }

    pub fn shutdown(&self) {
        self.send_worker(WorkerCommand::Shutdown);
    }
}

#[derive(Debug, PartialEq)]
pub enum UriType {
    Album,
    Artist,
    Track,
    Playlist,
    Show,
    Episode,
}

impl UriType {
    pub fn from_uri(s: &str) -> Option<UriType> {
        if s.starts_with("spotify:album:") {
            Some(UriType::Album)
        } else if s.starts_with("spotify:artist:") {
            Some(UriType::Artist)
        } else if s.starts_with("spotify:track:") {
            Some(UriType::Track)
        } else if s.starts_with("spotify:") && s.contains(":playlist:") {
            Some(UriType::Playlist)
        } else if s.starts_with("spotify:show:") {
            Some(UriType::Show)
        } else if s.starts_with("spotify:episode:") {
            Some(UriType::Episode)
        } else {
            None
        }
    }
}
