use std::sync::{Arc, RwLock};
use std::thread;

use cursive::Cursive;
use cursive::view::ViewWrapper;
use rspotify::model::AlbumType;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::traits::ViewExt;
use crate::ui::listview::ListView;
use crate::ui::tabbedview::TabbedView;

pub struct ArtistView {
    artist: Artist,
    tabs: TabbedView,
}

impl ArtistView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, artist: &Artist) -> Self {
        let spotify = queue.get_spotify();

        let albums_view =
            Self::albums_view(artist, AlbumType::Album, queue.clone(), library.clone());
        let singles_view =
            Self::albums_view(artist, AlbumType::Single, queue.clone(), library.clone());

        let top_tracks: Arc<RwLock<Vec<Track>>> = Arc::new(RwLock::new(Vec::new()));
        {
            let top_tracks = top_tracks.clone();
            let spotify = spotify.clone();
            let id = artist.id.clone();
            let library = library.clone();
            thread::spawn(move || {
                if let Some(id) = id
                    && let Ok(tracks) = spotify.api.artist_top_tracks(&id)
                {
                    top_tracks.write().unwrap().extend(tracks);
                    library.trigger_redraw();
                }
            });
        }

        let related: Arc<RwLock<Vec<Artist>>> = Arc::new(RwLock::new(Vec::new()));
        {
            let related = related.clone();
            let id = artist.id.clone();
            let library = library.clone();
            thread::spawn(move || {
                if let Some(id) = id
                    && let Ok(artists) = spotify.api.artist_related_artists(&id)
                {
                    related.write().unwrap().extend(artists);
                    library.trigger_redraw();
                }
            });
        }

        let mut tabs = TabbedView::new();

        if let Some(tracks) = artist.tracks.as_ref() {
            let tracks = tracks.clone();

            tabs.add_tab(
                "Saved Tracks",
                ListView::new(
                    Arc::new(RwLock::new(tracks)),
                    queue.clone(),
                    library.clone(),
                ),
            );
        }
        tabs.add_tab(
            "Top 10",
            ListView::new(top_tracks, queue.clone(), library.clone()),
        );
        tabs.add_tab("Albums", albums_view);
        tabs.add_tab("Singles", singles_view);
        tabs.add_tab("Related Artists", ListView::new(related, queue, library));

        Self {
            artist: artist.clone(),
            tabs,
        }
    }

    fn albums_view(
        artist: &Artist,
        album_type: AlbumType,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> ListView<Album> {
        if let Some(artist_id) = &artist.id {
            let spotify = queue.get_spotify();
            let albums_page = spotify.api.artist_albums(artist_id, Some(album_type));
            let view = ListView::new(albums_page.items.clone(), queue, library);
            albums_page.apply_pagination(view.get_pagination());

            view
        } else {
            ListView::new(Arc::new(RwLock::new(Vec::new())), queue, library)
        }
    }
}

impl ViewWrapper for ArtistView {
    wrap_impl!(self.tabs: TabbedView);
}

impl ViewExt for ArtistView {
    fn title(&self) -> String {
        self.artist.name.clone()
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.tabs.on_command(s, cmd)
    }
}
