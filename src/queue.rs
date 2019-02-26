use std::collections::vec_deque::Iter;
use std::collections::VecDeque;

use rspotify::spotify::model::track::FullTrack;

pub struct Queue {
    queue: VecDeque<FullTrack>,
}

impl Queue {
    pub fn new() -> Queue {
        Queue {
            queue: VecDeque::new(),
        }
    }
    pub fn remove(&mut self, index: usize) -> Option<FullTrack> {
        self.queue.remove(index)
    }
    pub fn enqueue(&mut self, track: FullTrack) {
        self.queue.push_back(track);
    }
    pub fn dequeue(&mut self) -> Option<FullTrack> {
        self.queue.pop_front()
    }
    pub fn iter(&self) -> Iter<FullTrack> {
        self.queue.iter()
    }
}
