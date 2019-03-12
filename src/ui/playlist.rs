use std::sync::{Arc, Mutex, RwLock};
use std::thread;

use cursive::direction::Orientation;
use cursive::event::Key;
use cursive::traits::Boxable;
use cursive::traits::Identifiable;
use cursive::views::*;
use cursive::Cursive;
use rspotify::spotify::model::playlist::SimplifiedPlaylist;

use events::{Event, EventManager};
use queue::Queue;
use spotify::Spotify;
use track::Track;
use ui::splitbutton::SplitButton;

#[derive(Clone)]
pub struct Playlist {
    meta: SimplifiedPlaylist,
    tracks: Vec<Track>,
}

pub enum PlaylistEvent {
    NewList(Playlist),
}

pub struct PlaylistView {
    pub view: Option<BoxView<ScrollView<IdView<LinearLayout>>>>,
    queue: Arc<Mutex<Queue>>,
    playlists: Arc<RwLock<Vec<Playlist>>>,
}

impl PlaylistView {
    pub fn new(ev: EventManager, queue: Arc<Mutex<Queue>>, spotify: Arc<Spotify>) -> PlaylistView {
        let playlists_view = LinearLayout::new(Orientation::Vertical).with_id("playlists");
        let scrollable = ScrollView::new(playlists_view).full_screen();
        let playlists = Arc::new(RwLock::new(Vec::new()));

        {
            let spotify = spotify.clone();
            let playlists = playlists.clone();
            Self::load_playlists(ev, spotify, playlists);
        }

        PlaylistView {
            view: Some(scrollable),
            queue: queue,
            playlists: playlists,
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

    fn load_playlist(list: &SimplifiedPlaylist, spotify: Arc<Spotify>) -> Playlist {
        debug!("got list: {}", list.name);
        let id = list.id.clone();

        let mut collected_tracks = Vec::new();

        let mut tracks_result = spotify.user_playlist_tracks(&id, 100, 0).ok();
        while let Some(ref tracks) = tracks_result.clone() {
            for listtrack in &tracks.items {
                collected_tracks.push(Track::new(&listtrack.track));
            }
            debug!("got {} tracks", tracks.items.len());

            // load next batch if necessary
            tracks_result = match tracks.next {
                Some(_) => {
                    debug!("requesting tracks again..");
                    spotify
                        .user_playlist_tracks(&id, 100, tracks.offset + tracks.items.len() as u32)
                        .ok()
                }
                None => None,
            }
        }
        Playlist {
            meta: list.clone(),
            tracks: collected_tracks,
        }
    }

    fn load_playlists(
        ev: EventManager,
        spotify: Arc<Spotify>,
        playlists: Arc<RwLock<Vec<Playlist>>>,
    ) {
        thread::spawn(move || {
            debug!("loading playlists");
            let mut lists_result = spotify.current_user_playlist(50, 0).ok();
            while let Some(ref lists) = lists_result.clone() {
                for list in &lists.items {
                    let playlist = Self::load_playlist(&list, spotify.clone());
                    ev.send(Event::Playlist(PlaylistEvent::NewList(playlist.clone())));
                    playlists
                        .write()
                        .expect("could not acquire write lock on playlists")
                        .push(playlist);
                }

                // load next batch if necessary
                lists_result = match lists.next {
                    Some(_) => {
                        debug!("requesting playlists again..");
                        spotify
                            .current_user_playlist(50, lists.offset + lists.items.len() as u32)
                            .ok()
                    }
                    None => None,
                }
            }
        });
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

            for list in self
                .playlists
                .read()
                .expect("could not acquire read lock on playlists")
                .iter()
            {
                let button = self.create_button(&list);
                playlists.add_child(button);
            }
        }
    }

    pub fn handle_ev(&self, cursive: &mut Cursive, event: PlaylistEvent) {
        let view_ref: Option<ViewRef<LinearLayout>> = cursive.find_id("playlists");

        if let Some(mut playlists) = view_ref {
            match event {
                PlaylistEvent::NewList(list) => {
                    let button = self.create_button(&list);
                    playlists.add_child(button);
                }
            }
        }
    }
}
