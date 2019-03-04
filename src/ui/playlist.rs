use std::sync::{Arc, Mutex};

use cursive::direction::Orientation;
use cursive::traits::Boxable;
use cursive::views::*;

use spotify::Spotify;
use queue::Queue;

pub struct PlaylistView {
    pub view: Panel<LinearLayout>,
}

impl PlaylistView {
    pub fn new(queue: Arc<Mutex<Queue>>, spotify: Arc<Spotify>) -> PlaylistView {
        let mut results = SelectView::new();
        let playlists = spotify.current_user_playlist(50, 0).unwrap().items;

        for playlist in &playlists {
            results.add_item(playlist.name.clone(), playlist.id.clone());
        }

        let spotify_ref = spotify.clone();
        results.set_on_submit(move |_s, id| {
            let tracks = spotify_ref.user_playlist_tracks(id).unwrap().items;

            let mut l_queue = queue.lock().expect("Could not aquire lock");
            for playlist_track in tracks {
                l_queue.enqueue(playlist_track.track.clone());
            }
        });

        let scrollable = ScrollView::new(results).full_width().full_height();
        let layout = LinearLayout::new(Orientation::Vertical).child(scrollable);
        let rootpanel = Panel::new(layout).title("Playlists");

        PlaylistView {
            view: rootpanel,
        }
    }
}

