use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TryIter, unbounded};
use cursive::{CbSink, Cursive};
use log::debug;

use crate::queue::QueueEvent;
use crate::rate_limit::ClientLabel;
use crate::spotify::PlayerEvent;

/// Events that can be sent to and handled by the main event loop (the one drawing the TUI).
pub enum Event {
    Player(PlayerEvent),
    Queue(QueueEvent),
    SessionDied,
    IpcInput(String),
    /// A Web API client started being rate limited by Spotify (HTTP 429).
    RateLimited {
        client: ClientLabel,
        retry_after: Duration,
    },
    /// The personal Web API client's token can't be refreshed without a new login, so it was
    /// disabled for the rest of the session.
    PersonalClientDisabled {
        reason: String,
    },
}

/// Manager that can be used to send and receive messages across threads.
#[derive(Clone)]
pub struct EventManager {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    cursive_sink: CbSink,
}

impl EventManager {
    /// Like [EventManager::send], but reports whether the UI was notified. `tx.send` cannot fail
    /// while the manager holds `rx`, so this returns false only when the cursive sink is closed
    /// (the event is still queued). For senders on background threads that may outlive the UI.
    pub fn try_send(&self, event: Event) -> bool {
        self.tx.send(event).is_ok() && self.try_trigger()
    }

    /// Like [EventManager::trigger], but returns false instead of panicking if the event loop
    /// has already shut down.
    pub fn try_trigger(&self) -> bool {
        self.cursive_sink.send(Box::new(Cursive::noop)).is_ok()
    }

    /// Create an EventManager backed by a discarded crossbeam channel, for use in tests where no
    /// events need to be processed.
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        let (cb_sink, _): (CbSink, _) = crossbeam_channel::unbounded();
        Self::new(cb_sink)
    }

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
        if !self.try_send(event) {
            debug!("event loop closed, dropping event");
        }
    }

    /// Send a no-op to the Cursive event loop to trigger immediate processing of events.
    pub fn trigger(&self) {
        if !self.try_trigger() {
            debug!("event loop closed, skipping redraw trigger");
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn try_trigger_returns_false_when_the_ui_is_gone() {
        assert!(!EventManager::new_for_test().try_trigger());
    }

    #[test]
    fn trigger_does_not_panic_when_the_ui_is_gone() {
        EventManager::new_for_test().trigger();
    }

    #[test]
    fn send_does_not_panic_when_the_ui_is_gone() {
        EventManager::new_for_test().send(Event::SessionDied);
    }

    #[test]
    fn try_trigger_returns_true_while_the_ui_is_alive() {
        let (sink, _rx) = crossbeam_channel::unbounded();
        let events = EventManager::new(sink);
        assert!(events.try_trigger());
    }
}
