use std::sync::{Arc, RwLock};

use spotify::Spotify;
use track::Track;

pub struct Queue {
    pub queue: Arc<RwLock<Vec<Track>>>,
    current_track: RwLock<Option<usize>>,
    spotify: Arc<Spotify>,
}

impl Queue {
    pub fn new(spotify: Arc<Spotify>) -> Queue {
        Queue {
            queue: Arc::new(RwLock::new(Vec::new())),
            current_track: RwLock::new(None),
            spotify: spotify,
        }
    }

    pub fn next_index(&self) -> Option<usize> {
        match *self.current_track.read().unwrap() {
            Some(index) => {
                let next_index = index + 1;
                if next_index < self.queue.read().unwrap().len() {
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
            Some(index) => {
                if index > 0 {
                    Some(index - 1)
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
        let mut q = self.queue.write().unwrap();
        q.push(track.clone());
    }

    pub fn append_next(&self, track: &Track) -> usize {
        let next = self.next_index();
        let mut q = self.queue.write().unwrap();

        if let Some(next_index) = next {
            q.insert(next_index, track.clone());
            next_index
        } else {
            q.push(track.clone());
            q.len() - 1
        }
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
                self.play(index);
            } else if index < current_track {
                let mut current = self.current_track.write().unwrap();
                current.replace(current_track - 1);
            }
        }
    }

    pub fn clear(&self) {
        self.stop();

        let mut q = self.queue.write().unwrap();
        q.clear();
    }

    pub fn play(&self, index: usize) {
        let track = &self.queue.read().unwrap()[index];
        self.spotify.load(&track);
        let mut current = self.current_track.write().unwrap();
        current.replace(index);
        self.spotify.play();
        self.spotify.update_track();
    }

    pub fn toggleplayback(&self) {
        self.spotify.toggleplayback();
    }

    pub fn stop(&self) {
        let mut current = self.current_track.write().unwrap();
        *current = None;
        self.spotify.stop();
    }

    pub fn next(&self) {
        if let Some(index) = self.next_index() {
            self.play(index);
        } else {
            self.spotify.stop();
        }
    }

    pub fn previous(&self) {
        if let Some(index) = self.previous_index() {
            self.play(index);
        } else {
            self.spotify.stop();
        }
    }
}
