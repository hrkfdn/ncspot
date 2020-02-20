use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::config::SessionConfig;
use librespot_core::keymaster::Token;
use librespot_core::mercury::MercuryError;
use librespot_core::session::Session;
use librespot_core::spotify_id::SpotifyId;
use librespot_playback::config::PlayerConfig;

use librespot_playback::audio_backend;
use librespot_playback::config::Bitrate;
use librespot_playback::mixer::Mixer;
use librespot_playback::player::Player;

use rspotify::spotify::client::ApiError;
use rspotify::spotify::client::Spotify as SpotifyAPI;
use rspotify::spotify::model::album::{FullAlbum, SavedAlbum, SimplifiedAlbum};
use rspotify::spotify::model::artist::FullArtist;
use rspotify::spotify::model::page::{CursorBasedPage, Page};
use rspotify::spotify::model::playlist::{FullPlaylist, PlaylistTrack, SimplifiedPlaylist};
use rspotify::spotify::model::search::{
    SearchAlbums, SearchArtists, SearchPlaylists, SearchTracks,
};
use rspotify::spotify::model::track::{FullTrack, SavedTrack};
use rspotify::spotify::model::user::PrivateUser;

use failure::Error;

use futures;
use futures::sync::mpsc;
use futures::sync::oneshot;
use futures::Async;
use futures::Future;
use futures::Stream;
use tokio_core::reactor::Core;
use tokio_timer;
use url::Url;

use std::env;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::RwLock;
use std::thread;
use std::time::{Duration, SystemTime};

use artist::Artist;
use config;
use events::{Event, EventManager};
use track::Track;
use queue;

pub const VOLUME_PERCENT: u16 = ((u16::max_value() as f64) * 1.0 / 100.0) as u16;

enum WorkerCommand {
    Load(Box<Track>),
    Play,
    Pause,
    Stop,
    Seek(u32),
    SetVolume(u16),
    RequestToken(oneshot::Sender<Token>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlayerEvent {
    Playing,
    Paused,
    Stopped,
    FinishedTrack,
}

pub struct Spotify {
    status: RwLock<PlayerEvent>,
    api: RwLock<SpotifyAPI>,
    elapsed: RwLock<Option<Duration>>,
    since: RwLock<Option<SystemTime>>,
    token_issued: RwLock<Option<SystemTime>>,
    channel: mpsc::UnboundedSender<WorkerCommand>,
    user: String,
    pub volume: AtomicU16,
    pub repeat: queue::RepeatSetting,
    pub shuffle: bool,
}

struct Worker {
    events: EventManager,
    commands: mpsc::UnboundedReceiver<WorkerCommand>,
    session: Session,
    player: Player,
    play_task: Box<dyn futures::Future<Item = (), Error = oneshot::Canceled>>,
    refresh_task: Box<dyn futures::Stream<Item = (), Error = tokio_timer::Error>>,
    token_task: Box<dyn futures::Future<Item = (), Error = MercuryError>>,
    active: bool,
    mixer: Box<dyn Mixer>,
}

impl Worker {
    fn new(
        events: EventManager,
        commands: mpsc::UnboundedReceiver<WorkerCommand>,
        session: Session,
        player: Player,
        mixer: Box<dyn Mixer>,
    ) -> Worker {
        Worker {
            events,
            commands,
            player,
            session,
            play_task: Box::new(futures::empty()),
            refresh_task: Box::new(futures::stream::empty()),
            token_task: Box::new(futures::empty()),
            active: false,
            mixer,
        }
    }
}

impl Worker {
    fn create_refresh(&self) -> Box<dyn futures::Stream<Item = (), Error = tokio_timer::Error>> {
        let ev = self.events.clone();
        let future =
            tokio_timer::Interval::new_interval(Duration::from_millis(400)).map(move |_| {
                ev.trigger();
            });
        Box::new(future)
    }
}

impl futures::Future for Worker {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> futures::Poll<(), ()> {
        loop {
            let mut progress = false;

            if let Async::Ready(Some(cmd)) = self.commands.poll().unwrap() {
                progress = true;
                debug!("message received!");
                match cmd {
                    WorkerCommand::Load(track) => {
                        if let Some(track_id) = &track.id {
                            let id = SpotifyId::from_base62(track_id).expect("could not parse id");
                            self.play_task = Box::new(self.player.load(id, false, 0));
                            info!("player loading track: {:?}", track);
                        } else {
                            self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                        }
                    }
                    WorkerCommand::Play => {
                        self.player.play();
                        self.events.send(Event::Player(PlayerEvent::Playing));
                        self.refresh_task = self.create_refresh();
                        self.active = true;
                    }
                    WorkerCommand::Pause => {
                        self.player.pause();
                        self.events.send(Event::Player(PlayerEvent::Paused));
                        self.active = false;
                    }
                    WorkerCommand::Stop => {
                        self.player.stop();
                        self.events.send(Event::Player(PlayerEvent::Stopped));
                        self.active = false;
                    }
                    WorkerCommand::Seek(pos) => {
                        self.player.seek(pos);
                    }
                    WorkerCommand::SetVolume(volume) => {
                        self.mixer.set_volume(volume);
                    }
                    WorkerCommand::RequestToken(sender) => {
                        self.token_task = Spotify::get_token(&self.session, sender);
                        progress = true;
                    }
                }
            }
            match self.play_task.poll() {
                Ok(Async::Ready(())) => {
                    debug!("end of track!");
                    progress = true;
                    self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                }
                Ok(Async::NotReady) => (),
                Err(oneshot::Canceled) => {
                    debug!("player task is over!");
                    self.play_task = Box::new(futures::empty());
                }
            }
            if let Ok(Async::Ready(_)) = self.refresh_task.poll() {
                self.refresh_task = if self.active {
                    progress = true;
                    self.create_refresh()
                } else {
                    Box::new(futures::stream::empty())
                };
            }
            match self.token_task.poll() {
                Ok(Async::Ready(_)) => {
                    info!("token updated!");
                    self.token_task = Box::new(futures::empty())
                }
                Err(e) => {
                    error!("could not generate token: {:?}", e);
                }
                _ => (),
            }

            if !progress {
                return Ok(Async::NotReady);
            }
        }
    }
}

impl Spotify {
    pub fn new(events: EventManager, credentials: Credentials, cfg: &config::Config) -> Spotify {
        let player_config = PlayerConfig {
            bitrate: Bitrate::Bitrate320,
            normalisation: false,
            normalisation_pregain: 0.0,
        };
        let (user_tx, user_rx) = oneshot::channel();
        let volume = match &cfg.saved_state {
            Some(state) => match state.volume {
                Some(vol) => vol,
                None => 0xFFFF as u16,
            },
            None => 0xFFFF as u16,
        };
        let repeat = match &cfg.saved_state {
            Some(state) => match &state.repeat {
                Some(s) => match s.as_str() {
                    "track" => queue::RepeatSetting::RepeatTrack,
                    "playlist" => queue::RepeatSetting::RepeatPlaylist,
                    _ => queue::RepeatSetting::None,
                }
                _ => queue::RepeatSetting::None,
            },
            _ => queue::RepeatSetting::None,
        };
        let shuffle = match &cfg.saved_state {
            Some(state) => match &state.shuffle {
                Some(true) => true,
                _ => false,
            },
            None => false,
        };

        let (tx, rx) = mpsc::unbounded();
        {
            let events = events.clone();
            thread::spawn(move || {
                Self::worker(events, rx, player_config, credentials, user_tx, volume)
            });
        }

        let spotify = Spotify {
            status: RwLock::new(PlayerEvent::Stopped),
            api: RwLock::new(SpotifyAPI::default()),
            elapsed: RwLock::new(None),
            since: RwLock::new(None),
            token_issued: RwLock::new(None),
            channel: tx,
            user: user_rx.wait().expect("error retrieving userid from worker"),
            volume: AtomicU16::new(volume),
            repeat: repeat,
            shuffle: shuffle,
        };

        // acquire token for web api usage
        spotify.refresh_token();
        spotify
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

    pub fn test_credentials(credentials: Credentials) -> bool {
        let th = thread::spawn(move || {
            let mut core = Core::new().unwrap();
            let config = Self::session_config();
            let handle = core.handle();

            core.run(Session::connect(config, credentials, None, handle))
                .unwrap();
        });
        th.join().is_ok()
    }

    fn create_session(core: &mut Core, credentials: Credentials) -> Session {
        let session_config = Self::session_config();
        let cache = Cache::new(config::cache_path("librespot"), true);
        let handle = core.handle();
        debug!("opening spotify session");
        core.run(Session::connect(
            session_config,
            credentials,
            Some(cache),
            handle,
        ))
        .ok()
        .unwrap()
    }

    fn get_token(
        session: &Session,
        sender: oneshot::Sender<Token>,
    ) -> Box<dyn Future<Item = (), Error = MercuryError>> {
        let client_id = config::CLIENT_ID;
        let scopes = "user-read-private,playlist-read-private,playlist-read-collaborative,playlist-modify-public,playlist-modify-private,user-follow-modify,user-follow-read,user-library-read,user-library-modify,user-top-read,user-read-recently-played";
        let url = format!(
            "hm://keymaster/token/authenticated?client_id={}&scope={}",
            client_id, scopes
        );
        Box::new(
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
                .map(|token| sender.send(token).unwrap()),
        )
    }

    fn worker(
        events: EventManager,
        commands: mpsc::UnboundedReceiver<WorkerCommand>,
        player_config: PlayerConfig,
        credentials: Credentials,
        user_tx: oneshot::Sender<String>,
        volume: u16,
    ) {
        let mut core = Core::new().unwrap();

        let session = Self::create_session(&mut core, credentials);
        user_tx
            .send(session.username())
            .expect("could not pass username back to Spotify::new");

        let create_mixer = librespot_playback::mixer::find(Some("softvol".to_owned()))
            .expect("could not create softvol mixer");
        let mixer = create_mixer(None);
        mixer.set_volume(volume);

        let backend = audio_backend::find(None).unwrap();
        let (player, _eventchannel) = Player::new(
            player_config,
            session.clone(),
            mixer.get_audio_filter(),
            move || (backend)(None),
        );

        let worker = Worker::new(events, commands, session, player, mixer);
        debug!("worker thread ready.");
        core.run(worker).unwrap();
        debug!("worker thread finished.");
    }

    pub fn get_current_status(&self) -> PlayerEvent {
        let status = self
            .status
            .read()
            .expect("could not acquire read lock on playback status");
        (*status).clone()
    }

    pub fn get_current_progress(&self) -> Duration {
        self.get_elapsed().unwrap_or(Duration::from_secs(0))
            + self
                .get_since()
                .map(|t| t.elapsed().unwrap())
                .unwrap_or(Duration::from_secs(0))
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
        (*elapsed)
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
        (*since)
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
        self.channel
            .unbounded_send(WorkerCommand::RequestToken(token_tx))
            .unwrap();
        let token = token_rx.wait().unwrap();

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
            api.user_playlist_add_tracks(&self.user, playlist_id, &tracks, position)
        })
        .is_some()
    }

    pub fn overwrite_playlist(&self, id: &str, tracks: &[Track]) {
        // extract only track IDs
        let mut tracks: Vec<String> = tracks
            .iter()
            .filter(|track| track.id.is_some())
            .map(|track| track.id.clone().unwrap())
            .collect();

        // we can only send 100 tracks per request
        let mut remainder = if tracks.len() > 100 {
            Some(tracks.split_off(100))
        } else {
            None
        };

        if let Some(()) =
            self.api_with_retry(|api| api.user_playlist_replace_tracks(&self.user, id, &tracks))
        {
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
        self.api_with_retry(|api| api.user_playlist_unfollow(&self.user, id))
            .is_some()
    }

    pub fn create_playlist(
        &self,
        name: &str,
        public: Option<bool>,
        description: Option<String>,
    ) -> Option<String> {
        let result = self.api_with_retry(|api| {
            api.user_playlist_create(&self.user, name, public, description.clone())
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
        self.api_with_retry(|api| api.playlist(playlist_id, None, None))
    }

    pub fn track(&self, track_id: &str) -> Option<FullTrack> {
        self.api_with_retry(|api| api.track(track_id))
    }

    pub fn search_track(&self, query: &str, limit: u32, offset: u32) -> Option<SearchTracks> {
        self.api_with_retry(|api| api.search_track(query, limit, offset, None))
    }

    pub fn search_album(&self, query: &str, limit: u32, offset: u32) -> Option<SearchAlbums> {
        self.api_with_retry(|api| api.search_album(query, limit, offset, None))
    }

    pub fn search_artist(&self, query: &str, limit: u32, offset: u32) -> Option<SearchArtists> {
        self.api_with_retry(|api| api.search_artist(query, limit, offset, None))
    }

    pub fn search_playlist(&self, query: &str, limit: u32, offset: u32) -> Option<SearchPlaylists> {
        self.api_with_retry(|api| api.search_playlist(query, limit, offset, None))
    }

    pub fn current_user_playlist(
        &self,
        limit: u32,
        offset: u32,
    ) -> Option<Page<SimplifiedPlaylist>> {
        self.api_with_retry(|api| api.current_user_playlists(limit, offset))
    }

    pub fn user_playlist_tracks(
        &self,
        playlist_id: &str,
        limit: u32,
        offset: u32,
    ) -> Option<Page<PlaylistTrack>> {
        let user = self.user.clone();
        self.api_with_retry(|api| {
            api.user_playlist_tracks(&user, playlist_id, None, limit, offset, None)
        })
    }

    pub fn full_album(&self, album_id: &str) -> Option<FullAlbum> {
        self.api_with_retry(|api| api.album(album_id))
    }

    pub fn artist_albums(
        &self,
        artist_id: &str,
        limit: u32,
        offset: u32,
    ) -> Option<Page<SimplifiedAlbum>> {
        self.api_with_retry(|api| {
            api.artist_albums(artist_id, None, None, Some(limit), Some(offset))
        })
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

    pub fn artist_top_tracks(&self, id: String) -> Option<Vec<Track>> {
        self.api_with_retry(|api| api.artist_top_tracks(&id, None))
            .map(|ft| ft.tracks.iter().map(|t| t.into()).collect())
    }

    pub fn artist_related_artists(&self, id: String) -> Option<Vec<Artist>> {
        self.api_with_retry(|api| api.artist_related_artists(&id))
            .map(|fa| fa.artists.iter().map(|a| a.into()).collect())
    }

    pub fn current_user(&self) -> Option<PrivateUser> {
        self.api_with_retry(|api| api.current_user())
    }

    pub fn load(&self, track: &Track) {
        info!("loading track: {:?}", track);
        self.channel
            .unbounded_send(WorkerCommand::Load(Box::new(track.clone())))
            .unwrap();
    }

    pub fn update_status(&self, new_status: PlayerEvent) {
        match new_status {
            PlayerEvent::Paused => {
                self.set_elapsed(Some(self.get_current_progress()));
                self.set_since(None);
            }
            PlayerEvent::Playing => {
                self.set_since(Some(SystemTime::now()));
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
        self.channel.unbounded_send(WorkerCommand::Play).unwrap();
    }

    pub fn toggleplayback(&self) {
        let status = self
            .status
            .read()
            .expect("could not acquire read lock on player state");
        match *status {
            PlayerEvent::Playing => self.pause(),
            PlayerEvent::Paused => self.play(),
            _ => (),
        }
    }

    pub fn pause(&self) {
        info!("pause()");
        self.channel.unbounded_send(WorkerCommand::Pause).unwrap();
    }

    pub fn stop(&self) {
        info!("stop()");
        self.channel.unbounded_send(WorkerCommand::Stop).unwrap();
    }

    pub fn seek(&self, position_ms: u32) {
        self.set_elapsed(Some(Duration::from_millis(position_ms.into())));
        self.set_since(if self.get_current_status() == PlayerEvent::Playing {
            Some(SystemTime::now())
        } else {
            None
        });

        self.channel
            .unbounded_send(WorkerCommand::Seek(position_ms))
            .unwrap();
    }

    pub fn seek_relative(&self, delta: i32) {
        let progress = self.get_current_progress();
        let new = (progress.as_secs() * 1000) as i32 + progress.subsec_millis() as i32 + delta;
        self.seek(std::cmp::max(0, new) as u32);
    }

    pub fn volume(&self) -> u16 {
        self.volume.load(Ordering::Relaxed) as u16
    }

    fn to_log_scale(volume: u16) -> u16 {
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
        self.volume.store(volume, Ordering::Relaxed);
        self.channel
            .unbounded_send(WorkerCommand::SetVolume(Self::to_log_scale(volume)))
            .unwrap();
    }
}

pub enum URIType {
    Album,
    Artist,
    Track,
    Playlist,
}

impl URIType {
    pub fn from_uri(s: &str) -> Option<URIType> {
        if s.starts_with("spotify:album:") {
            Some(URIType::Album)
        } else if s.starts_with("spotify:artist:") {
            Some(URIType::Artist)
        } else if s.starts_with("spotify:track:") {
            Some(URIType::Track)
        } else if s.starts_with("spotify:user:") && s.contains(":playlist:") {
            Some(URIType::Playlist)
        } else {
            None
        }
    }
}
