use std::sync::{Arc, RwLock};
use std::thread;

use cursive::view::ViewWrapper;
use cursive::views::ScrollView;
use cursive::Cursive;
use rspotify::model::AlbumType;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::model::artist::Artist;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::traits::ViewExt;
use crate::ui::tabview::TabView;

use super::list::List;

/// A view that shows all the content from a specific artist.
///
/// # Content
/// - Saved Tracks: Tracks that are also in the user library, if any;
/// - Top 10: The top 10 tracks of the artist;
/// - Albums: Artist albums;
/// - Singles: Artist singles;
/// - Related Artists: Artists that are similar to this artist.
pub struct ArtistView {
    artist: Artist,
    tabs: TabView,
}

impl ArtistView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, artist: &Artist) -> Self {
        let spotify = queue.get_spotify();

        let albums_view = Self::albums_view(artist, AlbumType::Album, queue.clone());
        let singles_view = Self::albums_view(artist, AlbumType::Single, queue);

        let top_tracks: Arc<RwLock<Vec<Track>>> = Arc::new(RwLock::new(Vec::new()));
        {
            let top_tracks = top_tracks.clone();
            let spotify = spotify.clone();
            let id = artist.id.clone();
            let library = library.clone();
            thread::spawn(move || {
                if let Some(id) = id {
                    if let Some(tracks) = spotify.api.artist_top_tracks(&id) {
                        top_tracks.write().unwrap().extend(tracks);
                        library.trigger_redraw();
                    }
                }
            });
        }

        let related: Arc<RwLock<Vec<Artist>>> = Arc::new(RwLock::new(Vec::new()));
        {
            let related = related.clone();
            let id = artist.id.clone();
            let library = library;
            thread::spawn(move || {
                if let Some(id) = id {
                    if let Some(artists) = spotify.api.artist_related_artists(&id) {
                        related.write().unwrap().extend(artists);
                        library.trigger_redraw();
                    }
                }
            });
        }

        let mut tabs = TabView::new();

        if let Some(tracks) = artist.tracks.as_ref() {
            let tracks = tracks.clone();

            tabs.add_tab(
                "tracks",
                ScrollView::new(List::from(tracks)),
            );
        }

        tabs.add_tab("top_tracks", ScrollView::new(List::new(top_tracks)));

        tabs.add_tab("albums", albums_view);
        tabs.add_tab("singles", singles_view);

        tabs.add_tab("related", ScrollView::new(List::new(related)));

        Self {
            artist: artist.clone(),
            tabs,
        }
    }

    fn albums_view(artist: &Artist, album_type: AlbumType, queue: Arc<Queue>) -> impl ViewExt {
        if let Some(artist_id) = &artist.id {
            let spotify = queue.get_spotify();
            let albums_page = spotify.api.artist_albums(artist_id, Some(album_type));
            ScrollView::new(List::new(albums_page.items))
            // FIX: albums_page.apply_pagination(view.get_pagination()) used to
            // be here!
        } else {
            ScrollView::new(List::new(Arc::new(RwLock::new(Vec::new()))))
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

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.tabs.on_command(s, cmd)
    }
}
