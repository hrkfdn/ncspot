use std::sync::Arc;

use cursive::view::ViewWrapper;
use cursive::Cursive;

use commands::CommandResult;
use library::Library;
use queue::Queue;
use traits::ViewExt;
use ui::listview::ListView;
use ui::playlists::PlaylistsView;
use ui::tabview::TabView;

pub struct LibraryView {
    tabs: TabView,
}

impl LibraryView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>) -> Self {
        let tabs = TabView::new()
            .tab("tracks", "Tracks", ListView::new(library.tracks.clone(), queue.clone()))
            .tab("albums", "Albums", ListView::new(library.albums.clone(), queue.clone()))
            .tab("artists", "Artists", ListView::new(library.artists.clone(), queue.clone()))
            .tab("playlists", "Playlists", PlaylistsView::new(queue.clone(), library.clone()));

        Self {
            tabs
        }
    }
}

impl ViewWrapper for LibraryView {
    wrap_impl!(self.tabs: TabView);
}

impl ViewExt for LibraryView {
    fn on_command(
        &mut self,
        s: &mut Cursive,
        cmd: &str,
        args: &[String],
    ) -> Result<CommandResult, String> {
        self.tabs.on_command(s, cmd, args)
    }
}
