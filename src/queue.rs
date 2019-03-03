use std::collections::vec_deque::Iter;
use std::collections::VecDeque;

use rspotify::spotify::model::track::FullTrack;

use events::{Event, EventManager};

pub struct Queue {
    queue: VecDeque<FullTrack>,
    ev: EventManager,
}

pub enum QueueChange {
    Dequeue,
    Enqueue,
    Remove(usize),
    Show,
}

impl Queue {
    pub fn new(ev: EventManager) -> Queue {
        Queue {
            queue: VecDeque::new(),
            ev: ev,
        }
    }
    pub fn remove(&mut self, index: usize) -> Option<FullTrack> {
        match self.queue.remove(index) {
            Some(track) => {
                debug!("Removed from queue: {}", &track.name);
                self.ev.send(Event::Queue(QueueChange::Remove(index)));
                Some(track)
            }
            None => None,
        }
    }
    pub fn enqueue(&mut self, track: FullTrack) {
        debug!("Queued: {}", &track.name);
        self.queue.push_back(track);
        self.ev.send(Event::Queue(QueueChange::Enqueue));
    }
    pub fn dequeue(&mut self) -> Option<FullTrack> {
        match self.queue.pop_front() {
            Some(track) => {
                debug!("Dequeued : {}", track.name);
                self.ev.send(Event::Queue(QueueChange::Dequeue));
                Some(track)
            }
            None => None,
        }
    }
    pub fn peek(&self) -> Option<&FullTrack> {
        self.queue.get(0)
    }
    pub fn iter(&self) -> Iter<FullTrack> {
        self.queue.iter()
    }
}
