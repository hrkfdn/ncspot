use std::collections::vec_deque::Iter;
use std::collections::VecDeque;

use track::Track;

use events::{Event, EventManager};

pub struct Queue {
    queue: VecDeque<Track>,
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
    pub fn remove(&mut self, index: usize) -> Option<Track> {
        match self.queue.remove(index) {
            Some(track) => {
                debug!("Removed from queue: {}", &track);
                self.ev.send(Event::Queue(QueueChange::Remove(index)));
                Some(track)
            }
            None => None,
        }
    }
    pub fn enqueue(&mut self, track: Track) {
        debug!("Queued: {}", &track);
        self.queue.push_back(track);
        self.ev.send(Event::Queue(QueueChange::Enqueue));
    }
    pub fn dequeue(&mut self) -> Option<Track> {
        match self.queue.pop_front() {
            Some(track) => {
                debug!("Dequeued : {}", track);
                self.ev.send(Event::Queue(QueueChange::Dequeue));
                Some(track)
            }
            None => None,
        }
    }
    pub fn peek(&self) -> Option<&Track> {
        self.queue.get(0)
    }
    pub fn iter(&self) -> Iter<Track> {
        self.queue.iter()
    }
}
