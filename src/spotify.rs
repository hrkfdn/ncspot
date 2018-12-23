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

use std::thread;

enum WorkerCommand {
    Load(SpotifyId),
    Play,
    Pause,
    Stop,
}

pub struct Spotify {
    pub api: SpotifyAPI,
    channel: mpsc::UnboundedSender<WorkerCommand>,
}

struct Worker {
    commands: mpsc::UnboundedReceiver<WorkerCommand>,
    player: Player,
    play_task: Box<futures::Future<Item = (), Error = oneshot::Canceled>>,
}

impl Worker {
    fn new(commands: mpsc::UnboundedReceiver<WorkerCommand>, player: Player) -> Worker {
        Worker {
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
                        self.play_task = Box::new(self.player.load(track, false, 0));
                        info!("player loading track..");
                    }
                    WorkerCommand::Play => self.player.play(),
                    WorkerCommand::Pause => self.player.pause(),
                    WorkerCommand::Stop => self.player.stop(),
                }
            }
            match self.play_task.poll() {
                Ok(Async::Ready(())) => {
                    debug!("end of track!");
                    progress = true;
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
    pub fn new(user: String, password: String, client_id: String) -> Spotify {
        let session_config = SessionConfig::default();
        let player_config = PlayerConfig {
            bitrate: Bitrate::Bitrate320,
            normalisation: false,
            normalisation_pregain: 0.0,
        };
        let credentials = Credentials::with_password(user.clone(), password.clone());

        let (tx, rx) = mpsc::unbounded();
        let (p, c) = oneshot::channel();
        thread::spawn(move || {
            Spotify::worker(rx, p, session_config, player_config, credentials, client_id)
        });

        let token = c.wait().unwrap();
        debug!("token received: {:?}", token);
        let api = SpotifyAPI::default().access_token(&token.access_token);

        Spotify {
            api: api,
            channel: tx,
        }
    }

    fn worker(
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

        let worker = Worker::new(commands, player);
        debug!("worker thread ready.");
        core.run(worker).unwrap();
        debug!("worker thread finished.");
    }

    pub fn run(&mut self) {
        println!("Spotify::run() finished");
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

    pub fn play(&self) {
        info!("play()");
        self.channel.unbounded_send(WorkerCommand::Play).unwrap();
    }

    pub fn pause(&mut self) {
        info!("pause()");
        self.channel.unbounded_send(WorkerCommand::Pause).unwrap();
    }

    pub fn stop(&mut self) {
        info!("stop()");
        self.channel.unbounded_send(WorkerCommand::Stop).unwrap();
    }
}
