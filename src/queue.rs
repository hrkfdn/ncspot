use std::slice::Iter;
use std::sync::Arc;

use events::{Event, EventManager};
use spotify::Spotify;
use track::Track;

pub struct Queue {
    // TODO: put this in an RwLock instead of locking the whole Queue struct
    queue: Vec<Track>,
    current_track: Option<usize>,
    spotify: Arc<Spotify>,
    ev: EventManager,
}

impl Queue {
    pub fn new(ev: EventManager, spotify: Arc<Spotify>) -> Queue {
        Queue {
            queue: Vec::new(),
            current_track: None,
            spotify: spotify,
            ev: ev,
        }
    }

    pub fn next_index(&self) -> Option<usize> {
        match self.current_track {
            Some(index) => {
                let next_index = index + 1;
                if next_index < self.queue.len() {
                    Some(next_index)
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub fn get_current(&self) -> Option<&Track> {
        match self.current_track {
            Some(index) => Some(&self.queue[index]),
            None => None,
        }
    }

    pub fn append(&mut self, track: &Track) {
        self.queue.push(track.clone());
    }

    pub fn append_next(&mut self, track: &Track) -> usize {
        if let Some(next_index) = self.next_index() {
            self.queue.insert(next_index, track.clone());
            next_index
        } else {
            self.queue.push(track.clone());
            self.queue.len() - 1
        }
    }

    pub fn remove(&mut self, index: usize) {
        self.queue.remove(index);

        // if the queue is empty or we are at the end of the queue, stop
        // playback
        if self.queue.len() == 0 || index == self.queue.len() {
            self.stop();
            return;
        }

        // if we are deleting the currently playing track, play the track with
        // the same index again, because the next track is now at the position
        // of the one we deleted
        if let Some(current_track) = self.current_track {
            if index == current_track {
                self.play(index);
            } else if index < current_track {
                self.current_track = Some(current_track - 1);
            }
        }
    }

    pub fn clear(&mut self) {
        self.stop();
        self.queue.clear();

        // redraw queue if open
        self.ev.send(Event::ScreenChange("queue".to_owned()));
    }

    pub fn play(&mut self, index: usize) {
        let track = &self.queue[index];
        self.spotify.load(&track);
        self.current_track = Some(index);
        self.spotify.play();
        self.spotify.update_track();
    }

    pub fn toggleplayback(&self) {
        self.spotify.toggleplayback();
    }

    pub fn stop(&mut self) {
        self.current_track = None;
        self.spotify.stop();
    }

    pub fn next(&mut self) {
        if let Some(next_index) = self.next_index() {
            self.play(next_index);
        } else {
            self.spotify.stop();
        }
    }

    pub fn iter(&self) -> Iter<Track> {
        self.queue.iter()
    }
}
