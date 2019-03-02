use librespot::core::authentication::Credentials;
use librespot::core::config::SessionConfig;
use librespot::core::keymaster::get_token;
use librespot::core::keymaster::Token;
use librespot::core::session::Session;
use librespot::core::spotify_id::SpotifyId;
use librespot::playback::config::PlayerConfig;

use librespot::playback::audio_backend;
use librespot::playback::config::Bitrate;
use librespot::playback::player::Player;

use rspotify::spotify::client::Spotify as SpotifyAPI;
use rspotify::spotify::model::search::SearchTracks;

use failure::Error;

use futures;
use futures::sync::mpsc;
use futures::sync::oneshot;
use futures::Async;
use futures::Future;
use futures::Stream;
use tokio_core::reactor::Core;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use std::thread;

use events::{Event, EventManager};
use queue::Queue;

enum WorkerCommand {
    Load(SpotifyId),
    Play,
    Pause,
    Stop,
}

pub enum PlayerState {
    Playing,
    Paused,
    Stopped,
}

pub struct Spotify {
    pub state: RwLock<PlayerState>,
    pub api: SpotifyAPI,
    channel: mpsc::UnboundedSender<WorkerCommand>,
    events: EventManager,
}

struct Worker {
    events: EventManager,
    commands: mpsc::UnboundedReceiver<WorkerCommand>,
    player: Player,
    play_task: Box<futures::Future<Item = (), Error = oneshot::Canceled>>,
    queue: Arc<Mutex<Queue>>,
}

impl Worker {
    fn new(
        events: EventManager,
        commands: mpsc::UnboundedReceiver<WorkerCommand>,
        player: Player,
        queue: Arc<Mutex<Queue>>,
    ) -> Worker {
        Worker {
            events: events,
            commands: commands,
            player: player,
            play_task: Box::new(futures::empty()),
            queue: queue,
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
                        self.play_task = Box::new(self.player.load(track, false, 0));
                        info!("player loading track..");
                    }
                    WorkerCommand::Play => {
                        self.player.play();
                        self.events.send(Event::PlayState(PlayerState::Playing));
                    },
                    WorkerCommand::Pause => {
                        self.player.pause();
                        self.events.send(Event::PlayState(PlayerState::Paused));
                    }
                    WorkerCommand::Stop => {
                        self.player.stop();
                        self.events.send(Event::PlayState(PlayerState::Stopped));
                    }
                }
            }
            match self.play_task.poll() {
                Ok(Async::Ready(())) => {
                    debug!("end of track!");
                    progress = true;

                    let mut queue = self.queue.lock().unwrap();
                    if let Some(track) = queue.dequeue() {
                        debug!("next track in queue: {}", track.name);
                        let trackid =
                            SpotifyId::from_base62(&track.id).expect("could not load track");
                        self.play_task = Box::new(self.player.load(trackid, false, 0));
                        self.player.play();

                        self.events.send(Event::PlayState(PlayerState::Playing));
                    }
                    else {
                        self.events.send(Event::PlayState(PlayerState::Stopped));
                    }
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
    pub fn new(
        events: EventManager,
        user: String,
        password: String,
        client_id: String,
        queue: Arc<Mutex<Queue>>,
    ) -> Spotify {
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
                Spotify::worker(
                    events,
                    rx,
                    p,
                    session_config,
                    player_config,
                    credentials,
                    client_id,
                    queue,
                )
            });
        }

        let token = c.wait().unwrap();
        debug!("token received: {:?}", token);
        let api = SpotifyAPI::default().access_token(&token.access_token);

        Spotify {
            state: RwLock::new(PlayerState::Stopped),
            api: api,
            channel: tx,
            events: events,
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
        queue: Arc<Mutex<Queue>>,
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

        let worker = Worker::new(events, commands, player, queue);
        debug!("worker thread ready.");
        core.run(worker).unwrap();
        debug!("worker thread finished.");
    }

    pub fn search(&self, query: &str, limit: u32, offset: u32) -> Result<SearchTracks, Error> {
        self.api.search_track(query, limit, offset, None)
    }

    pub fn load(&self, track: SpotifyId) {
        info!("loading track: {:?}", track);
        self.channel
            .unbounded_send(WorkerCommand::Load(track))
            .unwrap();
    }

    pub fn updatestate(&self, newstate: PlayerState) {
        let mut state = self.state.write().expect("could not acquire write lock on player state");
        *state = newstate;
    }

    pub fn play(&self) {
        info!("play()");
        self.channel.unbounded_send(WorkerCommand::Play).unwrap();
    }

    pub fn toggleplayback(&self) {
        let state = self.state.read().expect("could not acquire read lock on player state");
        match *state {
            PlayerState::Playing => self.pause(),
            PlayerState::Paused => self.play(),
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
