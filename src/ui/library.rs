use std::sync::Arc;

use cursive::view::ViewWrapper;
use cursive::Cursive;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::queue::Queue;
use crate::traits::ViewExt;
use crate::ui::listview::ListView;
use crate::ui::playlists::PlaylistsView;
use crate::ui::tabview::TabView;

pub struct LibraryView {
    tabs: TabView,
    display_name: Option<String>,
}

impl LibraryView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>) -> Self {
        let tabs = TabView::new()
            .tab(
                "tracks",
                "Tracks",
                ListView::new(library.tracks.clone(), queue.clone(), library.clone()),
            )
            .tab(
                "albums",
                "Albums",
                ListView::new(library.albums.clone(), queue.clone(), library.clone()),
            )
            .tab(
                "artists",
                "Artists",
                ListView::new(library.artists.clone(), queue.clone(), library.clone()),
            )
            .tab(
                "playlists",
                "Playlists",
                PlaylistsView::new(queue.clone(), library.clone()),
            )
            .tab(
                "podcasts",
                "Podcasts",
                ListView::new(library.shows.clone(), queue, library.clone()),
            );

        Self {
            tabs,
            display_name: library.display_name.clone(),
        }
    }
}

impl ViewWrapper for LibraryView {
    wrap_impl!(self.tabs: TabView);
}

impl ViewExt for LibraryView {
    fn title(&self) -> String {
        if let Some(name) = &self.display_name {
            format!("Library of {}", name)
        } else {
            "Library".to_string()
        }
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.tabs.on_command(s, cmd)
    }
}
