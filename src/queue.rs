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
    pub fn enqueue(&mut self, track: FullTrack) {
        self.queue.push_back(track);
    }
    pub fn dequeue(&mut self) -> Option<FullTrack> {
        self.queue.pop_front()
    }
}
