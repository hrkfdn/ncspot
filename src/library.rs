use std::collections::HashMap;
use std::iter::Iterator;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, RwLockReadGuard};
use std::thread;

use rspotify::spotify::model::artist::SimplifiedArtist;
use rspotify::spotify::model::playlist::{FullPlaylist, SimplifiedPlaylist};
use serde::de::DeserializeOwned;
use serde::Serialize;

use album::Album;
use artist::Artist;
use config;
use events::EventManager;
use playlist::Playlist;
use spotify::Spotify;
use track::Track;

const CACHE_TRACKS: &str = "tracks.db";
const CACHE_ALBUMS: &str = "albums.db";
const CACHE_ARTISTS: &str = "artists.db";
const CACHE_PLAYLISTS: &str = "playlists.db";

#[derive(Clone)]
pub struct Library {
    pub tracks: Arc<RwLock<Vec<Track>>>,
    pub albums: Arc<RwLock<Vec<Album>>>,
    pub artists: Arc<RwLock<Vec<Artist>>>,
    pub playlists: Arc<RwLock<Vec<Playlist>>>,
    ev: EventManager,
    spotify: Arc<Spotify>,
}

impl Library {
    pub fn new(ev: &EventManager, spotify: Arc<Spotify>) -> Self {
        let library = Self {
            tracks: Arc::new(RwLock::new(Vec::new())),
            albums: Arc::new(RwLock::new(Vec::new())),
            artists: Arc::new(RwLock::new(Vec::new())),
            playlists: Arc::new(RwLock::new(Vec::new())),
            ev: ev.clone(),
            spotify,
        };

        {
            // download playlists via web api in a background thread
            let library = library.clone();
            thread::spawn(move || {
                // load cache (if existing)
                library.load_caches();

                library.fetch_artists();
                library.fetch_tracks();
                library.fetch_albums();
                library.fetch_playlists();

                library.populate_artists();

                // re-cache for next startup
                library.save_caches();
            });
        }

        library
    }

    pub fn items(&self) -> RwLockReadGuard<Vec<Playlist>> {
        self.playlists
            .read()
            .expect("could not readlock listview content")
    }

    fn load_cache<T: DeserializeOwned>(&self, cache_path: PathBuf, store: Arc<RwLock<Vec<T>>>) {
        if let Ok(contents) = std::fs::read_to_string(&cache_path) {
            debug!("loading cache from {}", cache_path.display());
            let parsed: Result<Vec<T>, _> = serde_json::from_str(&contents);
            match parsed {
                Ok(cache) => {
                    debug!("cache from {} loaded ({} lists)", cache_path.display(), cache.len());
                    let mut store = store.write().expect("can't writelock store");
                    store.clear();
                    store.extend(cache);

                    // force refresh of UI (if visible)
                    self.ev.trigger();
                }
                Err(e) => {
                    error!("can't parse cache: {}", e);
                }
            }
        }
    }

    fn load_caches(&self) {
        self.load_cache(config::cache_path(CACHE_TRACKS), self.tracks.clone());
        self.load_cache(config::cache_path(CACHE_ALBUMS), self.albums.clone());
        self.load_cache(config::cache_path(CACHE_ARTISTS), self.artists.clone());
        self.load_cache(config::cache_path(CACHE_PLAYLISTS), self.playlists.clone());
    }

    fn save_cache<T: Serialize>(&self, cache_path: PathBuf, store: Arc<RwLock<Vec<T>>>) {
        match serde_json::to_string(&store.deref()) {
            Ok(contents) => std::fs::write(cache_path, contents).unwrap(),
            Err(e) => error!("could not write cache: {:?}", e),
        }
    }

    fn save_caches(&self) {
        self.save_cache(config::cache_path(CACHE_TRACKS), self.tracks.clone());
        self.save_cache(config::cache_path(CACHE_ALBUMS), self.albums.clone());
        self.save_cache(config::cache_path(CACHE_ARTISTS), self.artists.clone());
        self.save_cache(config::cache_path(CACHE_PLAYLISTS), self.playlists.clone());
    }

    pub fn process_simplified_playlist(list: &SimplifiedPlaylist, spotify: &Spotify) -> Playlist {
        Self::_process_playlist(
            list.id.clone(),
            list.name.clone(),
            list.snapshot_id.clone(),
            spotify,
        )
    }

    pub fn process_full_playlist(list: &FullPlaylist, spotify: &Spotify) -> Playlist {
        Self::_process_playlist(
            list.id.clone(),
            list.name.clone(),
            list.snapshot_id.clone(),
            spotify,
        )
    }

    fn _process_playlist(
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
        for local in self.playlists.read().expect("can't readlock playlists").iter() {
            if local.id == remote.id {
                return local.snapshot_id != remote.snapshot_id;
            }
        }
        true
    }

    fn append_or_update(&self, updated: &Playlist) -> usize {
        let mut store = self.playlists.write().expect("can't writelock playlists");
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
        let mut store = self.playlists.write().expect("can't writelock playlists");
        if let Some(position) = store.iter().position(|ref i| i.id == id) {
            if self.spotify.delete_playlist(id) {
                store.remove(position);
                self.save_cache(config::cache_path(CACHE_PLAYLISTS), self.playlists.clone());
            }
        }
    }

    pub fn overwrite_playlist(&self, id: &str, tracks: &[Track]) {
        debug!("saving {} tracks to {}", tracks.len(), id);
        self.spotify.overwrite_playlist(id, &tracks);

        self.update_playlists();
    }

    pub fn save_playlist(&self, name: &str, tracks: &[Track]) {
        debug!("saving {} tracks to new list {}", tracks.len(), name);
        match self.spotify.create_playlist(name, None, None) {
            Some(id) => self.overwrite_playlist(&id, &tracks),
            None => error!("could not create new playlist.."),
        }
    }

    pub fn update_playlists(&self) {
        self.fetch_playlists();
        self.save_cache(config::cache_path(CACHE_PLAYLISTS), self.playlists.clone());
    }

    fn fetch_playlists(&self) {
        debug!("loading playlists");
        let mut stale_lists = self.playlists.read().unwrap().clone();

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
                .playlists
                .read()
                .unwrap()
                .iter()
                .position(|x| x.id == stale.id);
            if let Some(index) = index {
                debug!("removing stale list: {:?}", stale.name);
                self.playlists.write().unwrap().remove(index);
            }
        }
        // trigger redraw
        self.ev.trigger();
    }

    fn fetch_artists(&self) {
        let mut artists: Vec<Artist> = Vec::new();
        let mut last: Option<String> = None;

        let mut i: u32 = 0;

        loop {
            let page = self.spotify.current_user_followed_artists(last);
            debug!("artists page: {}", i);
            i += 1;
            if page.is_none() {
                error!("Failed to fetch artists.");
                return;
            }
            let page = page.unwrap();

            artists.extend(page.items.iter().map(|fa| fa.into()));

            if page.next.is_some() {
                last = Some(artists.last().unwrap().id.clone());
            } else {
                break;
            }
        }

        for artist in artists.iter_mut() {
            // Only play saved tracks
            artist.albums = Some(Vec::new());
            artist.tracks = Some(Vec::new());
        }

        *(self.artists.write().unwrap()) = artists;
    }

    fn insert_artist(&self, artist: &SimplifiedArtist) {
        let mut artists = self.artists.write().unwrap();
        if artists.iter().any(|a| a.id == artist.id) {
            return;
        }

        artists.push(artist.into());
    }

    fn fetch_albums(&self) {
        let mut albums: Vec<Album> = Vec::new();

        let mut i: u32 = 0;

        loop {
            let page = self.spotify.current_user_saved_albums(albums.len() as u32);
            debug!("albums page: {}", i);
            i += 1;
            if page.is_none() {
                error!("Failed to fetch albums.");
                return;
            }
            let page = page.unwrap();

            albums.extend(page.items.iter().map(|a| a.into()));

            if page.next.is_none() {
                break;
            }
        }

        *(self.albums.write().unwrap()) = albums;
    }

    fn fetch_tracks(&self) {
        let mut tracks: Vec<Track> = Vec::new();

        let mut i: u32 = 0;

        loop {
            let page = self.spotify.current_user_saved_tracks(tracks.len() as u32);

            debug!("tracks page: {}", i);
            i += 1;

            if page.is_none() {
                error!("Failed to fetch tracks.");
                return;
            }
            let page = page.unwrap();

            if page.offset == 0 {
                // If first page matches the first items in store and total is
                // identical, assume list is unchanged.

                let store = self.tracks.read().unwrap();

                if page.total as usize == store.len() &&
                    !page.items
                        .iter()
                        .enumerate()
                        .any(|(i, t)| &t.track.id != &store[i].id)
                {
                    return;
                }
            }

            for track in page.items.iter() {
                for artist in track.track.artists.iter() {
                    self.insert_artist(artist);
                }
                tracks.push(track.into());
            }

            if page.next.is_none() {
                break;
            }
        }

        *(self.tracks.write().unwrap()) = tracks;
    }

    fn populate_artists(&self) {
        let mut artists = self.artists.write().unwrap();
        let mut lookup: HashMap<String, Option<usize>> = HashMap::new();

        artists.sort_unstable_by(|a, b| a.name.partial_cmp(&b.name).unwrap());

        {
            let albums = self.albums.read().unwrap();
            for album in albums.iter() {
                for artist_id in &album.artist_ids {
                    let index = if let Some(i) = lookup.get(artist_id).cloned() {
                        i
                    } else {
                        let i = artists.iter().position(|a| &a.id == artist_id);
                        lookup.insert(artist_id.clone(), i);
                        i
                    };

                    if let Some(i) = index {
                        let mut artist = artists.get_mut(i).unwrap();
                        if artist.albums.is_none() {
                            artist.albums = Some(Vec::new());
                        }

                        if let Some(albums) = artist.albums.as_mut() {
                            albums.push(album.clone());
                        }
                    }
                }
            }
        }

        {
            let tracks = self.tracks.read().unwrap();
            for track in tracks.iter() {
                for artist_id in &track.artist_ids {
                    let index = if let Some(i) = lookup.get(artist_id).cloned() {
                        i
                    } else {
                        let i = artists.iter().position(|a| &a.id == artist_id);
                        lookup.insert(artist_id.clone(), i);
                        i
                    };

                    if let Some(i) = index {
                        let mut artist = artists.get_mut(i).unwrap();
                        if artist.tracks.is_none() {
                            artist.tracks = Some(Vec::new());
                        }

                        if let Some(tracks) = artist.tracks.as_mut() {
                            tracks.push(track.clone());
                        }
                    }
                }
            }
        }
    }
}
