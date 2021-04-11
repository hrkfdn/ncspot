use crate::events::{Event, EventManager};
use crate::playable::Playable;
use crate::queue::QueueEvent;
use crate::spotify::{PlayerEvent, Spotify};
use futures::channel::{mpsc, oneshot};
use futures::compat::Stream01CompatExt;
use futures::task::{Context, Poll};
use futures::{Future, Stream};
use futures_01::stream::Stream as v01_Stream;
use futures_01::sync::mpsc::UnboundedReceiver;
use futures_01::Async as v01_Async;
use librespot_core::keymaster::Token;
use librespot_core::mercury::MercuryError;
use librespot_core::session::Session;
use librespot_core::spotify_id::{SpotifyAudioType, SpotifyId};
use librespot_playback::mixer::Mixer;
use librespot_playback::player::{Player, PlayerEvent as LibrespotPlayerEvent};
use std::time::Duration;
use std::{pin::Pin, time::SystemTime};

pub(crate) enum WorkerCommand {
    Load(Playable, bool, u32),
    Play,
    Pause,
    Stop,
    Seek(u32),
    SetVolume(u16),
    RequestToken(oneshot::Sender<Token>),
    Preload(Playable),
    Shutdown,
}

pub struct Worker {
    events: EventManager,
    player_events: UnboundedReceiver<LibrespotPlayerEvent>,
    commands: Pin<Box<mpsc::UnboundedReceiver<WorkerCommand>>>,
    session: Session,
    player: Player,
    refresh_task: Pin<Box<dyn Stream<Item = Result<(), tokio_timer::Error>>>>,
    token_task: Pin<Box<dyn Future<Output = Result<(), MercuryError>>>>,
    active: bool,
    mixer: Box<dyn Mixer>,
}

impl Worker {
    pub(crate) fn new(
        events: EventManager,
        player_events: UnboundedReceiver<LibrespotPlayerEvent>,
        commands: Pin<Box<mpsc::UnboundedReceiver<WorkerCommand>>>,
        session: Session,
        player: Player,
        mixer: Box<dyn Mixer>,
    ) -> Worker {
        Worker {
            events,
            player_events,
            commands,
            player,
            session,
            refresh_task: Box::pin(futures::stream::empty()),
            token_task: Box::pin(futures::future::pending()),
            active: false,
            mixer,
        }
    }
}

impl Worker {
    fn create_refresh(&self) -> Pin<Box<dyn Stream<Item = Result<(), tokio_timer::Error>>>> {
        let ev = self.events.clone();
        let future =
            tokio_timer::Interval::new_interval(Duration::from_millis(400)).map(move |_| {
                ev.trigger();
            });
        Box::pin(future.compat())
    }
}

impl futures::Future for Worker {
    type Output = Result<(), ()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> futures::task::Poll<Self::Output> {
        loop {
            let mut progress = false;

            if self.session.is_invalid() {
                self.events.send(Event::Player(PlayerEvent::Stopped));
                return Poll::Ready(Result::Err(()));
            }

            if let Poll::Ready(Some(cmd)) = self.commands.as_mut().poll_next(cx) {
                progress = true;
                debug!("message received!");
                match cmd {
                    WorkerCommand::Load(playable, start_playing, position_ms) => {
                        match SpotifyId::from_uri(&playable.uri()) {
                            Ok(id) => {
                                info!("player loading track: {:?}", id);
                                if id.audio_type == SpotifyAudioType::NonPlayable {
                                    warn!("track is not playable");
                                    self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                                } else {
                                    self.player.load(id, start_playing, position_ms);
                                }
                            }
                            Err(e) => {
                                error!("error parsing uri: {:?}", e);
                                self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                            }
                        }
                    }
                    WorkerCommand::Play => {
                        self.player.play();
                    }
                    WorkerCommand::Pause => {
                        self.player.pause();
                    }
                    WorkerCommand::Stop => {
                        self.player.stop();
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
                    WorkerCommand::Preload(playable) => {
                        if let Ok(id) = SpotifyId::from_uri(&playable.uri()) {
                            debug!("Preloading {:?}", id);
                            self.player.preload(id);
                        }
                    }
                    WorkerCommand::Shutdown => {
                        self.player.stop();
                        self.session.shutdown();
                    }
                }
            }

            if let Ok(v01_Async::Ready(Some(event))) = self.player_events.poll() {
                debug!("librespot player event: {:?}", event);
                match event {
                    LibrespotPlayerEvent::Started { .. }
                    | LibrespotPlayerEvent::Loading { .. }
                    | LibrespotPlayerEvent::Changed { .. } => {
                        progress = true;
                    }
                    LibrespotPlayerEvent::Playing {
                        play_request_id: _,
                        track_id: _,
                        position_ms,
                        duration_ms: _,
                    } => {
                        let position = Duration::from_millis(position_ms as u64);
                        let playback_start = SystemTime::now() - position;
                        self.events
                            .send(Event::Player(PlayerEvent::Playing(playback_start)));
                        self.refresh_task = self.create_refresh();
                        self.active = true;
                    }
                    LibrespotPlayerEvent::Paused {
                        play_request_id: _,
                        track_id: _,
                        position_ms,
                        duration_ms: _,
                    } => {
                        let position = Duration::from_millis(position_ms as u64);
                        self.events
                            .send(Event::Player(PlayerEvent::Paused(position)));
                        self.active = false;
                    }
                    LibrespotPlayerEvent::Stopped { .. } => {
                        self.events.send(Event::Player(PlayerEvent::Stopped));
                        self.active = false;
                    }
                    LibrespotPlayerEvent::EndOfTrack { .. } => {
                        self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                        progress = true;
                    }
                    LibrespotPlayerEvent::TimeToPreloadNextTrack { .. } => {
                        self.events
                            .send(Event::Queue(QueueEvent::PreloadTrackRequest));
                    }
                    _ => {}
                }
            }

            if let Poll::Ready(Some(Ok(_))) = self.refresh_task.as_mut().poll_next(cx) {
                self.refresh_task = if self.active {
                    progress = true;
                    self.create_refresh()
                } else {
                    Box::pin(futures::stream::empty())
                };
            }

            match self.token_task.as_mut().poll(cx) {
                Poll::Ready(Ok(_)) => {
                    info!("token updated!");
                    self.token_task = Box::pin(futures::future::pending())
                }
                Poll::Ready(Err(e)) => {
                    error!("could not generate token: {:?}", e);
                }
                _ => (),
            }

            if !progress {
                return Poll::Pending;
            }
        }
    }
}
