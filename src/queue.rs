use std::cmp::Ordering;
use std::sync::{Arc, RwLock};

use log::{debug, info};
#[cfg(feature = "notify")]
use notify_rust::Notification;

use rand::prelude::*;
use strum_macros::Display;

use crate::config::Config;
use crate::library::Library;
use crate::model::playable::Playable;
use crate::spotify::PlayerEvent;
use crate::spotify::Spotify;
use crate::traits::ListItem;

/// Repeat behavior for the [Queue].
#[derive(Display, Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RepeatSetting {
    #[serde(rename = "off")]
    None,
    #[serde(rename = "playlist")]
    RepeatPlaylist,
    #[serde(rename = "track")]
    RepeatTrack,
}

/// Events that are specific to the [Queue].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueEvent {
    /// Request the player to 'preload' a track, basically making sure that
    /// transitions between tracks can be uninterrupted.
    PreloadTrackRequest,
}

/// The queue determines the playback order of [Playable] items, and is also used to control
/// playback itself.
pub struct Queue {
    /// The internal data, which doesn't change with shuffle or repeat. This is
    /// the raw data only.
    pub queue: Arc<RwLock<Vec<Playable>>>,
    /// The playback order of the queue, as indices into `self.queue`.
    random_order: RwLock<Option<Vec<usize>>>,
    current_track: RwLock<Option<usize>>,
    spotify: Spotify,
    cfg: Arc<Config>,
    library: Arc<Library>,
}

impl Queue {
    pub fn new(spotify: Spotify, cfg: Arc<Config>, library: Arc<Library>) -> Self {
        let queue_state = cfg.state().queuestate.clone();

        Self {
            queue: Arc::new(RwLock::new(queue_state.queue)),
            spotify: spotify.clone(),
            current_track: RwLock::new(queue_state.current_track),
            random_order: RwLock::new(queue_state.random_order),
            cfg,
            library,
        }
    }

    /// The index of the next item in `self.queue` that should be played. None
    /// if at the end of the queue.
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

    /// The index of the previous item in `self.queue` that should be played.
    /// None if at the start of the queue.
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

    /// The currently playing item from `self.queue`.
    pub fn get_current(&self) -> Option<Playable> {
        self.get_current_index()
            .map(|index| self.queue.read().unwrap()[index].clone())
    }

    /// The index of the currently playing item from `self.queue`.
    pub fn get_current_index(&self) -> Option<usize> {
        *self.current_track.read().unwrap()
    }

    /// Insert `track` as the item that should logically follow the currently
    /// playing item, taking into account shuffle status.
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

    /// Add `track` to the end of the queue.
    pub fn append(&self, track: Playable) {
        let mut random_order = self.random_order.write().unwrap();
        if let Some(order) = random_order.as_mut() {
            order.push(order.len());
        }

        let mut q = self.queue.write().unwrap();
        q.push(track);
    }

    /// Append `tracks` after the currently playing item, taking into account
    /// shuffle status. Returns the first index(in `self.queue`) of added items.
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

    /// Remove the item at `index`. This doesn't take into account shuffle
    /// status, and will literally remove the item at `index` in `self.queue`.
    pub fn remove(&self, index: usize) {
        {
            let mut q = self.queue.write().unwrap();
            if q.is_empty() {
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

    /// Clear all the items from the queue and stop playback.
    pub fn clear(&self) {
        self.stop();

        let mut q = self.queue.write().unwrap();
        q.clear();

        let mut random_order = self.random_order.write().unwrap();
        if let Some(o) = random_order.as_mut() {
            o.clear()
        }
    }

    /// The amount of items in `self.queue`.
    pub fn len(&self) -> usize {
        self.queue.read().unwrap().len()
    }

    /// Shift the item at `from` in `self.queue` to `to`.
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

    /// Play the item at `index` in `self.queue`.
    ///
    /// `reshuffle`: Reshuffle the current order of the queue.
    /// `shuffle_index`: If this is true, `index` isn't actually used, but is
    /// chosen at random as a valid index in the queue.
    pub fn play(&self, mut index: usize, reshuffle: bool, shuffle_index: bool) {
        let queue_length = self.queue.read().unwrap().len();
        // The length of the queue must be bigger than 0 or gen_range panics!
        if queue_length > 0 && shuffle_index && self.get_shuffle() {
            let mut rng = rand::rng();
            index = rng.random_range(0..queue_length);
        }

        if let Some(track) = &self.queue.read().unwrap().get(index) {
            self.spotify.load(track, true, 0);
            let mut current = self.current_track.write().unwrap();
            current.replace(index);
            self.spotify.update_track();

            #[cfg(feature = "notify")]
            if self.cfg.values().notify.unwrap_or(false) {
                std::thread::spawn({
                    // use same parser as track_format, Playable::format
                    let format = self
                        .cfg
                        .values()
                        .notification_format
                        .clone()
                        .unwrap_or_default();
                    let default_title = crate::config::NotificationFormat::default().title.unwrap();
                    let title = format.title.unwrap_or_else(|| default_title.clone());

                    let default_body = crate::config::NotificationFormat::default().body.unwrap();
                    let body = format.body.unwrap_or_else(|| default_body.clone());

                    let summary_txt = Playable::format(track, &title, &self.library);
                    let body_txt = Playable::format(track, &body, &self.library);
                    let cover_url = track.cover_url();
                    move || send_notification(&summary_txt, &body_txt, cover_url)
                });
            }

            // Send a Seeked signal at start of new track
            #[cfg(feature = "mpris")]
            self.spotify.notify_seeked(0);
        }

        if reshuffle && self.get_shuffle() {
            self.generate_random_order()
        }
    }

    /// Toggle the playback. If playback is currently stopped, this will either
    /// play the next song if one is available, or restart from the start.
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

    /// Stop playback.
    pub fn stop(&self) {
        let mut current = self.current_track.write().unwrap();
        *current = None;
        self.spotify.stop();
    }

    /// Play the next song in the queue. Stops playback if there are no playable tracks
    /// remaining.
    ///
    /// `manual`: If this is true, normal queue logic like repeat will not be
    /// used, and the next track will actually be played. This should be used
    /// when going to the next entry in the queue is the wanted behavior.
    pub fn next(&self, manual: bool) {
        let q = self.queue.read().unwrap();
        let current = *self.current_track.read().unwrap();
        let repeat = self.cfg.state().repeat;

        if repeat == RepeatSetting::RepeatTrack && !manual {
            if let Some(index) = current
                && q[index].is_playable()
            {
                self.play(index, false, false);
            }
        } else if let Some(index) = self.next_index() {
            self.play(index, false, false);
            if repeat == RepeatSetting::RepeatTrack && manual {
                self.set_repeat(RepeatSetting::RepeatPlaylist);
            }
        } else if repeat == RepeatSetting::RepeatPlaylist
            && !q.is_empty()
            && q.iter().any(|track| track.is_playable())
        {
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

    /// Play the previous item in the queue.
    pub fn previous(&self) {
        let q = self.queue.read().unwrap();
        let current = *self.current_track.read().unwrap();
        let repeat = self.cfg.state().repeat;

        if let Some(index) = self.previous_index() {
            self.play(index, false, false);
        } else if repeat == RepeatSetting::RepeatPlaylist && !q.is_empty() {
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

    /// Get the current repeat behavior.
    pub fn get_repeat(&self) -> RepeatSetting {
        self.cfg.state().repeat
    }

    /// Set the current repeat behavior and save it to the configuration.
    pub fn set_repeat(&self, new: RepeatSetting) {
        self.cfg.with_state_mut(|s| s.repeat = new);
    }

    /// Get the current shuffle behavior.
    pub fn get_shuffle(&self) -> bool {
        self.cfg.state().shuffle
    }

    /// Get the current order that is used to shuffle.
    pub fn get_random_order(&self) -> Option<Vec<usize>> {
        self.random_order.read().unwrap().clone()
    }

    /// (Re)generate the random shuffle order.
    fn generate_random_order(&self) {
        let q = self.queue.read().unwrap();
        let mut order: Vec<usize> = Vec::with_capacity(q.len());
        let mut random: Vec<usize> = (0..q.len()).collect();

        if let Some(current) = *self.current_track.read().unwrap() {
            order.push(current);
            random.remove(current);
        }

        let mut rng = rand::rng();
        random.shuffle(&mut rng);
        order.extend(random);

        let mut random_order = self.random_order.write().unwrap();
        *random_order = Some(order);
    }

    /// Set the current shuffle behavior.
    pub fn set_shuffle(&self, new: bool) {
        self.cfg.with_state_mut(|s| s.shuffle = new);
        if new {
            self.generate_random_order();
        } else {
            let mut random_order = self.random_order.write().unwrap();
            *random_order = None;
        }
    }

    /// Handle events that are specific to the queue.
    pub fn handle_event(&self, event: QueueEvent) {
        match event {
            QueueEvent::PreloadTrackRequest => {
                if let Some(next_index) = self.next_index() {
                    let track = self.queue.read().unwrap()[next_index].clone();
                    debug!("Preloading track {track} as requested by librespot");
                    self.spotify.preload(&track);
                }
            }
        }
    }

    /// Get the spotify session.
    pub fn get_spotify(&self) -> Spotify {
        self.spotify.clone()
    }
}

/// Send a notification using the desktops default notification method.
///
/// `summary_txt`: A short title for the notification.
/// `body_txt`: The actual content of the notification.
/// `cover_url`: A URL to an image to show in the notification.
/// `notification_id`: Unique id for a notification, that can be used to operate
/// on a previous notification (for example to close it).
#[cfg(feature = "notify")]
pub fn send_notification(summary_txt: &str, body_txt: &str, cover_url: Option<String>) {
    let mut n = Notification::new();
    n.appname("ncspot").summary(summary_txt).body(body_txt);

    // album cover image
    if let Some(u) = cover_url {
        let path = crate::utils::cache_path_for_url(u.to_string());
        if !path.exists()
            && let Err(e) = crate::utils::download(u, path.clone())
        {
            log::error!("Failed to download cover: {e}");
        }
        n.icon(path.to_str().unwrap());
    }

    // XDG desktop entry hints
    #[cfg(all(unix, not(target_os = "macos")))]
    n.urgency(notify_rust::Urgency::Low)
        .hint(notify_rust::Hint::Transient(true))
        .hint(notify_rust::Hint::DesktopEntry("ncspot".into()));

    match n.show() {
        Ok(handle) => {
            // only available for XDG
            #[cfg(all(unix, not(target_os = "macos")))]
            info!("Created notification: {}", handle.id());
        }
        Err(e) => log::error!("Failed to send notification cover: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use super::*;
    use crate::config::Config;
    use crate::events::EventManager;
    use crate::library::Library;
    use crate::model::track::Track;
    use crate::spotify::Spotify;

    fn make_track(id: u32) -> Playable {
        Playable::Track(Track {
            id: Some(format!("id_{id}")),
            uri: format!("spotify:track:id_{id}"),
            title: format!("Track {id}"),
            track_number: id,
            disc_number: 1,
            duration: 180_000,
            artists: vec!["Artist".to_string()],
            artist_ids: vec![],
            album: Some("Album".to_string()),
            album_id: None,
            album_artists: vec![],
            cover_url: None,
            url: String::new(),
            added_at: None,
            list_index: 0,
            is_local: false,
            // Must be Some(true) so Spotify::load() doesn't fire events.
            is_playable: Some(true),
        })
    }

    fn track_id(p: &Playable) -> &str {
        match p {
            Playable::Track(t) => t.id.as_deref().unwrap_or(""),
            Playable::Episode(_) => "",
        }
    }

    fn make_queue(tracks: Vec<Playable>, current: Option<usize>) -> Queue {
        let cfg = Config::new_for_test();
        let ev = EventManager::new_for_test();
        let spotify = Spotify::new_for_test(cfg.clone(), ev.clone());
        let library = Library::new_for_test(ev, spotify.clone(), cfg.clone());
        Queue {
            queue: Arc::new(RwLock::new(tracks)),
            random_order: RwLock::new(None),
            current_track: RwLock::new(current),
            spotify,
            cfg,
            library,
        }
    }

    // --- next_index / previous_index ---

    #[test]
    fn test_next_index_basic() {
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], Some(0));
        assert_eq!(q.next_index(), Some(1));
    }

    #[test]
    fn test_next_index_at_end_returns_none() {
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], Some(2));
        assert_eq!(q.next_index(), None);
    }

    #[test]
    fn test_next_index_no_current_returns_none() {
        let q = make_queue(vec![make_track(0), make_track(1)], None);
        assert_eq!(q.next_index(), None);
    }

    #[test]
    fn test_previous_index_basic() {
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], Some(2));
        assert_eq!(q.previous_index(), Some(1));
    }

    #[test]
    fn test_previous_index_at_start_returns_none() {
        let q = make_queue(vec![make_track(0), make_track(1)], Some(0));
        assert_eq!(q.previous_index(), None);
    }

    #[test]
    fn test_previous_index_no_current_returns_none() {
        let q = make_queue(vec![make_track(0)], None);
        assert_eq!(q.previous_index(), None);
    }

    // --- next / previous with shuffle ---

    #[test]
    fn test_next_index_respects_random_order() {
        // Queue [0,1,2,3], current_track=2, random_order=[2,0,3,1]
        // Position of 2 in order = 0, next position = 1, order[1] = 0 → Some(0)
        let q = make_queue(
            vec![make_track(0), make_track(1), make_track(2), make_track(3)],
            Some(2),
        );
        *q.random_order.write().unwrap() = Some(vec![2, 0, 3, 1]);
        assert_eq!(q.next_index(), Some(0));
    }

    #[test]
    fn test_previous_index_respects_random_order() {
        // Queue [0,1,2,3], current_track=0, random_order=[2,0,3,1]
        // Position of 0 in order = 1, prev position = 0, order[0] = 2 → Some(2)
        let q = make_queue(
            vec![make_track(0), make_track(1), make_track(2), make_track(3)],
            Some(0),
        );
        *q.random_order.write().unwrap() = Some(vec![2, 0, 3, 1]);
        assert_eq!(q.previous_index(), Some(2));
    }

    #[test]
    fn test_next_index_at_end_of_shuffle_order_returns_none() {
        // Queue [0,1,2,3], current_track=1, random_order=[2,0,3,1]
        // Position of 1 in order = 3 (last), next = 4 → None
        let q = make_queue(
            vec![make_track(0), make_track(1), make_track(2), make_track(3)],
            Some(1),
        );
        *q.random_order.write().unwrap() = Some(vec![2, 0, 3, 1]);
        assert_eq!(q.next_index(), None);
    }

    // --- len, get_current ---

    #[test]
    fn test_len() {
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], None);
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn test_get_current_returns_right_track() {
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], Some(1));
        let current = q.get_current().expect("should have current");
        assert_eq!(track_id(&current), "id_1");
    }

    #[test]
    fn test_get_current_none_when_no_current() {
        let q = make_queue(vec![make_track(0)], None);
        assert!(q.get_current().is_none());
    }

    // --- append / append_next / insert_after_current ---

    #[test]
    fn test_append_increases_len() {
        let q = make_queue(vec![make_track(0)], None);
        q.append(make_track(1));
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn test_append_adds_to_end() {
        let q = make_queue(vec![make_track(0), make_track(1)], None);
        q.append(make_track(2));
        let queue = q.queue.read().unwrap();
        assert_eq!(track_id(&queue[2]), "id_2");
    }

    #[test]
    fn test_append_next_inserts_after_current() {
        // [0, 1, 2], current=1 → append_next([3, 4]) → [0, 1, 3, 4, 2]
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], Some(1));
        let first = q.append_next(&vec![make_track(3), make_track(4)]);
        assert_eq!(first, 2);
        let queue = q.queue.read().unwrap();
        let ids: Vec<&str> = queue.iter().map(track_id).collect();
        assert_eq!(ids, ["id_0", "id_1", "id_3", "id_4", "id_2"]);
    }

    #[test]
    fn test_append_next_no_current_appends_at_end() {
        let q = make_queue(vec![make_track(0), make_track(1)], None);
        let first = q.append_next(&vec![make_track(2)]);
        assert_eq!(first, 2);
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn test_insert_after_current() {
        // [0, 1], current=0 → insert 99 → [0, 99, 1], current still 0, next is 99
        let q = make_queue(vec![make_track(0), make_track(1)], Some(0));
        q.insert_after_current(make_track(99));
        assert_eq!(q.len(), 3);
        assert_eq!(q.get_current_index(), Some(0));
        let queue = q.queue.read().unwrap();
        assert_eq!(track_id(&queue[1]), "id_99");
        assert_eq!(track_id(&queue[2]), "id_1");
    }

    #[test]
    fn test_insert_after_current_no_current_appends() {
        let q = make_queue(vec![make_track(0)], None);
        q.insert_after_current(make_track(1));
        assert_eq!(q.len(), 2);
    }

    // --- remove ---

    #[test]
    fn test_remove_item_after_current_keeps_index() {
        // [0, 1, 2], current=0, remove(2) → current stays 0
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], Some(0));
        q.remove(2);
        assert_eq!(q.len(), 2);
        assert_eq!(q.get_current_index(), Some(0));
    }

    #[test]
    fn test_remove_item_before_current_adjusts_index() {
        // [0, 1, 2], current=2, remove(0) → current becomes 1
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], Some(2));
        q.remove(0);
        assert_eq!(q.len(), 2);
        assert_eq!(q.get_current_index(), Some(1));
    }

    // --- shift ---

    #[test]
    fn test_shift_moves_item() {
        // [0, 1, 2], shift(2, 0) → [2, 0, 1]
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], None);
        q.shift(2, 0);
        let queue = q.queue.read().unwrap();
        let ids: Vec<&str> = queue.iter().map(track_id).collect();
        assert_eq!(ids, ["id_2", "id_0", "id_1"]);
    }

    #[test]
    fn test_shift_updates_current_when_current_is_moved() {
        // [0, 1, 2], current=2, shift(2, 0) → current becomes 0
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], Some(2));
        q.shift(2, 0);
        assert_eq!(q.get_current_index(), Some(0));
    }

    // --- shuffle ---

    #[test]
    fn test_shuffle_generates_random_order() {
        let q = make_queue(
            vec![make_track(0), make_track(1), make_track(2), make_track(3)],
            Some(1),
        );
        q.set_shuffle(true);
        let order = q.get_random_order().expect("random_order should be Some");
        assert_eq!(order.len(), 4);
        // Current track (index 1) must be first.
        assert_eq!(order[0], 1);
        // All queue indices appear exactly once.
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1, 2, 3]);
    }

    #[test]
    fn test_shuffle_disable_clears_order() {
        let q = make_queue(vec![make_track(0), make_track(1)], Some(0));
        q.set_shuffle(true);
        assert!(q.get_random_order().is_some());
        q.set_shuffle(false);
        assert!(q.get_random_order().is_none());
    }

    #[test]
    fn test_append_in_shuffle_mode_maintains_valid_order() {
        // After appending to a shuffled queue, random_order must contain every
        // queue index exactly once.
        let q = make_queue(vec![make_track(0), make_track(1)], Some(0));
        q.set_shuffle(true);
        q.append(make_track(2));
        let order = q.get_random_order().expect("random_order should be Some");
        assert_eq!(order.len(), 3);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            [0, 1, 2],
            "random_order must contain every queue index once"
        );
    }

    // --- clear ---

    #[test]
    fn test_clear_empties_queue() {
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], Some(1));
        q.clear();
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn test_clear_empties_random_order_when_shuffled() {
        let q = make_queue(vec![make_track(0), make_track(1), make_track(2)], Some(1));
        q.set_shuffle(true);
        q.clear();
        // clear() clears the order contents but leaves it as Some([]).
        assert_eq!(q.get_random_order(), Some(vec![]));
    }
}
