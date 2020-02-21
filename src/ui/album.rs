use std::sync::{Arc, RwLock};

use cursive::view::ViewWrapper;
use cursive::Cursive;

use album::Album;
use artist::Artist;
use command::Command;
use commands::CommandResult;
use library::Library;
use queue::Queue;
use traits::ViewExt;
use ui::listview::ListView;
use ui::tabview::TabView;

pub struct AlbumView {
    album: Album,
    tabs: TabView,
}

impl AlbumView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, album: &Album) -> Self {
        let mut album = album.clone();

        album.load_tracks(queue.get_spotify());

        let tracks = if let Some(t) = album.tracks.as_ref() {
            t.clone()
        } else {
            Vec::new()
        };

        let artists = album
            .artist_ids
            .iter()
            .zip(album.artists.iter())
            .map(|(id, name)| Artist::new(id.clone(), name.clone()))
            .collect();

        let tabs = TabView::new()
            .tab(
                "tracks",
                "Tracks",
                ListView::new(
                    Arc::new(RwLock::new(tracks)),
                    queue.clone(),
                    library.clone(),
                ),
            )
            .tab(
                "artists",
                "Artists",
                ListView::new(Arc::new(RwLock::new(artists)), queue, library),
            );

        Self { album, tabs }
    }
}

impl ViewWrapper for AlbumView {
    wrap_impl!(self.tabs: TabView);
}

impl ViewExt for AlbumView {
    fn title(&self) -> String {
        format!("{} ({})", self.album.title, self.album.year)
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.tabs.on_command(s, cmd)
    }
}
