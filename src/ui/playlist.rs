use std::sync::{Arc, Mutex};

use cursive::direction::Orientation;
use cursive::event::Key;
use cursive::traits::Boxable;
use cursive::traits::Identifiable;
use cursive::views::*;
use cursive::Cursive;
use rspotify::spotify::model::playlist::SimplifiedPlaylist;

use queue::Queue;
use spotify::Spotify;
use track::Track;
use ui::splitbutton::SplitButton;

pub enum PlaylistEvent {
    Refresh,
}

pub struct PlaylistView {
    pub view: Option<BoxView<ScrollView<IdView<LinearLayout>>>>,
    queue: Arc<Mutex<Queue>>,
    spotify: Arc<Spotify>,
}

impl PlaylistView {
    pub fn new(queue: Arc<Mutex<Queue>>, spotify: Arc<Spotify>) -> PlaylistView {
        let playlists = LinearLayout::new(Orientation::Vertical).with_id("playlists");
        let scrollable = ScrollView::new(playlists).full_screen();

        PlaylistView {
            view: Some(scrollable),
            queue: queue,
            spotify: spotify,
        }
    }
    fn clear_playlists(&self, playlists: &mut ViewRef<LinearLayout>) {
        while playlists.len() > 0 {
            playlists.remove_child(0);
        }
    }

    fn create_button(&self, playlist: &SimplifiedPlaylist) -> SplitButton {
        let collab = match playlist.collaborative {
            true => "collaborative",
            false => "",
        };

        let mut button = SplitButton::new(&playlist.name, collab);

        // <enter> plays the selected playlist
        {
            let id = playlist.id.clone();
            let spotify_ref = self.spotify.clone();
            let queue_ref = self.queue.clone();
            button.add_callback(Key::Enter, move |_s| {
                let tracks = spotify_ref.user_playlist_tracks(&id).unwrap().items;
                let mut locked_queue = queue_ref.lock().expect("Could not aquire lock");

                let mut first_played = false;
                for playlist_track in tracks {
                    let index = locked_queue.append_next(&Track::new(&playlist_track.track));
                    if !first_played {
                        locked_queue.play(index);
                        first_played = true;
                    }
                }
            });
        }

        // <space> queues the selected playlist
        {
            let id = playlist.id.clone();
            let spotify_ref = self.spotify.clone();
            let queue_ref = self.queue.clone();
            button.add_callback(' ', move |_s| {
                let tracks = spotify_ref.user_playlist_tracks(&id).unwrap().items;
                let mut locked_queue = queue_ref.lock().expect("Could not aquire lock");
                for playlist_track in tracks {
                    locked_queue.append(&Track::new(&playlist_track.track));
                }
            });
        }

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
