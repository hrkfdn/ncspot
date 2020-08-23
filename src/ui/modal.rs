use cursive::event::{Event, EventResult};
use cursive::view::{View, ViewWrapper};

pub struct Modal<T: View> {
    block_events: bool,
    inner: T,
}

impl<T: View> Modal<T> {
    pub fn new(inner: T) -> Self {
        Modal {
            block_events: true,
            inner,
        }
    }
    pub fn new_ext(inner: T) -> Self {
        Modal {
            block_events: false,
            inner,
        }
    }
}

impl<T: View> ViewWrapper for Modal<T> {
    wrap_impl!(self.inner: T);
    fn wrap_on_event(&mut self, ch: Event) -> EventResult {
        match self.inner.on_event(ch) {
            EventResult::Consumed(cb) => EventResult::Consumed(cb),
            _ => match self.block_events {
                true => EventResult::Consumed(None),
                false => EventResult::Ignored,
            },
        }
    }
}
