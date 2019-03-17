use std::sync::Arc;

use cursive::traits::Identifiable;
use cursive::view::{ViewWrapper};
use cursive::views::{IdView, ScrollView};

use playlists::{Playlist, Playlists};
use queue::Queue;
use ui::listview::ListView;

pub struct PlaylistView {
    list: ScrollView<IdView<ListView<Playlist>>>
}

impl PlaylistView {
    pub fn new(playlists: &Playlists, queue: Arc<Queue>) -> PlaylistView {
        let list = ListView::new(playlists.store.clone(), queue).with_id("list");
        let scrollable = ScrollView::new(list);

        PlaylistView {
            list: scrollable
        }
    }
}

impl ViewWrapper for PlaylistView {
    wrap_impl!(self.list: ScrollView<IdView<ListView<Playlist>>>);
}
