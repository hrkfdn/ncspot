use crossbeam_channel::{unbounded, Receiver, Sender, TryIter};
use cursive::{CbFunc, Cursive};

use spotify::PlayerState;

pub enum Event {
    QueueUpdate,
    PlayState(PlayerState),
}

pub type EventSender = Sender<Event>;

#[derive(Clone)]
pub struct EventManager {
    tx: EventSender,
    rx: Receiver<Event>,
    cursive_sink: Sender<Box<dyn CbFunc>>,
}

impl EventManager {
    pub fn new(cursive_sink: Sender<Box<dyn CbFunc>>) -> EventManager {
        let (tx, rx) = unbounded();

        EventManager {
            tx: tx,
            rx: rx,
            cursive_sink: cursive_sink,
        }
    }

    pub fn msg_iter(&self) -> TryIter<Event> {
        self.rx.try_iter()
    }

    pub fn send(&self, event: Event) {
        self.tx.send(event).expect("could not send event");
        self.trigger();
    }

    pub fn trigger(&self) {
        // send a no-op to trigger event loop processing
        self.cursive_sink.send(Box::new(|_s: &mut Cursive| {})).expect("could not send no-op event to cursive");
    }
}
