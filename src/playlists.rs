use std::iter::Iterator;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, RwLockReadGuard};

use rspotify::spotify::model::playlist::{FullPlaylist, SimplifiedPlaylist};

use config;
use events::EventManager;
use queue::Queue;
use spotify::Spotify;
use track::Track;
use traits::ListItem;

const CACHE_FILE: &str = "playlists.db";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub snapshot_id: String,
    pub tracks: Vec<Track>,
}

#[derive(Clone)]
pub struct Playlists {
    pub store: Arc<RwLock<Vec<Playlist>>>,
    ev: EventManager,
    spotify: Arc<Spotify>,
    cache_path: PathBuf,
}

impl ListItem for Playlist {
    fn is_playing(&self, queue: Arc<Queue>) -> bool {
        let playing: Vec<String> = queue
            .queue
            .read()
            .unwrap()
            .iter()
            .map(|t| t.id.clone())
            .collect();
        let ids: Vec<String> = self.tracks.iter().map(|t| t.id.clone()).collect();
        !ids.is_empty() && playing == ids
    }

    fn display_left(&self) -> String {
        self.name.clone()
    }

    fn display_right(&self) -> String {
        format!("{} tracks", self.tracks.len())
    }

    fn play(&mut self, queue: Arc<Queue>) {
        let index = queue.append_next(self.tracks.iter().collect());
        queue.play(index, true);
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        for track in self.tracks.iter() {
            queue.append(track);
        }
    }
}

impl Playlists {
    pub fn new(ev: &EventManager, spotify: &Arc<Spotify>) -> Playlists {
        Playlists {
            store: Arc::new(RwLock::new(Vec::new())),
            ev: ev.clone(),
            spotify: spotify.clone(),
            cache_path: config::cache_path(CACHE_FILE),
        }
    }

    pub fn items(&self) -> RwLockReadGuard<Vec<Playlist>> {
        self.store
            .read()
            .expect("could not readlock listview content")
    }

    pub fn load_cache(&self) {
        if let Ok(contents) = std::fs::read_to_string(&self.cache_path) {
            debug!(
                "loading playlist cache from {}",
                self.cache_path.to_str().unwrap()
            );
            let parsed: Result<Vec<Playlist>, _> = serde_json::from_str(&contents);
            match parsed {
                Ok(cache) => {
                    debug!("playlist cache loaded ({} lists)", cache.len());
                    let mut store = self.store.write().expect("can't writelock playlist store");
                    store.clear();
                    store.extend(cache);

                    // force refresh of UI (if visible)
                    self.ev.trigger();
                }
                Err(e) => {
                    error!("can't parse playlist cache: {}", e);
                }
            }
        }
    }

    pub fn save_cache(&self) {
        match serde_json::to_string(&self.store.deref()) {
            Ok(contents) => std::fs::write(&self.cache_path, contents).unwrap(),
            Err(e) => error!("could not write playlist cache: {:?}", e),
        }
    }

    pub fn process_simplified_playlist(list: &SimplifiedPlaylist, spotify: &Spotify) -> Playlist {
        Playlists::_process_playlist(
            list.id.clone(),
            list.name.clone(),
            list.snapshot_id.clone(),
            spotify,
        )
    }

    pub fn process_full_playlist(list: &FullPlaylist, spotify: &Spotify) -> Playlist {
        Playlists::_process_playlist(
            list.id.clone(),
            list.name.clone(),
            list.snapshot_id.clone(),
            spotify,
        )
    }

    pub fn _process_playlist(
        id: String,
        name: String,
        snapshot_id: String,
        spotify: &Spotify,
    ) -> Playlist {
        let mut collected_tracks = Vec::new();

        let mut tracks_result = spotify.user_playlist_tracks(&id, 100, 0);
        while let Some(ref tracks) = tracks_result.clone() {
            for listtrack in &tracks.items {
                collected_tracks.push((&listtrack.track).into());
            }
            debug!("got {} tracks", tracks.items.len());

            // load next batch if necessary
            tracks_result = match tracks.next {
                Some(_) => {
                    debug!("requesting tracks again..");
                    spotify.user_playlist_tracks(
                        &id,
                        100,
                        tracks.offset + tracks.items.len() as u32,
                    )
                }
                None => None,
            }
        }
        Playlist {
            id: id.clone(),
            name: name.clone(),
            snapshot_id: snapshot_id.clone(),
            tracks: collected_tracks,
        }
    }

    fn needs_download(&self, remote: &SimplifiedPlaylist) -> bool {
        for local in self.store.read().expect("can't readlock playlists").iter() {
            if local.id == remote.id {
                return local.snapshot_id != remote.snapshot_id;
            }
        }
        true
    }

    fn append_or_update(&self, updated: &Playlist) -> usize {
        let mut store = self.store.write().expect("can't writelock playlists");
        for (index, mut local) in store.iter_mut().enumerate() {
            if local.id == updated.id {
                *local = updated.clone();
                return index;
            }
        }
        store.push(updated.clone());
        store.len() - 1
    }

    pub fn delete_playlist(&self, id: &str) {
        let mut store = self.store.write().expect("can't writelock playlists");
        if let Some(position) = store.iter().position(|ref i| i.id == id) {
            if self.spotify.delete_playlist(id) {
                store.remove(position);
                self.save_cache();
            }
        }
    }

    pub fn overwrite_playlist(&self, id: &str, tracks: &[Track]) {
        debug!("saving {} tracks to {}", tracks.len(), id);
        self.spotify.overwrite_playlist(id, &tracks);

        self.fetch_playlists();
        self.save_cache();
    }

    pub fn save_playlist(&self, name: &str, tracks: &[Track]) {
        debug!("saving {} tracks to new list {}", tracks.len(), name);
        match self.spotify.create_playlist(name, None, None) {
            Some(id) => self.overwrite_playlist(&id, &tracks),
            None => error!("could not create new playlist.."),
        }
    }

    pub fn fetch_playlists(&self) {
        debug!("loading playlists");
        let mut stale_lists = self.store.read().unwrap().clone();

        let mut lists_result = self.spotify.current_user_playlist(50, 0);
        while let Some(ref lists) = lists_result.clone() {
            for remote in &lists.items {
                // remove from stale playlists so we won't prune it later on
                if let Some(index) = stale_lists.iter().position(|x| x.id == remote.id) {
                    stale_lists.remove(index);
                }

                if self.needs_download(remote) {
                    info!("updating playlist {}", remote.name);
                    let playlist = Self::process_simplified_playlist(remote, &self.spotify);
                    self.append_or_update(&playlist);
                    // trigger redraw
                    self.ev.trigger();
                }
            }

            // load next batch if necessary
            lists_result = match lists.next {
                Some(_) => {
                    debug!("requesting playlists again..");
                    self.spotify
                        .current_user_playlist(50, lists.offset + lists.items.len() as u32)
                }
                None => None,
            }
        }

        // remove stale playlists
        for stale in stale_lists {
            let index = self
                .store
                .read()
                .unwrap()
                .iter()
                .position(|x| x.id == stale.id);
            if let Some(index) = index {
                debug!("removing stale list: {:?}", stale.name);
                self.store.write().unwrap().remove(index);
            }
        }
        // trigger redraw
        self.ev.trigger();
    }
}
