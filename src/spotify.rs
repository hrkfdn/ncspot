use std::error::Error;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};
use std::{env, fmt};

use futures::channel::oneshot;
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::config::SessionConfig;
use librespot_core::session::Session;
use librespot_playback::audio_backend;
use librespot_playback::audio_backend::SinkBuilder;
use librespot_playback::config::Bitrate;
use librespot_playback::config::PlayerConfig;
use librespot_playback::mixer::MixerConfig;
use librespot_playback::mixer::softmixer::SoftMixer;
use librespot_playback::player::Player;
use log::{debug, error, info, warn};
use tokio::sync::mpsc;
use url::Url;

use crate::application::ASYNC_RUNTIME;
use crate::authentication::SPOTIFY_CLIENT_ID;
use crate::config;
use crate::events::{Event, EventManager};
use crate::model::playable::Playable;
#[cfg(feature = "mpris")]
use crate::mpris::{MprisCommand, MprisManager};
use crate::spotify_api::WebApi;
use crate::spotify_worker::{Worker, WorkerCommand};
use crate::traits::ListItem;

/// One percent of the maximum supported [Player] volume, used when setting the volume to a certain
/// percent.
pub const VOLUME_PERCENT: u16 = ((u16::MAX as f64) * 1.0 / 100.0) as u16;

/// Events sent by the [Player].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum PlayerEvent {
    Playing(SystemTime),
    Paused(Duration),
    Stopped,
    FinishedTrack,
}

/// Wrapper around a worker thread that exposes methods to safely control it.
#[derive(Clone)]
pub struct Spotify {
    events: EventManager,
    /// The credentials for the currently logged in user, used to authenticate to the Spotify API.
    #[cfg(feature = "mpris")]
    mpris: Arc<std::sync::Mutex<Option<MprisManager>>>,
    credentials: Credentials,
    cfg: Arc<config::Config>,
    /// Playback status of the [Player] owned by the worker thread.
    status: Arc<RwLock<PlayerEvent>>,
    pub api: WebApi,
    /// The amount of the current [Playable] that had elapsed when last paused.
    elapsed: Arc<RwLock<Option<Duration>>>,
    /// The amount of the current [Playable] that has been played in total.
    since: Arc<RwLock<Option<SystemTime>>>,
    /// Channel to send commands to the worker thread.
    channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
}

impl Spotify {
    pub fn new(
        events: EventManager,
        credentials: Credentials,
        cfg: Arc<config::Config>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut spotify = Self {
            events,
            #[cfg(feature = "mpris")]
            mpris: Default::default(),
            credentials,
            cfg: cfg.clone(),
            status: Arc::new(RwLock::new(PlayerEvent::Stopped)),
            api: WebApi::new(),
            elapsed: Arc::new(RwLock::new(None)),
            since: Arc::new(RwLock::new(None)),
            channel: Arc::new(RwLock::new(None)),
        };

        let (user_tx, user_rx) = oneshot::channel();
        spotify.start_worker(Some(user_tx))?;
        let user = ASYNC_RUNTIME.get().unwrap().block_on(user_rx).ok();
        let volume = cfg.state().volume;
        spotify.set_volume(volume, true);

        spotify.api.set_worker_channel(spotify.channel.clone());
        spotify
            .api
            .update_token()
            .map(move |h| ASYNC_RUNTIME.get().unwrap().block_on(h).ok());

        spotify.api.set_user(user);

        Ok(spotify)
    }

    /// Start the worker thread. If `user_tx` is given, it will receive the username of the logged
    /// in user.
    pub fn start_worker(
        &self,
        user_tx: Option<oneshot::Sender<String>>,
    ) -> Result<(), Box<dyn Error>> {
        let (tx, rx) = mpsc::unbounded_channel();
        *self.channel.write().unwrap() = Some(tx);
        let worker_channel = self.channel.clone();
        let cfg = self.cfg.clone();
        let events = self.events.clone();
        let volume = self.volume();
        let credentials = self.credentials.clone();
        let backend_name = cfg.values().backend.clone();
        let backend = Self::init_backend(backend_name)?;
        ASYNC_RUNTIME.get().unwrap().spawn(Self::worker(
            worker_channel,
            events,
            rx,
            cfg,
            credentials,
            user_tx,
            volume,
            backend,
        ));
        Ok(())
    }

    /// Generate the librespot [SessionConfig] used when creating a [Session].
    pub fn session_config(cfg: &config::Config) -> SessionConfig {
        let mut session_config = librespot_core::SessionConfig {
            client_id: SPOTIFY_CLIENT_ID.to_string(),
            ..Default::default()
        };
        match env::var("http_proxy") {
            Ok(proxy) => {
                info!("Setting HTTP proxy {proxy}");
                session_config.proxy = Url::parse(&proxy).ok();
            }
            Err(_) => debug!("No HTTP proxy set"),
        }
        if let Some(ap_port) = cfg.values().ap_port {
            session_config.ap_port = Some(ap_port)
        }
        session_config
    }

    pub fn test_credentials(
        cfg: &config::Config,
        credentials: Credentials,
    ) -> Result<Session, librespot_core::Error> {
        let config = Self::session_config(cfg);
        let _guard = ASYNC_RUNTIME.get().unwrap().enter();
        let session = Session::new(config, None);
        ASYNC_RUNTIME
            .get()
            .unwrap()
            .block_on(session.connect(credentials, true))
            .map(|_| session)
    }

    /// Create a [Session] that respects the user configuration in `cfg` and with the given
    /// credentials.
    async fn create_session(
        cfg: &config::Config,
        credentials: Credentials,
    ) -> Result<Session, librespot_core::Error> {
        let librespot_cache_path = config::cache_path("librespot");
        let audio_cache_path = match cfg.values().audio_cache {
            Some(false) => None,
            _ => Some(librespot_cache_path.join("files")),
        };
        let cache = Cache::new(
            Some(librespot_cache_path.clone()),
            Some(librespot_cache_path.join("volume")),
            audio_cache_path,
            cfg.values()
                .audio_cache_size
                .map(|size| size as u64 * 1048576),
        )
        .expect("Could not create cache");
        debug!("opening spotify session");
        let session_config = Self::session_config(cfg);
        let session = Session::new(session_config, Some(cache));
        session.connect(credentials, true).await.map(|_| session)
    }

    /// Create and initialize the requested audio backend.
    fn init_backend(desired_backend: Option<String>) -> Result<SinkBuilder, Box<dyn Error>> {
        let backend = if let Some(name) = desired_backend {
            audio_backend::BACKENDS
                .iter()
                .find(|backend| name == backend.0)
                .ok_or(format!(
                    r#"configured audio backend "{name}" can't be found"#
                ))?
        } else {
            audio_backend::BACKENDS
                .first()
                .ok_or("no available audio backends found")?
        };

        let backend_name = backend.0;

        info!("Initializing audio backend {backend_name}");
        if backend_name == "pulseaudio" {
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { env::set_var("PULSE_PROP_application.name", "ncspot") };
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { env::set_var("PULSE_PROP_stream.description", "ncurses Spotify client") };
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { env::set_var("PULSE_PROP_media.role", "music") };
        }

        Ok(backend.1)
    }

    /// Create and run the worker thread.
    #[allow(clippy::too_many_arguments)]
    async fn worker(
        worker_channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
        events: EventManager,
        commands: mpsc::UnboundedReceiver<WorkerCommand>,
        cfg: Arc<config::Config>,
        credentials: Credentials,
        user_tx: Option<oneshot::Sender<String>>,
        volume: u16,
        backend: SinkBuilder,
    ) {
        let bitrate_str = cfg.values().bitrate.unwrap_or(320).to_string();
        let bitrate = Bitrate::from_str(&bitrate_str);
        if bitrate.is_err() {
            error!("invalid bitrate, will use 320 instead")
        }

        let player_config = PlayerConfig {
            gapless: cfg.values().gapless.unwrap_or(true),
            bitrate: bitrate.unwrap_or(Bitrate::Bitrate320),
            normalisation: cfg.values().volnorm.unwrap_or(false),
            normalisation_pregain_db: cfg.values().volnorm_pregain.unwrap_or(0.0),
            ..Default::default()
        };

        let session = Self::create_session(&cfg, credentials)
            .await
            .expect("Could not create session");
        user_tx.map(|tx| tx.send(session.username()));

        let mixer_factory_opt = librespot_playback::mixer::find(Some(SoftMixer::NAME));
        let factory = mixer_factory_opt.expect("could not find softvol mixer factory");
        let mixer = factory(MixerConfig::default()).expect("could not create softvol mixer");

        mixer.set_volume(volume);

        let audio_format: librespot_playback::config::AudioFormat = Default::default();
        let player = Player::new(
            player_config,
            session.clone(),
            mixer.get_soft_volume(),
            move || (backend)(cfg.values().backend_device.clone(), audio_format),
        );
        let player_events = player.get_player_event_channel();

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
        *worker_channel.write().unwrap() = None;
        events.send(Event::SessionDied)
    }

    /// Get the current playback status of the [Player].
    pub fn get_current_status(&self) -> PlayerEvent {
        let status = self.status.read().unwrap();
        (*status).clone()
    }

    /// Get the total amount of the current [Playable] that has been played.
    pub fn get_current_progress(&self) -> Duration {
        self.get_elapsed().unwrap_or_else(|| Duration::from_secs(0))
            + self
                .get_since()
                .map(|t| t.elapsed().unwrap())
                .unwrap_or_else(|| Duration::from_secs(0))
    }

    fn set_elapsed(&self, new_elapsed: Option<Duration>) {
        let mut elapsed = self.elapsed.write().unwrap();
        *elapsed = new_elapsed;
    }

    fn get_elapsed(&self) -> Option<Duration> {
        let elapsed = self.elapsed.read().unwrap();
        *elapsed
    }

    fn set_since(&self, new_since: Option<SystemTime>) {
        let mut since = self.since.write().unwrap();
        *since = new_since;
    }

    fn get_since(&self) -> Option<SystemTime> {
        let since = self.since.read().unwrap();
        *since
    }

    /// Load `track` into the [Player]. Start playing immediately if
    /// `start_playing` is true. Start playing from `position_ms` in the song.
    pub fn load(&self, track: &Playable, start_playing: bool, position_ms: u32) {
        info!("loading track: {track:?}");

        if !track.is_playable() {
            warn!("track {track:?} can not be played, skipping..");
            self.events.send(Event::Player(PlayerEvent::FinishedTrack));
            return;
        }

        self.send_worker(WorkerCommand::Load(
            track.clone(),
            start_playing,
            position_ms,
        ));

        #[cfg(feature = "mpris")]
        self.send_mpris(MprisCommand::EmitMetadataStatus);
    }

    /// Update the cached status of the [Player]. This makes sure the status
    /// doesn't have to be retrieved every time from the thread, which would be harder and more
    /// expensive.
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

        let mut status = self.status.write().unwrap();
        *status = new_status;

        #[cfg(feature = "mpris")]
        self.send_mpris(MprisCommand::EmitPlaybackStatus);
    }

    /// Reset the time tracking stats for the current song. This should be called when a new song is
    /// loaded.
    pub fn update_track(&self) {
        self.set_elapsed(None);
        self.set_since(None);
    }

    /// Start playback of the [Player].
    pub fn play(&self) {
        info!("play()");
        self.send_worker(WorkerCommand::Play);
    }

    /// Toggle playback (play/pause) of the [Player].
    pub fn toggleplayback(&self) {
        match self.get_current_status() {
            PlayerEvent::Playing(_) => self.pause(),
            PlayerEvent::Paused(_) => self.play(),
            _ => (),
        }
    }

    /// Send an [MprisCommand] to the mpris thread.
    #[cfg(feature = "mpris")]
    fn send_mpris(&self, cmd: MprisCommand) {
        debug!("Sending mpris command: {cmd:?}");
        match self.mpris.lock().unwrap().as_ref() {
            Some(mpris_manager) => {
                mpris_manager.send(cmd);
            }
            _ => {
                warn!("mpris context is unitialized");
            }
        }
    }

    /// Send a [WorkerCommand] to the worker thread.
    fn send_worker(&self, cmd: WorkerCommand) {
        info!("sending command to worker: {cmd:?}");
        let channel = self.channel.read().unwrap();
        match channel.as_ref() {
            Some(channel) => {
                if let Err(e) = channel.send(cmd) {
                    error!("can't send command to spotify worker: {e}, dropping command");
                }
            }
            None => error!("no channel to worker available"),
        }
    }

    /// Pause playback of the [Player].
    pub fn pause(&self) {
        info!("pause()");
        self.send_worker(WorkerCommand::Pause);
    }

    /// Stop playback of the [Player].
    pub fn stop(&self) {
        info!("stop()");
        self.send_worker(WorkerCommand::Stop);
    }

    /// Seek in the currently played [Playable] played by the [Player].
    pub fn seek(&self, position_ms: u32) {
        self.send_worker(WorkerCommand::Seek(position_ms));
        #[cfg(feature = "mpris")]
        self.notify_seeked(position_ms);
    }

    /// Seek relatively to the current playback position of the [Player].
    pub fn seek_relative(&self, delta: i32) {
        let progress = self.get_current_progress();
        let new = (progress.as_secs() * 1000) as i32 + progress.subsec_millis() as i32 + delta;
        self.seek(std::cmp::max(0, new) as u32);
    }

    /// Get the current volume of the [Player].
    pub fn volume(&self) -> u16 {
        self.cfg.state().volume
    }

    /// Send a Seeked signal on Mpris interface
    #[cfg(feature = "mpris")]
    pub fn notify_seeked(&self, position_ms: u32) {
        let new_position = Duration::from_millis(position_ms.into());
        let command = MprisCommand::EmitSeekedStatus(
            new_position
                .as_micros()
                .try_into()
                .expect("track position exceeds MPRIS datatype"),
        );
        self.send_mpris(command);
    }

    /// Set the current volume of the [Player]. If `notify` is true, also notify MPRIS clients about
    /// the update.
    pub fn set_volume(&self, volume: u16, notify: bool) {
        info!("setting volume to {volume}");
        self.cfg.with_state_mut(|s| s.volume = volume);
        self.send_worker(WorkerCommand::SetVolume(volume));
        // HACK: This is a bit of a hack to prevent duplicate update signals when updating from the
        // MPRIS implementation.
        if notify {
            #[cfg(feature = "mpris")]
            self.send_mpris(MprisCommand::EmitVolumeStatus)
        }
    }

    /// Preload the given [Playable] in the [Player]. This makes sure it can be played immediately
    /// after the current [Playable] is finished.
    pub fn preload(&self, track: &Playable) {
        self.send_worker(WorkerCommand::Preload(track.clone()));
    }

    /// Shut down the worker thread.
    pub fn shutdown(&self) {
        self.send_worker(WorkerCommand::Shutdown);
    }

    #[cfg(feature = "mpris")]
    pub fn set_mpris(&mut self, mpris: MprisManager) {
        *self.mpris.lock().unwrap() = Some(mpris);
    }
}

/// A type of Spotify URI.
#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum UriType {
    Album,
    Artist,
    Track,
    Playlist,
    Show,
    Episode,
}

#[derive(Debug)]
pub struct UriParseError;

impl fmt::Display for UriParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Spotify URI")
    }
}

impl Error for UriParseError {}

impl FromStr for UriType {
    type Err = UriParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("spotify:album:") {
            Ok(Self::Album)
        } else if s.starts_with("spotify:artist:") {
            Ok(Self::Artist)
        } else if s.starts_with("spotify:track:") {
            Ok(Self::Track)
        } else if s.starts_with("spotify:") && s.contains(":playlist:") {
            Ok(Self::Playlist)
        } else if s.starts_with("spotify:show:") {
            Ok(Self::Show)
        } else if s.starts_with("spotify:episode:") {
            Ok(Self::Episode)
        } else {
            Err(UriParseError)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_album_uri() {
        let uri_type = "spotify:album:29F5MF6Q9VYlryDsYEQz6a".parse();
        assert!(matches!(uri_type, Ok(UriType::Album)));
    }

    #[test]
    fn parse_invalid_uri() {
        let uri_type: Result<UriType, _> = "kayava".parse();
        assert!(matches!(uri_type, Err(UriParseError)));
    }

    #[test]

    fn parse_playlist_uri() {
        let uri_type = "spotify:playlist:37i9dQZF1DX36Xw4IJIVKA".parse();
        assert!(matches!(uri_type, Ok(UriType::Playlist)));
    }
}
