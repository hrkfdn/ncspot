use crate::events::{Event, EventManager};
use crate::model::playable::Playable;
use crate::queue::QueueEvent;
use crate::spotify::PlayerEvent;
use librespot_connect::{ConnectConfig, LoadRequest, LoadRequestOptions, Spirc};
use librespot_core::SpotifyUri;
use librespot_core::session::Session;
use librespot_playback::mixer::Mixer;
use librespot_playback::player::{Player, PlayerEvent as LibrespotPlayerEvent};
use log::{debug, error, info, warn};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use tokio::sync::mpsc;
use tokio::time;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Debug)]
pub(crate) enum WorkerCommand {
    Load(Playable, bool, u32),
    Play,
    Pause,
    Stop,
    Seek(u32),
    SetVolume(u16),
    Preload(Playable),
    Shutdown,
}

enum PlayerStatus {
    Playing,
    Paused,
    Stopped,
}

pub struct Worker {
    events: EventManager,
    player_events: UnboundedReceiverStream<LibrespotPlayerEvent>,
    commands: UnboundedReceiverStream<WorkerCommand>,
    player_status: PlayerStatus,
    session: Session,
    spirc: Spirc,
    spirc_task: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Worker {
    pub(crate) async fn new(
        events: EventManager,
        credentials: librespot_core::authentication::Credentials,
        player_events: mpsc::UnboundedReceiver<LibrespotPlayerEvent>,
        commands: mpsc::UnboundedReceiver<WorkerCommand>,
        session: Session,
        player: Arc<Player>,
        mixer: Arc<dyn Mixer>,
    ) -> Self {
        let config = ConnectConfig {
            name: "ncspot".to_string(),
            ..Default::default()
        };
        let (spirc, spirc_task) = Spirc::new(config, session.clone(), credentials, player, mixer)
            .await
            .expect("Spirc should be initialized");
        Self {
            events,
            player_events: UnboundedReceiverStream::new(player_events),
            commands: UnboundedReceiverStream::new(commands),
            player_status: PlayerStatus::Stopped,
            session,
            spirc,
            spirc_task: Box::pin(spirc_task),
        }
    }

    pub async fn run_loop(&mut self) {
        let mut ui_refresh = time::interval(Duration::from_millis(400));

        loop {
            if self.session.is_invalid() {
                info!("Librespot session invalidated, terminating worker");
                self.events.send(Event::Player(PlayerEvent::Stopped));
                break;
            }

            tokio::select! {
                cmd = self.commands.next() => match cmd {
                    Some(WorkerCommand::Load(playable, start_playing, position_ms)) => {
                        if let Err(e) = self.spirc.activate() {
                            warn!("error activating spirc: {e:?}");
                        }
                        match SpotifyUri::from_uri(&playable.uri()) {
                            Ok(uri) => {
                                info!("player loading track: {uri:?}");
                                if !uri.is_playable() {
                                    warn!("track is not playable");
                                    self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                                } else {
                                    let options = LoadRequestOptions {
                                        start_playing,
                                        seek_to: position_ms,
                                        context_options: None,
                                        playing_track: None,
                                    };
                                    let req =
                                        LoadRequest::from_tracks(vec![uri.to_uri().expect("uri")], options);
                                    if let Err(e) = self.spirc.load(req) {
                                        error!("error loading track into spirc: {e:?}");
                                        self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                                    }
                                }
                            }
                            Err(e) => {
                                error!("error parsing uri: {e:?}");
                                self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                            }
                        }
                    }
                    Some(WorkerCommand::Play) => {
                        if let Err(e) = self.spirc.play() {
                            error!("error resuming spirc: {e:?}");
                        }
                    }
                    Some(WorkerCommand::Pause) => {
                        if let Err(e) = self.spirc.pause() {
                            error!("error pausing spirc: {e:?}");
                        }
                    }
                    Some(WorkerCommand::Stop) => {
                        //todo!("stop spirc");
                    }
                    Some(WorkerCommand::Seek(pos)) => {
                        if let Err(e) = self.spirc.set_position_ms(pos) {
                            error!("error seeking spirc: {e:?}");
                        }
                    }
                    Some(WorkerCommand::SetVolume(volume)) => {
                        if let Err(e) = self.spirc.set_volume(volume) {
                            error!("error setting spirc volume: {e:?}");
                        }
                    }
                    Some(WorkerCommand::Preload(playable)) => {
                        if let Ok(uri) = SpotifyUri::from_uri(&playable.uri()) {
                            debug!("Preloading {uri:?}");
                        }
                    }
                    Some(WorkerCommand::Shutdown) => {
                        if let Err(e) = self.spirc.shutdown() {
                            error!("error shutting down spirc: {e:?}");
                        }
                    }
                    None => info!("empty stream")
                },
                event = self.player_events.next() => match event {
                    Some(LibrespotPlayerEvent::Playing {
                        play_request_id: _,
                        track_id: _,
                        position_ms,
                    }) => {
                        let position = Duration::from_millis(position_ms as u64);
                        let playback_start = SystemTime::now() - position;
                        self.events
                            .send(Event::Player(PlayerEvent::Playing(playback_start)));
                        self.player_status = PlayerStatus::Playing;
                    }
                    Some(LibrespotPlayerEvent::Paused {
                        play_request_id: _,
                        track_id: _,
                        position_ms,
                    }) => {
                        let position = Duration::from_millis(position_ms as u64);
                        self.events
                            .send(Event::Player(PlayerEvent::Paused(position)));
                        self.player_status = PlayerStatus::Paused;
                    }
                    Some(LibrespotPlayerEvent::Stopped { .. }) => {
                        self.events.send(Event::Player(PlayerEvent::Stopped));
                        self.player_status = PlayerStatus::Stopped;
                    }
                    Some(LibrespotPlayerEvent::EndOfTrack { .. }) => {
                        self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                    }
                    Some(LibrespotPlayerEvent::TimeToPreloadNextTrack { .. }) => {
                        self.events
                            .send(Event::Queue(QueueEvent::PreloadTrackRequest));
                    }
                    Some(LibrespotPlayerEvent::Seeked { play_request_id: _, track_id: _, position_ms}) => {
                        let position = Duration::from_millis(position_ms as u64);
                        let event = match self.player_status {
                            PlayerStatus::Playing => {
                                let playback_start = SystemTime::now() - position;
                                PlayerEvent::Playing(playback_start)
                            },
                            PlayerStatus::Paused => PlayerEvent::Paused(position),
                            PlayerStatus::Stopped => PlayerEvent::Stopped,
                        };
                        self.events.send(Event::Player(event));
                    }
                    Some(event) => {
                        debug!("Unhandled player event: {event:?}");
                    }
                    None => {
                        warn!("Librespot player event channel died, terminating worker");
                        break
                    },
                },
                _ = self.spirc_task.as_mut() => {
                    info!("spirc task tick");
                },
                // Update animated parts of the UI (e.g. statusbar during playback).
                _ = ui_refresh.tick() => {
                    if !matches!(self.player_status, PlayerStatus::Stopped) {
                        self.events.trigger();
                    }
                },
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        debug!("Worker thread is shutting down, stopping player");
        let _ = self.spirc.shutdown();
    }
}
