use std::sync::{Arc, RwLock};

use rand::prelude::*;

use spotify::Spotify;
use std::collections::VecDeque;
use std::fmt::{Debug, Display};
use track::Track;

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum RepeatSetting {
    None,
    RepeatPlaylist,
    RepeatTrack,
}

pub struct Queue {
    pub queue: Arc<RwLock<Vec<Track>>>,
    random_order: RwLock<Option<Vec<usize>>>,
    current_track: RwLock<Option<usize>>,
    repeat: RwLock<RepeatSetting>,
    spotify: Arc<Spotify>,
}

pub struct Queue2<T> {
    user_queue: RwLock<VecDeque<T>>,
    play_queue: RwLock<VecDeque<T>>,
    history: RwLock<Vec<T>>,
    current_track: RwLock<Option<T>>,
}

impl<T: Clone + Display + Debug> Queue2<T> {
    pub fn new() -> Queue2<T> {
        Queue2 {
            user_queue: Default::default(),
            play_queue: Default::default(),
            history: Default::default(),
            current_track: Default::default(),
        }
    }

    pub fn current_track(&self) -> Option<T> {
        self.current_track.read().unwrap().clone()
    }

    pub fn advance(&mut self) {
        let user_queue = self.user_queue.write().unwrap();

        // Select which queue to get the next track from
        let mut queue = if !user_queue.is_empty() {
            user_queue
        } else {
            self.play_queue.write().unwrap()
        };

        let mut current_track = self.current_track.write().unwrap();

        // Save previous track to history
        if current_track.is_some() {
            self.history
                .write()
                .unwrap()
                .push(current_track.clone().unwrap())
        }

        let track = queue.pop_front();
        *current_track = track;
    }

    pub fn queue_track(&mut self, track: T) {
        let mut user_queue = self.user_queue.write().unwrap();
        user_queue.push_back(track);
    }

    pub fn play(&mut self, tracks: Vec<T>) {
        if tracks.is_empty() {
            return;
        }

        let first = tracks.get(0).cloned();
        *self.current_track.write().unwrap() = first;

        let mut play_queue = self.play_queue.write().unwrap();
        let new_tracks = tracks.iter().skip(1).cloned();
        play_queue.extend(new_tracks);
    }

    pub fn print_status(&self) {
        if let Some(track) = self.current_track.read().unwrap().clone() {
            println!("Current track: {}", track)
        }

        println!("User queue: {:?}", self.user_queue.read().unwrap().clone());
        println!(
            "Play queue: {:?}\n",
            self.play_queue.read().unwrap().clone()
        );
    }
}

#[cfg(test)]
mod test {
    use queue::Queue2;

    #[test]
    fn test_queue() {
        let mut q2 = Queue2::new();
        q2.play(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

        assert_eq!(q2.current_track(), Some(0));

        q2.advance();
        q2.advance();

        assert_eq!(q2.current_track(), Some(2));

        q2.queue_track(-9);
        q2.queue_track(-8);
        q2.queue_track(-7);

        q2.advance();

        assert_eq!(q2.current_track(), Some(-9));

        q2.play(vec![15, 20, 25, 30]);

        assert_eq!(q2.current_track(), Some(15));

        q2.advance();

        assert_eq!(q2.current_track(), Some(-8));
    }
}

impl Queue {
    pub fn new(spotify: Arc<Spotify>) -> Queue {
        Queue {
            queue: Arc::new(RwLock::new(Vec::new())),
            current_track: RwLock::new(None),
            repeat: RwLock::new(RepeatSetting::None),
            random_order: RwLock::new(None),
            spotify,
        }
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

    pub fn get_current(&self) -> Option<Track> {
        match *self.current_track.read().unwrap() {
            Some(index) => Some(self.queue.read().unwrap()[index].clone()),
            None => None,
        }
    }

    pub fn append(&self, track: &Track) {
        let mut random_order = self.random_order.write().unwrap();
        if let Some(order) = random_order.as_mut() {
            let index = order.len().saturating_sub(1);
            order.push(index);
        }

        let mut q = self.queue.write().unwrap();
        q.push(track.clone());
    }

    pub fn append_next(&self, tracks: Vec<&Track>) -> usize {
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
            q.remove(index);
        }

        // if the queue is empty or we are at the end of the queue, stop
        // playback
        let len = self.queue.read().unwrap().len();
        if len == 0 || index == len {
            self.stop();
            return;
        }

        // if we are deleting the currently playing track, play the track with
        // the same index again, because the next track is now at the position
        // of the one we deleted
        let current = *self.current_track.read().unwrap();
        if let Some(current_track) = current {
            if index == current_track {
                self.play(index, false);
            } else if index < current_track {
                let mut current = self.current_track.write().unwrap();
                current.replace(current_track - 1);
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

    pub fn play(&self, index: usize, reshuffle: bool) {
        if let Some(track) = &self.queue.read().unwrap().get(index) {
            self.spotify.load(&track);
            let mut current = self.current_track.write().unwrap();
            current.replace(index);
            self.spotify.play();
            self.spotify.update_track();
        }

        if reshuffle && self.get_shuffle() {
            self.generate_random_order()
        }
    }

    pub fn toggleplayback(&self) {
        self.spotify.toggleplayback();
    }

    pub fn stop(&self) {
        let mut current = self.current_track.write().unwrap();
        *current = None;
        self.spotify.stop();
    }

    pub fn next(&self, manual: bool) {
        let q = self.queue.read().unwrap();
        let current = *self.current_track.read().unwrap();
        let repeat = *self.repeat.read().unwrap();

        if repeat == RepeatSetting::RepeatTrack && !manual {
            if let Some(index) = current {
                self.play(index, false);
            }
        } else if let Some(index) = self.next_index() {
            self.play(index, false);
        } else if repeat == RepeatSetting::RepeatPlaylist && q.len() > 0 {
            let random_order = self.random_order.read().unwrap();
            self.play(random_order.as_ref().map(|o| o[0]).unwrap_or(0), false);
        } else {
            self.spotify.stop();
        }
    }

    pub fn previous(&self) {
        if let Some(index) = self.previous_index() {
            self.play(index, false);
        } else {
            self.spotify.stop();
        }
    }

    pub fn get_repeat(&self) -> RepeatSetting {
        let repeat = self.repeat.read().unwrap();
        *repeat
    }

    pub fn set_repeat(&self, new: RepeatSetting) {
        let mut repeat = self.repeat.write().unwrap();
        *repeat = new;
    }

    pub fn get_shuffle(&self) -> bool {
        let random_order = self.random_order.read().unwrap();
        random_order.is_some()
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
        if new {
            self.generate_random_order();
        } else {
            let mut random_order = self.random_order.write().unwrap();
            *random_order = None;
        }
    }

    pub fn get_spotify(&self) -> Arc<Spotify> {
        self.spotify.clone()
    }
}
