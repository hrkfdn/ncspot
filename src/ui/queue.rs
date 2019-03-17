use cursive::traits::Identifiable;
use cursive::view::ViewWrapper;
use cursive::views::{IdView, ScrollView};

use std::sync::Arc;

use queue::Queue;
use track::Track;
use ui::listview::ListView;

pub struct QueueView {
    list: ScrollView<IdView<ListView<Track>>>,
}

impl QueueView {
    pub fn new(queue: Arc<Queue>) -> QueueView {
        let list = ListView::new(queue.queue.clone(), queue.clone()).with_id("queue_list");
        let scrollable = ScrollView::new(list);

        QueueView { list: scrollable }
    }
}

impl ViewWrapper for QueueView {
    wrap_impl!(self.list: ScrollView<IdView<ListView<Track>>>);
}
