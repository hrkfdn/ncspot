use crossbeam_channel::{unbounded, Sender, Receiver, TryIter};
use cursive::{Cursive, CbFunc};

pub enum Event {
    QueueUpdate,
}

pub type EventSender = Sender<Event>;

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

    pub fn sink(&mut self) -> EventSender {
        self.tx.clone()
    }

    pub fn trigger(&self) {
        // send a no-op to trigger event loop processing
        self.cursive_sink.send(Box::new(|_s: &mut Cursive| {}));
    }
}
