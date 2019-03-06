use librespot::core::authentication::Credentials;
use librespot::core::config::SessionConfig;
use librespot::core::keymaster::get_token;
use librespot::core::keymaster::Token;
use librespot::core::session::Session;
use librespot::playback::config::PlayerConfig;

use librespot::playback::audio_backend;
use librespot::playback::config::Bitrate;
use librespot::playback::player::Player;

use rspotify::spotify::client::Spotify as SpotifyAPI;
use rspotify::spotify::model::page::Page;
use rspotify::spotify::model::playlist::{PlaylistTrack, SimplifiedPlaylist};
use rspotify::spotify::model::search::SearchTracks;

use failure::Error;

use futures;
use futures::sync::mpsc;
use futures::sync::oneshot;
use futures::Async;
use futures::Future;
use futures::Stream;
use tokio_core::reactor::Core;

use std::sync::RwLock;
use std::thread;
use std::time::{Duration, SystemTime};

use events::{Event, EventManager};
use track::Track;

enum WorkerCommand {
    Load(Track),
    Play,
    Pause,
    Stop,
}

#[derive(Clone, PartialEq)]
pub enum PlayerEvent {
    Playing,
    Paused,
    Stopped,
    FinishedTrack,
}

pub struct Spotify {
    status: RwLock<PlayerEvent>,
    pub api: SpotifyAPI,
    elapsed: RwLock<Option<Duration>>,
    since: RwLock<Option<SystemTime>>,
    channel: mpsc::UnboundedSender<WorkerCommand>,
    user: String,
}

struct Worker {
    events: EventManager,
    commands: mpsc::UnboundedReceiver<WorkerCommand>,
    player: Player,
    play_task: Box<futures::Future<Item = (), Error = oneshot::Canceled>>,
}

impl Worker {
    fn new(
        events: EventManager,
        commands: mpsc::UnboundedReceiver<WorkerCommand>,
        player: Player,
    ) -> Worker {
        Worker {
            events: events,
            commands: commands,
            player: player,
            play_task: Box::new(futures::empty()),
        }
    }
}

impl futures::Future for Worker {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> futures::Poll<(), ()> {
        loop {
            let mut progress = false;

            trace!("Worker is polling");
            if let Async::Ready(Some(cmd)) = self.commands.poll().unwrap() {
                progress = true;
                debug!("message received!");
                match cmd {
                    WorkerCommand::Load(track) => {
                        self.play_task = Box::new(self.player.load(track.id, false, 0));
                        info!("player loading track: {:?}", track);
                    }
                    WorkerCommand::Play => {
                        self.player.play();
                        self.events.send(Event::Player(PlayerEvent::Playing));
                    }
                    WorkerCommand::Pause => {
                        self.player.pause();
                        self.events.send(Event::Player(PlayerEvent::Paused));
                    }
                    WorkerCommand::Stop => {
                        self.player.stop();
                        self.events.send(Event::Player(PlayerEvent::Stopped));
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

            info!("worker done");
            if !progress {
                trace!("handing executor to other tasks");
                return Ok(Async::NotReady);
            }
        }
    }
}

impl Spotify {
    pub fn new(events: EventManager, user: String, password: String, client_id: String) -> Spotify {
        let session_config = SessionConfig::default();
        let player_config = PlayerConfig {
            bitrate: Bitrate::Bitrate320,
            normalisation: false,
            normalisation_pregain: 0.0,
        };
        let credentials = Credentials::with_password(user.clone(), password.clone());

        let (tx, rx) = mpsc::unbounded();
        let (p, c) = oneshot::channel();
        {
            let events = events.clone();
            thread::spawn(move || {
                Self::worker(
                    events,
                    rx,
                    p,
                    session_config,
                    player_config,
                    credentials,
                    client_id,
                )
            });
        }

        let token = c.wait().unwrap();
        debug!("token received: {:?}", token);
        let api = SpotifyAPI::default().access_token(&token.access_token);

        Spotify {
            status: RwLock::new(PlayerEvent::Stopped),
            api: api,
            elapsed: RwLock::new(None),
            since: RwLock::new(None),
            channel: tx,
            user: user,
        }
    }

    fn worker(
        events: EventManager,
        commands: mpsc::UnboundedReceiver<WorkerCommand>,
        token_channel: oneshot::Sender<Token>,
        session_config: SessionConfig,
        player_config: PlayerConfig,
        credentials: Credentials,
        client_id: String,
    ) {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let session = core
            .run(Session::connect(session_config, credentials, None, handle))
            .ok()
            .unwrap();

        let scopes = "user-read-private,playlist-read-private,playlist-read-collaborative,playlist-modify-public,playlist-modify-private,user-follow-modify,user-follow-read,user-library-read,user-library-modify,user-top-read,user-read-recently-played";
        let token = core.run(get_token(&session, &client_id, &scopes)).unwrap();
        token_channel.send(token).unwrap();

        let backend = audio_backend::find(None).unwrap();
        let (player, _eventchannel) =
            Player::new(player_config, session, None, move || (backend)(None));

        let worker = Worker::new(events, commands, player);
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
        (*elapsed).clone()
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
        (*since).clone()
    }

    pub fn search(&self, query: &str, limit: u32, offset: u32) -> Result<SearchTracks, Error> {
        self.api.search_track(query, limit, offset, None)
    }

    pub fn current_user_playlist(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Page<SimplifiedPlaylist>, Error> {
        self.api.current_user_playlists(limit, offset)
    }

    pub fn user_playlist_tracks(&self, playlist_id: &str) -> Result<Page<PlaylistTrack>, Error> {
        self.api
            .user_playlist_tracks(&self.user, playlist_id, None, 50, 0, None)
    }

    pub fn load(&self, track: &Track) {
        info!("loading track: {:?}", track);
        self.channel
            .unbounded_send(WorkerCommand::Load(track.clone()))
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
}
