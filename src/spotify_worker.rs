use crate::config;
use crate::events::{Event, EventManager};
use crate::model::playable::Playable;
use crate::queue::QueueEvent;
use crate::spotify::{PlayerEvent, Spotify};
use futures::future::BoxFuture;
use librespot_core::SpotifyUri;
use librespot_core::authentication::Credentials;
use librespot_core::session::Session;
use librespot_playback::mixer::Mixer;
use librespot_playback::player::{Player, PlayerEvent as LibrespotPlayerEvent};
use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use tokio::sync::mpsc;
use tokio::time;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

fn reconnect_delay(failed_attempts: u32) -> Duration {
    Duration::from_secs(
        2u64.saturating_pow(failed_attempts.saturating_sub(1))
            .min(30),
    )
}

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
    session: Session,
    player: Arc<Player>,
    player_status: PlayerStatus,
    mixer: Arc<dyn Mixer>,
    cfg: Arc<config::Config>,
    credentials: Credentials,
}

impl Worker {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        events: EventManager,
        player_events: mpsc::UnboundedReceiver<LibrespotPlayerEvent>,
        commands: mpsc::UnboundedReceiver<WorkerCommand>,
        session: Session,
        player: Arc<Player>,
        mixer: Arc<dyn Mixer>,
        cfg: Arc<config::Config>,
        credentials: Credentials,
    ) -> Self {
        Self {
            events,
            player_events: UnboundedReceiverStream::new(player_events),
            commands: UnboundedReceiverStream::new(commands),
            player,
            session,
            player_status: PlayerStatus::Stopped,
            mixer,
            cfg,
            credentials,
        }
    }

    pub async fn run_loop(&mut self) {
        let mut ui_refresh = time::interval(Duration::from_millis(400));
        let mut reconnect: Option<BoxFuture<'static, Result<Session, librespot_core::Error>>> =
            None;
        let mut failed_attempts = 0;
        let mut loading = None;
        let mut load_failed = false;
        let mut preload = None;
        let mut preload_retried = false;

        loop {
            if self.session.is_invalid() && reconnect.is_none() {
                let delay = reconnect_delay(failed_attempts);
                warn!("Librespot session invalidated, reconnecting in {delay:?}");
                let cache = self.session.cache();
                // a login's access token expires, the stored credentials don't
                let credentials = cache
                    .and_then(|cache| cache.credentials())
                    .unwrap_or_else(|| self.credentials.clone());
                let session = Session::new(
                    Spotify::session_config(&self.cfg),
                    cache.map(|cache| cache.as_ref().clone()),
                );
                reconnect = Some(Box::pin(async move {
                    time::sleep(delay).await;
                    time::timeout(CONNECT_TIMEOUT, session.connect(credentials, true))
                        .await
                        .unwrap_or_else(|e| Err(librespot_core::Error::deadline_exceeded(e)))
                        .map(|_| session)
                }));
            }

            tokio::select! {
                result = async { reconnect.as_mut().unwrap().await }, if reconnect.is_some() => {
                    reconnect = None;
                    match result {
                        Ok(session) => {
                            info!("Librespot session reconnected");
                            self.player.set_session(session.clone());
                            self.session = session;
                            failed_attempts = 0;
                            load_failed = false;
                            preload_retried = false;
                            if let Some((uri, start_playing, position_ms)) = loading.clone() {
                                self.player.load(uri, start_playing, position_ms);
                            }
                            if let Some(uri) = preload.clone() {
                                self.player.preload(uri);
                            }
                        }
                        Err(e) => {
                            failed_attempts += 1;
                            error!("Reconnecting failed: {e}");
                        }
                    }
                },
                cmd = self.commands.next() => match cmd {
                    Some(WorkerCommand::Load(playable, start_playing, position_ms)) => {
                        loading = None;
                        preload = None;
                        match SpotifyUri::from_uri(&playable.uri()) {
                            Ok(uri) => {
                                info!("player loading track: {uri:?}");
                                if !uri.is_playable() {
                                    warn!("track is not playable");
                                    self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                                } else {
                                    self.player.load(uri.clone(), start_playing, position_ms);
                                    loading = Some((uri, start_playing, position_ms));
                                    load_failed = false;
                                }
                            }
                            Err(e) => {
                                error!("error parsing uri: {e:?}");
                                self.events.send(Event::Player(PlayerEvent::FinishedTrack));
                            }
                        }
                    }
                    Some(WorkerCommand::Play) => {
                        if let Some((uri, start_playing, position_ms)) = loading.as_mut() {
                            *start_playing = true;
                            if load_failed && !self.session.is_invalid() {
                                self.player.load(uri.clone(), true, *position_ms);
                            }
                        }
                        self.player.play();
                    }
                    Some(WorkerCommand::Pause) => {
                        if let Some((_, start_playing, _)) = loading.as_mut() {
                            *start_playing = false;
                        }
                        self.player.pause();
                    }
                    Some(WorkerCommand::Stop) => {
                        loading = None;
                        preload = None;
                        self.player.stop();
                    }
                    Some(WorkerCommand::Seek(pos)) => {
                        if let Some((_, _, position_ms)) = loading.as_mut() {
                            *position_ms = pos;
                        }
                        self.player.seek(pos);
                    }
                    Some(WorkerCommand::SetVolume(volume)) => {
                        self.mixer.set_volume(volume);
                    }
                    Some(WorkerCommand::Preload(playable)) => {
                        if let Ok(uri) = SpotifyUri::from_uri(&playable.uri()) {
                            debug!("Preloading {uri:?}");
                            self.player.preload(uri.clone());
                            preload = Some(uri);
                            preload_retried = false;
                        }
                    }
                    Some(WorkerCommand::Shutdown) => {
                        info!("Librespot session shut down, terminating worker");
                        self.player.stop();
                        self.session.shutdown();
                        self.events.send(Event::Player(PlayerEvent::Stopped));
                        break;
                    }
                    None => info!("empty stream")
                },
                event = self.player_events.next() => match event {
                    Some(LibrespotPlayerEvent::Playing {
                        play_request_id: _,
                        track_id,
                        position_ms,
                    }) => {
                        loading.take_if(|(uri, ..)| *uri == track_id);
                        let position = Duration::from_millis(position_ms as u64);
                        let playback_start = SystemTime::now() - position;
                        self.events
                            .send(Event::Player(PlayerEvent::Playing(playback_start)));
                        self.player_status = PlayerStatus::Playing;
                    }
                    Some(LibrespotPlayerEvent::Paused {
                        play_request_id: _,
                        track_id,
                        position_ms,
                    }) => {
                        loading.take_if(|(uri, ..)| *uri == track_id);
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
                    Some(LibrespotPlayerEvent::Unavailable { track_id, .. }) => {
                        if let Some((uri, start_playing, position_ms)) =
                            loading.clone().filter(|(uri, ..)| *uri == track_id)
                        {
                            if load_failed || self.session.is_invalid() {
                                warn!("Unable to load {uri:?}, holding it");
                                let position = Duration::from_millis(position_ms as u64);
                                self.events
                                    .send(Event::Player(PlayerEvent::Paused(position)));
                                self.player_status = PlayerStatus::Paused;
                            } else {
                                warn!("Unable to load {uri:?}, retrying");
                                self.player.load(uri, start_playing, position_ms);
                            }
                            load_failed = true;
                        } else if preload.as_ref() == Some(&track_id)
                            && !preload_retried
                            && !self.session.is_invalid()
                        {
                            preload_retried = true;
                            self.player.preload(track_id);
                        }
                    }
                    Some(event) => {
                        debug!("Unhandled player event: {event:?}");
                    }
                    None => {
                        warn!("Librespot player event channel died, terminating worker");
                        break
                    },
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
        self.player.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::reconnect_delay;
    use std::time::Duration;

    #[test]
    fn reconnect_delay_doubles_up_to_half_a_minute() {
        let delays: Vec<_> = (0..=7).map(reconnect_delay).collect();
        let expected: Vec<_> = [1, 1, 2, 4, 8, 16, 30, 30].map(Duration::from_secs).into();
        assert_eq!(delays, expected);
    }
}
