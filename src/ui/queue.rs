use cursive::traits::Identifiable;
use cursive::view::ViewWrapper;
use cursive::views::IdView;

use std::sync::Arc;

use queue::Queue;
use track::Track;
use ui::listview::ListView;

pub struct QueueView {
    list: IdView<ListView<Track>>,
}

impl QueueView {
    pub fn new(queue: Arc<Queue>) -> QueueView {
        let list = ListView::new(queue.queue.clone(), queue.clone()).with_id("queue_list");

        QueueView { list: list }
    }
}

impl ViewWrapper for QueueView {
    wrap_impl!(self.list: IdView<ListView<Track>>);
}
