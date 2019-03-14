use std::sync::{Arc, Mutex};

use cursive::direction::Orientation;
use cursive::event::Key;
use cursive::traits::Boxable;
use cursive::traits::Identifiable;
use cursive::views::*;
use cursive::Cursive;

use playlists::{Playlist, PlaylistEvent, Playlists};
use queue::Queue;
use ui::splitbutton::SplitButton;

pub struct PlaylistView {
    pub view: Option<BoxView<ScrollView<IdView<LinearLayout>>>>,
    queue: Arc<Mutex<Queue>>,
    playlists: Playlists,
}

impl PlaylistView {
    pub fn new(playlists: &Playlists, queue: Arc<Mutex<Queue>>) -> PlaylistView {
        let playlists_view = LinearLayout::new(Orientation::Vertical).with_id("playlists");
        let scrollable = ScrollView::new(playlists_view).full_screen();

        PlaylistView {
            view: Some(scrollable),
            queue: queue,
            playlists: playlists.clone(),
        }
    }

    fn create_button(&self, playlist: &Playlist) -> SplitButton {
        let trackcount = format!("{} tracks", playlist.tracks.len());
        let mut button = SplitButton::new(&playlist.meta.name, &trackcount);

        // <enter> plays the selected playlist
        {
            let queue_ref = self.queue.clone();
            let playlist = playlist.clone();
            button.add_callback(Key::Enter, move |_s| {
                let mut locked_queue = queue_ref.lock().expect("could not acquire lock");
                let mut first_played = false;
                for track in playlist.tracks.iter() {
                    let index = locked_queue.append_next(track);
                    if !first_played {
                        locked_queue.play(index);
                        first_played = true;
                    }
                }
            });
        }

        // <space> queues the selected playlist
        {
            let queue_ref = self.queue.clone();
            let playlist = playlist.clone();
            button.add_callback(' ', move |_s| {
                let mut locked_queue = queue_ref.lock().expect("could not acquire lock");
                for track in playlist.tracks.iter() {
                    locked_queue.append(track);
                }
            });
        }

        button
    }

    fn clear_playlists(&self, playlists: &mut ViewRef<LinearLayout>) {
        while playlists.len() > 0 {
            playlists.remove_child(0);
        }
    }

    pub fn repopulate(&self, cursive: &mut Cursive) {
        let view_ref: Option<ViewRef<LinearLayout>> = cursive.find_id("playlists");
        if let Some(mut playlists) = view_ref {
            self.clear_playlists(&mut playlists);

            let playlist_store = &self
                .playlists
                .store
                .read()
                .expect("can't readlock playlists");
            info!("repopulating {} lists", playlist_store.playlists.len());
            for list in &playlist_store.playlists {
                let button = self.create_button(&list);
                playlists.add_child(button);
            }
        }
    }

    pub fn handle_ev(&self, cursive: &mut Cursive, event: PlaylistEvent) {
        let view_ref: Option<ViewRef<LinearLayout>> = cursive.find_id("playlists");

        if let Some(mut playlists) = view_ref {
            match event {
                PlaylistEvent::NewList(index, list) => {
                    let button = self.create_button(&list);

                    if let Some(_) = playlists.get_child(index) {
                        playlists.remove_child(index);
                    }
                    playlists.insert_child(index, button);
                }
            }
        }
    }
}
