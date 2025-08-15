use crossbeam_channel::{Receiver, Sender, TryIter, unbounded};
use cursive::{CbSink, Cursive};

use crate::queue::QueueEvent;
use crate::spotify::PlayerEvent;

/// Events that can be sent to and handled by the main event loop (the one drawing the TUI).
pub enum Event {
    Player(PlayerEvent),
    Queue(QueueEvent),
    SessionDied,
    IpcInput(String),
}

/// Manager that can be used to send and receive messages across threads.
#[derive(Clone)]
pub struct EventManager {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    cursive_sink: CbSink,
}

impl EventManager {
    pub fn new(cursive_sink: CbSink) -> Self {
        let (tx, rx) = unbounded();

        Self {
            tx,
            rx,
            cursive_sink,
        }
    }

    /// Return a non-blocking iterator over the messages awaiting handling. Calling `next()` on the
    /// iterator never blocks.
    pub fn msg_iter(&self) -> TryIter<'_, Event> {
        self.rx.try_iter()
    }

    /// Send a new event to be handled.
    pub fn send(&self, event: Event) {
        self.tx.send(event).unwrap();
        self.trigger();
    }

    /// Send a no-op to the Cursive event loop to trigger immediate processing of events.
    pub fn trigger(&self) {
        self.cursive_sink.send(Box::new(Cursive::noop)).unwrap();
    }
}
