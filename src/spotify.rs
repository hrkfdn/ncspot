use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::config::SessionConfig;
use librespot_core::session::Session;
use librespot_core::session::SessionError;
use librespot_playback::config::PlayerConfig;
use librespot_playback::mixer::softmixer::SoftMixer;
use librespot_playback::mixer::MixerConfig;
use log::{debug, error, info};

use librespot_playback::audio_backend;
use librespot_playback::config::Bitrate;
use librespot_playback::player::Player;

use futures::channel::oneshot;
use tokio::sync::mpsc;

use url::Url;

use std::env;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use crate::config;
use crate::events::{Event, EventManager};
use crate::model::playable::Playable;
use crate::spotify_api::WebApi;
use crate::spotify_worker::{Worker, WorkerCommand};

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
    pub api: WebApi,
    elapsed: Arc<RwLock<Option<Duration>>>,
    since: Arc<RwLock<Option<SystemTime>>>,
    token_issued: Arc<RwLock<Option<SystemTime>>>,
    channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
    user: Option<String>,
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
            api: WebApi::new(),
            elapsed: Arc::new(RwLock::new(None)),
            since: Arc::new(RwLock::new(None)),
            token_issued: Arc::new(RwLock::new(None)),
            channel: Arc::new(RwLock::new(None)),
            user: None,
        };

        let (user_tx, user_rx) = oneshot::channel();
        spotify.start_worker(Some(user_tx));
        spotify.user = futures::executor::block_on(user_rx).ok();
        let volume = cfg.state().volume;
        spotify.set_volume(volume);

        spotify.api.set_worker_channel(spotify.channel.clone());
        spotify.api.update_token();

        spotify.api.set_user(spotify.user.clone());

        spotify
    }

    pub fn start_worker(&self, user_tx: Option<oneshot::Sender<String>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        *self
            .channel
            .write()
            .expect("can't writelock worker channel") = Some(tx);
        {
            let worker_channel = self.channel.clone();
            let cfg = self.cfg.clone();
            let events = self.events.clone();
            let volume = self.volume();
            let credentials = self.credentials.clone();
            let handle = tokio::runtime::Handle::current();
            handle.spawn(async move {
                Self::worker(
                    worker_channel,
                    events,
                    rx,
                    cfg.clone(),
                    credentials,
                    user_tx,
                    volume,
                )
                .await
            });
        }
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

    pub fn test_credentials(credentials: Credentials) -> Result<Session, SessionError> {
        let config = Self::session_config();
        let handle = tokio::runtime::Handle::current();
        let jh = handle.spawn(async { Session::connect(config, credentials, None).await });
        futures::executor::block_on(jh).unwrap()
    }

    async fn create_session(
        cfg: &config::Config,
        credentials: Credentials,
    ) -> Result<Session, SessionError> {
        let session_config = Self::session_config();
        let audio_cache_path = match cfg.values().audio_cache.unwrap_or(true) {
            true => Some(config::cache_path("librespot").join("files")),
            false => None,
        };
        let cache = Cache::new(
            Some(config::cache_path("librespot")),
            audio_cache_path,
            cfg.values()
                .audio_cache_size
                .map(|size| (size * 1048576) as u64),
        )
        .expect("Could not create cache");
        debug!("opening spotify session");
        Session::connect(session_config, credentials, Some(cache)).await
    }

    async fn worker(
        worker_channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
        events: EventManager,
        commands: mpsc::UnboundedReceiver<WorkerCommand>,
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
            ..Default::default()
        };

        let session = Self::create_session(&cfg, credentials)
            .await
            .expect("Could not create session");
        user_tx.map(|tx| tx.send(session.username()));

        let create_mixer = librespot_playback::mixer::find(Some(SoftMixer::NAME))
            .expect("could not create softvol mixer");
        let mixer = create_mixer(MixerConfig::default());
        mixer.set_volume(volume);

        let backend = audio_backend::find(cfg.values().backend.clone()).unwrap();
        let audio_format: librespot_playback::config::AudioFormat = Default::default();
        let (player, player_events) = Player::new(
            player_config,
            session.clone(),
            mixer.get_audio_filter(),
            move || (backend)(cfg.values().backend_device.clone(), audio_format),
        );

        let mut worker = Worker::new(
            events.clone(),
            player_events,
            commands,
            session,
            player,
            mixer,
        );
        debug!("worker thread ready.");
        worker.run_loop().await;

        error!("worker thread died, requesting restart");
        *worker_channel
            .write()
            .expect("can't writelock worker channel") = None;
        events.send(Event::SessionDied)
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
            Some(channel) => channel.send(cmd).expect("can't send message to worker"),
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

    pub fn set_volume(&self, volume: u16) {
        info!("setting volume to {}", volume);
        self.cfg.with_state_mut(|mut s| s.volume = volume);
        self.send_worker(WorkerCommand::SetVolume(volume));
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
