use std::sync::{Arc, Mutex};

use cursive::traits::Identifiable;
use cursive::direction::Orientation;
use cursive::traits::Boxable;
use cursive::views::*;
use cursive::Cursive;

use spotify::Spotify;
use queue::Queue;

pub enum PlaylistEvent {
    Refresh,
}

pub struct PlaylistView {
    pub view: Option<Panel<LinearLayout>>,
    queue: Arc<Mutex<Queue>>,
    spotify: Arc<Spotify>,
}

impl PlaylistView {
    pub fn new(queue: Arc<Mutex<Queue>>, spotify: Arc<Spotify>) -> PlaylistView {
        let playlist_overview: IdView<SelectView> = SelectView::new().with_id("playlists_overview");
        let scrollable = ScrollView::new(playlist_overview).full_width().full_height();
        let layout = LinearLayout::new(Orientation::Vertical).child(scrollable);
        let rootpanel = Panel::new(layout).title("Playlists");

        PlaylistView {
            view: Some(rootpanel),
            queue: queue,
            spotify: spotify,
        }
    }
    fn clear_playlists(&self, playlist_overview: &mut ViewRef<SelectView>) {
        playlist_overview.clear();
    }

    fn show_playlists(&self, playlist_overview: &mut ViewRef<SelectView>) {
        let playlists = self.spotify.current_user_playlist(50, 0).unwrap().items;
        for playlist in &playlists {
            playlist_overview.add_item(playlist.name.clone(), playlist.id.clone());
        }

        let spotify_ref = self.spotify.clone();
        let queue_ref = self.queue.clone();
        playlist_overview.set_on_submit(move |_s, id| {
            let tracks = spotify_ref.user_playlist_tracks(id).unwrap().items;

            let mut locked_queue = queue_ref.lock().expect("Could not aquire lock");
            for playlist_track in tracks {
                locked_queue.enqueue(playlist_track.track.clone());
            }
        });
    }

    pub fn handle_ev(&self, cursive: &mut Cursive, event: PlaylistEvent) {
        let view_ref: Option<ViewRef<SelectView>> = cursive.find_id("playlists_overview");

        if let Some(mut playlist_overview) = view_ref {
            match event {
                PlaylistEvent::Refresh => {
                    self.clear_playlists(&mut playlist_overview);
                    self.show_playlists(&mut playlist_overview);
                }
            }
        }
    }
}

