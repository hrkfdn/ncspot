use std::sync::{Arc, RwLock};
use std::thread;

use cursive::view::ViewWrapper;
use cursive::Cursive;

use artist::Artist;
use commands::CommandResult;
use library::Library;
use queue::Queue;
use track::Track;
use traits::ViewExt;
use ui::listview::ListView;
use ui::tabview::TabView;

pub struct ArtistView {
    artist: Artist,
    tabs: TabView,
}

impl ArtistView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, artist: &Artist) -> Self {
        let mut artist = artist.clone();

        let spotify = queue.get_spotify();
        artist.load_albums(spotify.clone());

        let albums = if let Some(a) = artist.albums.as_ref() {
            a.clone()
        } else {
            Vec::new()
        };

        let top_tracks: Arc<RwLock<Vec<Track>>> = Arc::new(RwLock::new(Vec::new()));
        {
            let top_tracks = top_tracks.clone();
            let spotify = spotify.clone();
            let id = artist.id.clone();
            thread::spawn(move || {
                if let Some(tracks) = spotify.artist_top_tracks(id) {
                    top_tracks.write().unwrap().extend(tracks);
                }
            });
        }

        let related: Arc<RwLock<Vec<Artist>>> = Arc::new(RwLock::new(Vec::new()));
        {
            let related = related.clone();
            let spotify = spotify.clone();
            let id = artist.id.clone();
            thread::spawn(move || {
                if let Some(artists) = spotify.artist_related_artists(id) {
                    related.write().unwrap().extend(artists);
                }
            });
        }

        let mut tabs = TabView::new();

        if let Some(tracks) = artist.tracks.as_ref() {
            let tracks = tracks.clone();

            tabs.add_tab(
                "tracks",
                "Saved Tracks",
                ListView::new(Arc::new(RwLock::new(tracks)), queue.clone(), library.clone()),
            );
        }

        tabs.add_tab(
            "top_tracks",
            "Top 10",
            ListView::new(top_tracks.clone(), queue.clone(), library.clone()),
        );

        tabs.add_tab(
            "albums",
            "Albums",
            ListView::new(Arc::new(RwLock::new(albums)), queue.clone(), library.clone()),
        );

        tabs.add_tab(
            "related",
            "Related Artists",
            ListView::new(related.clone(), queue.clone(), library.clone()),
        );

        Self {
            artist,
            tabs
        }
    }
}

impl ViewWrapper for ArtistView {
    wrap_impl!(self.tabs: TabView);
}

impl ViewExt for ArtistView {
    fn title(&self) -> String {
        self.artist.name.clone()
    }

    fn on_command(
        &mut self,
        s: &mut Cursive,
        cmd: &str,
        args: &[String],
    ) -> Result<CommandResult, String> {
        self.tabs.on_command(s, cmd, args)
    }
}
