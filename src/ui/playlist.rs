use std::sync::{Arc, Mutex};

use cursive::direction::Orientation;
use cursive::traits::Boxable;
use cursive::traits::Identifiable;
use cursive::views::*;
use cursive::Cursive;
use rspotify::spotify::model::playlist::SimplifiedPlaylist;

use queue::Queue;
use spotify::Spotify;
use track::Track;

pub enum PlaylistEvent {
    Refresh,
}

pub struct PlaylistView {
    pub view: Option<Panel<BoxView<BoxView<ScrollView<IdView<LinearLayout>>>>>>, // FIXME: wow
    queue: Arc<Mutex<Queue>>,
    spotify: Arc<Spotify>,
}

impl PlaylistView {
    pub fn new(queue: Arc<Mutex<Queue>>, spotify: Arc<Spotify>) -> PlaylistView {
        let playlists = LinearLayout::new(Orientation::Vertical).with_id("playlists");
        let scrollable = ScrollView::new(playlists).full_width().full_height();
        let panel = Panel::new(scrollable).title("Playlists");

        PlaylistView {
            view: Some(panel),
            queue: queue,
            spotify: spotify,
        }
    }
    fn clear_playlists(&self, playlists: &mut ViewRef<LinearLayout>) {
        while playlists.len() > 0 {
            playlists.remove_child(0);
        }
    }

    fn create_button(&self, playlist: &SimplifiedPlaylist) -> Button {
        let spotify_ref = self.spotify.clone();
        let queue_ref = self.queue.clone();

        // TODO: implement a custom view that displays playlists similar to
        // TrackButton with more detail, e.g. number of tracks, total duration.
        let id = playlist.id.clone();
        let button = Button::new_raw(playlist.name.clone(), move |_s| {
            let tracks = spotify_ref.user_playlist_tracks(&id).unwrap().items;
            let mut locked_queue = queue_ref.lock().expect("Could not aquire lock");
            for playlist_track in tracks {
                locked_queue.enqueue(Track::new(&playlist_track.track));
            }
        });

        button
    }

    fn show_playlists(&self, playlists: &mut ViewRef<LinearLayout>) {
        let playlists_response = self.spotify.current_user_playlist(50, 0).unwrap().items;
        for playlist in &playlists_response {
            let button = self.create_button(playlist);
            playlists.add_child(button);
        }
    }

    pub fn handle_ev(&self, cursive: &mut Cursive, event: PlaylistEvent) {
        let view_ref: Option<ViewRef<LinearLayout>> = cursive.find_id("playlists");

        if let Some(mut playlists) = view_ref {
            match event {
                PlaylistEvent::Refresh => {
                    // FIXME: do this only once at startup or when requested by
                    // the user
                    self.clear_playlists(&mut playlists);
                    self.show_playlists(&mut playlists);
                }
            }
        }
    }
}
