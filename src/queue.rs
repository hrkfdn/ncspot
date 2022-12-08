use std::cmp::Ordering;
#[cfg(feature = "notify")]
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, RwLock};

use log::{debug, error, info};
#[cfg(feature = "notify")]
use notify_rust::{Hint, Notification, Urgency};

use rand::prelude::*;
use strum_macros::Display;

use crate::config::{Config, NotificationFormat, PlaybackState};
use crate::library::Library;
use crate::model::playable::Playable;
use crate::spotify::PlayerEvent;
use crate::spotify::Spotify;

#[derive(Display, Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RepeatSetting {
    #[serde(rename = "off")]
    None,
    #[serde(rename = "playlist")]
    RepeatPlaylist,
    #[serde(rename = "track")]
    RepeatTrack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueEvent {
    PreloadTrackRequest,
}

#[derive(Debug)]
pub struct Queue {
    pub queue: Arc<RwLock<Vec<Playable>>>,
    random_order: RwLock<Option<Vec<usize>>>,
    current_track: RwLock<Option<usize>>,
    spotify: Spotify,
    cfg: Arc<Config>,
    #[cfg(feature = "notify")]
    notification_id: Arc<AtomicU32>,
    library: Arc<Library>,
}

impl Queue {
    pub fn new(spotify: Spotify, cfg: Arc<Config>, library: Arc<Library>) -> Queue {
        let queue_state = cfg.state().queuestate.clone();
        let playback_state = cfg.state().playback_state.clone();
        let queue = Queue {
            queue: Arc::new(RwLock::new(queue_state.queue)),
            spotify: spotify.clone(),
            current_track: RwLock::new(queue_state.current_track),
            random_order: RwLock::new(queue_state.random_order),
            cfg,
            #[cfg(feature = "notify")]
            notification_id: Arc::new(AtomicU32::new(0)),
            library,
        };

        if let Some(playable) = queue.get_current() {
            spotify.load(
                &playable,
                playback_state == PlaybackState::Playing,
                queue_state.track_progress.as_millis() as u32,
            );
            spotify.update_track();
            match playback_state {
                PlaybackState::Stopped => {
                    spotify.stop();
                }
                PlaybackState::Paused | PlaybackState::Playing | PlaybackState::Default => {
                    spotify.pause();
                }
            }
        }

        queue
    }

    pub fn next_index(&self) -> Option<usize> {
        match *self.current_track.read().unwrap() {
            Some(mut index) => {
                let random_order = self.random_order.read().unwrap();
                if let Some(order) = random_order.as_ref() {
                    index = order.iter().position(|&i| i == index).unwrap();
                }

                let mut next_index = index + 1;
                if next_index < self.queue.read().unwrap().len() {
                    if let Some(order) = random_order.as_ref() {
                        next_index = order[next_index];
                    }

                    Some(next_index)
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub fn previous_index(&self) -> Option<usize> {
        match *self.current_track.read().unwrap() {
            Some(mut index) => {
                let random_order = self.random_order.read().unwrap();
                if let Some(order) = random_order.as_ref() {
                    index = order.iter().position(|&i| i == index).unwrap();
                }

                if index > 0 {
                    let mut next_index = index - 1;
                    if let Some(order) = random_order.as_ref() {
                        next_index = order[next_index];
                    }

                    Some(next_index)
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub fn get_current(&self) -> Option<Playable> {
        self.get_current_index()
            .map(|index| self.queue.read().unwrap()[index].clone())
    }

    pub fn get_current_index(&self) -> Option<usize> {
        *self.current_track.read().unwrap()
    }

    pub fn insert_after_current(&self, track: Playable) {
        if let Some(index) = self.get_current_index() {
            let mut random_order = self.random_order.write().unwrap();
            if let Some(order) = random_order.as_mut() {
                let next_i = order.iter().position(|&i| i == index).unwrap();
                // shift everything after the insertion in order
                for item in order.iter_mut() {
                    if *item > index {
                        *item += 1;
                    }
                }
                // finally, add the next track index
                order.insert(next_i + 1, index + 1);
            }
            let mut q = self.queue.write().unwrap();
            q.insert(index + 1, track);
        } else {
            self.append(track);
        }
    }

    pub fn append(&self, track: Playable) {
        let mut random_order = self.random_order.write().unwrap();
        if let Some(order) = random_order.as_mut() {
            let index = order.len().saturating_sub(1);
            order.push(index);
        }

        let mut q = self.queue.write().unwrap();
        q.push(track);
    }

    pub fn append_next(&self, tracks: &Vec<Playable>) -> usize {
        let mut q = self.queue.write().unwrap();

        {
            let mut random_order = self.random_order.write().unwrap();
            if let Some(order) = random_order.as_mut() {
                order.extend((q.len().saturating_sub(1))..(q.len() + tracks.len()));
            }
        }

        let first = match *self.current_track.read().unwrap() {
            Some(index) => index + 1,
            None => q.len(),
        };

        let mut i = first;
        for track in tracks {
            q.insert(i, track.clone());
            i += 1;
        }

        first
    }

    pub fn remove(&self, index: usize) {
        {
            let mut q = self.queue.write().unwrap();
            if q.len() == 0 {
                info!("queue is empty");
                return;
            }
            q.remove(index);
        }

        // if the queue is empty stop playback
        let len = self.queue.read().unwrap().len();
        if len == 0 {
            self.stop();
            return;
        }

        // if we are deleting the currently playing track, play the track with
        // the same index again, because the next track is now at the position
        // of the one we deleted
        let current = *self.current_track.read().unwrap();
        if let Some(current_track) = current {
            match current_track.cmp(&index) {
                Ordering::Equal => {
                    // if we have deleted the last item and it was playing
                    // stop playback, unless repeat playlist is on, play next
                    if current_track == len {
                        if self.get_repeat() == RepeatSetting::RepeatPlaylist {
                            self.next(false);
                        } else {
                            self.stop();
                        }
                    } else {
                        self.play(index, false, false);
                    }
                }
                Ordering::Greater => {
                    let mut current = self.current_track.write().unwrap();
                    current.replace(current_track - 1);
                }
                _ => (),
            }
        }

        if self.get_shuffle() {
            self.generate_random_order();
        }
    }

    pub fn clear(&self) {
        self.stop();

        let mut q = self.queue.write().unwrap();
        q.clear();

        let mut random_order = self.random_order.write().unwrap();
        if let Some(o) = random_order.as_mut() {
            o.clear()
        }
    }

    pub fn len(&self) -> usize {
        self.queue.read().unwrap().len()
    }

    pub fn shift(&self, from: usize, to: usize) {
        let mut queue = self.queue.write().unwrap();
        let item = queue.remove(from);
        queue.insert(to, item);

        // if the currently playing track is affected by the shift, update its
        // index
        let mut current = self.current_track.write().unwrap();
        if let Some(index) = *current {
            if index == from {
                current.replace(to);
            } else if index == to && from > index {
                current.replace(to + 1);
            } else if index == to && from < index {
                current.replace(to - 1);
            }
        }
    }

    pub fn play(&self, mut index: usize, reshuffle: bool, shuffle_index: bool) {
        let queue_length = self.queue.read().unwrap().len();
        // The length of the queue must be bigger than 0 or gen_range panics!
        if queue_length > 0 && shuffle_index && self.get_shuffle() {
            let mut rng = rand::thread_rng();
            index = rng.gen_range(0..queue_length);
        }

        if let Some(track) = &self.queue.read().unwrap().get(index) {
            self.spotify.load(track, true, 0);
            let mut current = self.current_track.write().unwrap();
            current.replace(index);
            self.spotify.update_track();

            #[cfg(feature = "notify")]
            if self.cfg.values().notify.unwrap_or(false) {
                let notification_id = self.notification_id.clone();
                std::thread::spawn({
                    // use same parser as track_format, Playable::format
                    let format = self
                        .cfg
                        .values()
                        .notification_format
                        .clone()
                        .unwrap_or_default();
                    let default_title = NotificationFormat::default().title.unwrap();
                    let title = format.title.unwrap_or_else(|| default_title.clone());

                    let default_body = NotificationFormat::default().body.unwrap();
                    let body = format.body.unwrap_or_else(|| default_body.clone());

                    let summary_txt = Playable::format(track, &title, self.library.clone());
                    let body_txt = Playable::format(track, &body, self.library.clone());
                    let cover_url = if cfg!(feature = "cover") {
                        track.cover_url()
                    } else {
                        None
                    };
                    move || send_notification(&summary_txt, &body_txt, cover_url, notification_id)
                });
            }
        }

        if reshuffle && self.get_shuffle() {
            self.generate_random_order()
        }
    }

    pub fn toggleplayback(&self) {
        match self.spotify.get_current_status() {
            PlayerEvent::Playing(_) | PlayerEvent::Paused(_) => {
                self.spotify.toggleplayback();
            }
            PlayerEvent::Stopped => match self.next_index() {
                Some(_) => self.next(false),
                None => self.play(0, false, false),
            },
            _ => (),
        }
    }

    pub fn stop(&self) {
        let mut current = self.current_track.write().unwrap();
        *current = None;
        self.spotify.stop();
    }

    pub fn next(&self, manual: bool) {
        let q = self.queue.read().unwrap();
        let current = *self.current_track.read().unwrap();
        let repeat = self.cfg.state().repeat;

        if repeat == RepeatSetting::RepeatTrack && !manual {
            if let Some(index) = current {
                self.play(index, false, false);
            }
        } else if let Some(index) = self.next_index() {
            self.play(index, false, false);
            if repeat == RepeatSetting::RepeatTrack && manual {
                self.set_repeat(RepeatSetting::RepeatPlaylist);
            }
        } else if repeat == RepeatSetting::RepeatPlaylist && q.len() > 0 {
            let random_order = self.random_order.read().unwrap();
            self.play(
                random_order.as_ref().map(|o| o[0]).unwrap_or(0),
                false,
                false,
            );
        } else {
            self.spotify.stop();
        }
    }

    pub fn previous(&self) {
        let q = self.queue.read().unwrap();
        let current = *self.current_track.read().unwrap();
        let repeat = self.cfg.state().repeat;

        if let Some(index) = self.previous_index() {
            self.play(index, false, false);
        } else if repeat == RepeatSetting::RepeatPlaylist && q.len() > 0 {
            if self.get_shuffle() {
                let random_order = self.random_order.read().unwrap();
                self.play(
                    random_order.as_ref().map(|o| o[q.len() - 1]).unwrap_or(0),
                    false,
                    false,
                );
            } else {
                self.play(q.len() - 1, false, false);
            }
        } else if let Some(index) = current {
            self.play(index, false, false);
        }
    }

    pub fn get_repeat(&self) -> RepeatSetting {
        self.cfg.state().repeat
    }

    pub fn set_repeat(&self, new: RepeatSetting) {
        self.cfg.with_state_mut(|mut s| s.repeat = new);
    }

    pub fn get_shuffle(&self) -> bool {
        self.cfg.state().shuffle
    }

    pub fn get_random_order(&self) -> Option<Vec<usize>> {
        self.random_order.read().unwrap().clone()
    }

    fn generate_random_order(&self) {
        let q = self.queue.read().unwrap();
        let mut order: Vec<usize> = Vec::with_capacity(q.len());
        let mut random: Vec<usize> = (0..q.len()).collect();

        if let Some(current) = *self.current_track.read().unwrap() {
            order.push(current);
            random.remove(current);
        }

        let mut rng = rand::thread_rng();
        random.shuffle(&mut rng);
        order.extend(random);

        let mut random_order = self.random_order.write().unwrap();
        *random_order = Some(order);
    }

    pub fn set_shuffle(&self, new: bool) {
        self.cfg.with_state_mut(|mut s| s.shuffle = new);
        if new {
            self.generate_random_order();
        } else {
            let mut random_order = self.random_order.write().unwrap();
            *random_order = None;
        }
    }

    pub fn handle_event(&self, event: QueueEvent) {
        match event {
            QueueEvent::PreloadTrackRequest => {
                if let Some(next_index) = self.next_index() {
                    let track = self.queue.read().unwrap()[next_index].clone();
                    debug!("Preloading track {} as requested by librespot", track);
                    self.spotify.preload(&track);
                }
            }
        }
    }

    pub fn get_spotify(&self) -> Spotify {
        self.spotify.clone()
    }
}

#[cfg(feature = "notify")]
pub fn send_notification(
    summary_txt: &str,
    body_txt: &str,
    cover_url: Option<String>,
    notification_id: Arc<AtomicU32>,
) {
    let current_notification_id = notification_id.load(std::sync::atomic::Ordering::Relaxed);

    let mut n = Notification::new();
    n.appname("ncspot")
        .id(current_notification_id)
        .summary(summary_txt)
        .body(body_txt);

    // album cover image
    if let Some(u) = cover_url {
        let path = crate::utils::cache_path_for_url(u.to_string());
        if !path.exists() {
            if let Err(e) = crate::utils::download(u, path.clone()) {
                error!("Failed to download cover: {}", e);
            }
        }
        n.icon(path.to_str().unwrap());
    }

    // XDG desktop entry hints
    #[cfg(all(unix, not(target_os = "macos")))]
    n.urgency(Urgency::Low)
        .hint(Hint::Transient(true))
        .hint(Hint::DesktopEntry("ncspot".into()));

    match n.show() {
        Ok(_handle) => {
            // only available for XDG
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                let new_notification_id = _handle.id();
                log::debug!(
                    "new notification id: {}, previously: {}",
                    new_notification_id,
                    current_notification_id
                );
                notification_id.store(new_notification_id, std::sync::atomic::Ordering::Relaxed);
            }
        }
        Err(e) => error!("Failed to send notification cover: {}", e),
    }
}
