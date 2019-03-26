use cursive::event::{Event, EventResult};
use cursive::view::{View, ViewWrapper};

pub struct Modal<T: View> {
    inner: T,
}

impl<T: View> Modal<T> {
    pub fn new(inner: T) -> Self {
        Modal { inner }
    }
}

impl<T: View> ViewWrapper for Modal<T> {
    wrap_impl!(self.inner: T);
    fn wrap_on_event(&mut self, ch: Event) -> EventResult {
        match self.inner.on_event(ch) {
            EventResult::Consumed(cb) => EventResult::Consumed(cb),
            _ => EventResult::Consumed(None),
        }
    }
}
