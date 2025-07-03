use std::collections::HashMap;
use std::fs::File;
use std::iter::Iterator;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::thread;

use log::{debug, error, info};
use rspotify::model::Id;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::Config;
use crate::config::{self, CACHE_VERSION};
use crate::events::EventManager;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::show::Show;
use crate::model::track::Track;
use crate::spotify::Spotify;

/// Cached tracks database filename.
const CACHE_TRACKS: &str = "tracks.db";

/// Cached albums database filename.
const CACHE_ALBUMS: &str = "albums.db";

/// Cached artists database filename.
const CACHE_ARTISTS: &str = "artists.db";

/// Cached playlists database filename.
const CACHE_PLAYLISTS: &str = "playlists.db";

/// The user library with all their saved tracks, albums, playlists... High level interface to the
/// Spotify API used to manage items in the user library.
#[derive(Clone)]
pub struct Library {
    pub tracks: Arc<RwLock<Vec<Track>>>,
    pub albums: Arc<RwLock<Vec<Album>>>,
    pub artists: Arc<RwLock<Vec<Artist>>>,
    pub playlists: Arc<RwLock<Vec<Playlist>>>,
    pub shows: Arc<RwLock<Vec<Show>>>,
    pub is_done: Arc<RwLock<bool>>,
    pub user_id: Option<String>,
    pub display_name: Option<String>,
    ev: EventManager,
    spotify: Spotify,
    pub cfg: Arc<Config>,
}

impl Library {
    pub fn new(ev: EventManager, spotify: Spotify, cfg: Arc<Config>) -> Self {
        let current_user = spotify.api.current_user().ok();
        let user_id = current_user.as_ref().map(|u| u.id.id().to_string());
        let display_name = current_user.as_ref().and_then(|u| u.display_name.clone());

        let library = Self {
            tracks: Arc::new(RwLock::new(Vec::new())),
            albums: Arc::new(RwLock::new(Vec::new())),
            artists: Arc::new(RwLock::new(Vec::new())),
            playlists: Arc::new(RwLock::new(Vec::new())),
            shows: Arc::new(RwLock::new(Vec::new())),
            is_done: Arc::new(RwLock::new(false)),
            user_id,
            display_name,
            ev,
            spotify,
            cfg,
        };

        library.update_library();
        library
    }

    /// Load cached items from the file at `cache_path` into the given `store`.
    fn load_cache<T: DeserializeOwned>(&self, cache_path: &Path, store: &mut Vec<T>) {
        let saved_cache_version = self.cfg.state().cache_version;
        if saved_cache_version < CACHE_VERSION {
            debug!(
                "Cache version for {cache_path:?} has changed from {saved_cache_version} to {CACHE_VERSION}, ignoring cache"
            );
            return;
        }

        if let Ok(contents) = std::fs::read_to_string(cache_path) {
            debug!("loading cache from {}", cache_path.display());
            // Parse from in-memory string instead of directly from the file because it's faster.
            let parsed = serde_json::from_str::<Vec<_>>(&contents);
            match parsed {
                Ok(cache) => {
                    debug!(
                        "cache from {} loaded ({} items)",
                        cache_path.display(),
                        cache.len()
                    );
                    store.clear();
                    store.extend(cache);

                    // force refresh of UI (if visible)
                    self.trigger_redraw();
                }
                Err(e) => {
                    error!("can't parse cache: {e}");
                }
            }
        }
    }

    /// Save the items from `store` in the file at `cache_path`.
    fn save_cache<T: Serialize>(&self, cache_path: &Path, store: &[T]) {
        let cache_file = File::create(cache_path).unwrap();
        let serialize_result = serde_json::to_writer(cache_file, store);
        if let Err(message) = serialize_result {
            error!("could not write cache: {message:?}");
        }
    }

    /// Check whether the `remote` [Playlist] is newer than its locally saved version. Returns
    /// `true` if it is or if a local version isn't found.
    fn needs_download(&self, remote: &Playlist) -> bool {
        self.playlists
            .read()
            .unwrap()
            .iter()
            .find(|local| local.id == remote.id)
            .map(|local| local.snapshot_id != remote.snapshot_id)
            .unwrap_or(true)
    }

    /// Append `updated` to the local playlists or update the local version if it exists. Return the
    /// index of the appended/updated playlist.
    fn append_or_update(&self, updated: Playlist) -> usize {
        let mut store = self.playlists.write().unwrap();
        for (index, local) in store.iter_mut().enumerate() {
            if local.id == updated.id {
                *local = updated;
                return index;
            }
        }
        store.push(updated);
        store.len() - 1
    }

    /// Delete the playlist with the given `id` if it exists.
    pub fn delete_playlist(&self, id: &str) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        let position = self
            .playlists
            .read()
            .unwrap()
            .iter()
            .position(|i| i.id == id);

        if let Some(position) = position {
            if self.spotify.api.delete_playlist(id).is_ok() {
                self.playlists.write().unwrap().remove(position);
                self.save_cache(
                    &config::cache_path(CACHE_PLAYLISTS),
                    &self.playlists.read().unwrap(),
                );
            }
        }
    }

    /// Set the playlist with `id` to contain only `tracks`. If the playlist already contains
    /// tracks, they will be removed. Update the cache to match the new state.
    pub fn overwrite_playlist(&self, id: &str, tracks: &[Playable]) {
        debug!("saving {} tracks to list {}", tracks.len(), id);
        self.spotify.api.overwrite_playlist(id, tracks);

        self.fetch_playlists();
        self.save_cache(
            &config::cache_path(CACHE_PLAYLISTS),
            &self.playlists.read().unwrap(),
        );
    }

    /// Create a playlist with the given `name` and add `tracks` to it.
    pub fn save_playlist(&self, name: &str, tracks: &[Playable]) {
        debug!("saving {} tracks to new list {}", tracks.len(), name);
        match self.spotify.api.create_playlist(name, None, None) {
            Ok(id) => self.overwrite_playlist(&id, tracks),
            Err(_) => error!("could not create new playlist.."),
        }
    }

    /// Update the local library and its cache on disk.
    pub fn update_library(&self) {
        *self.is_done.write().unwrap() = false;

        let library = self.clone();
        thread::spawn(move || {
            let t_tracks = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_TRACKS),
                        library.tracks.write().unwrap().as_mut(),
                    );
                    library.fetch_tracks();
                    library.save_cache(
                        &config::cache_path(CACHE_TRACKS),
                        &library.tracks.read().unwrap(),
                    );
                })
            };

            let t_albums = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_ALBUMS),
                        library.albums.write().unwrap().as_mut(),
                    );
                    library.fetch_albums();
                    library.save_cache(
                        &config::cache_path(CACHE_ALBUMS),
                        &library.albums.read().unwrap(),
                    );
                })
            };

            let t_artists = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_ARTISTS),
                        library.artists.write().unwrap().as_mut(),
                    );
                    library.fetch_artists();
                })
            };

            let t_playlists = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_PLAYLISTS),
                        library.playlists.write().unwrap().as_mut(),
                    );
                    library.fetch_playlists();
                    library.save_cache(
                        &config::cache_path(CACHE_PLAYLISTS),
                        &library.playlists.read().unwrap(),
                    );
                })
            };

            let t_shows = {
                let library = library.clone();
                thread::spawn(move || {
                    library.fetch_shows();
                })
            };

            t_tracks.join().unwrap();
            t_artists.join().unwrap();

            library.populate_artists();
            library.save_cache(
                &config::cache_path(CACHE_ARTISTS),
                &library.artists.read().unwrap(),
            );

            t_albums.join().unwrap();
            t_playlists.join().unwrap();
            t_shows.join().unwrap();

            let mut is_done = library.is_done.write().unwrap();
            *is_done = true;

            library.ev.trigger();
        });
    }

    /// Fetch the shows from the web API and save them to the local library.
    fn fetch_shows(&self) {
        debug!("loading shows");

        let mut saved_shows: Vec<Show> = Vec::new();
        let mut shows_result = self.spotify.api.get_saved_shows(0).ok();

        while let Some(shows) = shows_result {
            saved_shows.extend(shows.items.iter().map(|show| (&show.show).into()));

            // load next batch if necessary
            shows_result = match shows.next {
                Some(_) => {
                    debug!("requesting shows again..");
                    self.spotify
                        .api
                        .get_saved_shows(shows.offset + shows.items.len() as u32)
                        .ok()
                }
                None => None,
            }
        }

        *self.shows.write().unwrap() = saved_shows;
    }

    /// Fetch the playlists from the web API and save them to the local library. This synchronizes
    /// the local version with the remote, pruning removed playlists in the process.
    fn fetch_playlists(&self) {
        debug!("loading playlists");
        let mut stale_lists = self.playlists.read().unwrap().clone();
        let mut list_order = Vec::new();

        let lists_page = self.spotify.api.current_user_playlist();
        let mut lists_batch = Some(lists_page.items.read().unwrap().clone());
        while let Some(lists) = lists_batch {
            for (index, remote) in lists.iter().enumerate() {
                list_order.push(remote.id.clone());

                // remove from stale playlists so we won't prune it later on
                if let Some(index) = stale_lists.iter().position(|x| x.id == remote.id) {
                    stale_lists.remove(index);
                }

                if self.needs_download(remote) {
                    info!("updating playlist {} (index: {})", remote.name, index);
                    let mut playlist: Playlist = remote.clone();
                    playlist.tracks = None;
                    playlist.load_tracks(&self.spotify);
                    self.append_or_update(playlist);
                    // trigger redraw
                    self.trigger_redraw();
                }
            }
            lists_batch = lists_page.next();
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

        // sort by remote order
        self.playlists.write().unwrap().sort_by(|a, b| {
            let a_index = list_order.iter().position(|x| x == &a.id);
            let b_index = list_order.iter().position(|x| x == &b.id);
            a_index.cmp(&b_index)
        });

        // trigger redraw
        self.trigger_redraw();
    }

    /// Fetch the artists from the web API and save them to the local library.
    fn fetch_artists(&self) {
        let mut artists: Vec<Artist> = Vec::new();
        let mut last: Option<&str> = None;
        let mut i = 0u32;

        loop {
            let page = self.spotify.api.current_user_followed_artists(last);
            debug!("artists page: {i}");
            i += 1;
            if page.is_err() {
                error!("Failed to fetch artists.");
                return;
            }
            let page = page.unwrap();

            artists.extend(page.items.iter().map(|fa| fa.into()));

            if page.next.is_some() {
                last = artists.last().unwrap().id.as_deref();
            } else {
                break;
            }
        }

        let mut store = self.artists.write().unwrap();

        for mut artist in artists {
            let pos = store.iter().position(|a| a.id == artist.id);
            if let Some(i) = pos {
                store[i].is_followed = true;
                continue;
            }

            artist.is_followed = true;

            store.push(artist);
        }
    }

    /// Add the artist with `id` and `name` to the user library, but don't sync with the API.
    /// This does not add if there is already an artist with `id`.
    fn insert_artist(&self, id: String, name: String) {
        let mut artists = self.artists.write().unwrap();

        if !artists
            .iter()
            .any(|a| a.id.as_ref().is_some_and(|value| *value == id))
        {
            let mut artist = Artist::new(id.to_string(), name.to_string());
            artist.tracks = Some(Vec::new());
            artists.push(artist);
        }
    }

    /// Fetch the albums from the web API and store them in the local library.
    fn fetch_albums(&self) {
        let mut albums: Vec<Album> = Vec::new();
        let mut i = 0u32;

        loop {
            let page = self
                .spotify
                .api
                .current_user_saved_albums(albums.len() as u32);
            debug!("albums page: {i}");

            i += 1;

            if page.is_err() {
                error!("Failed to fetch albums.");
                return;
            }

            let page = page.unwrap();
            albums.extend(page.items.iter().map(|a| a.into()));

            if page.next.is_none() {
                break;
            }
        }

        albums.sort_unstable_by_key(|album| {
            let album_artist = album.artists[0]
                .strip_prefix("The ")
                .unwrap_or(&album.artists[0]);
            let album_title = album.title.strip_prefix("The ").unwrap_or(&album.title);
            format!(
                "{}{}{}",
                album_artist.to_lowercase(),
                album.year,
                album_title.to_lowercase()
            )
        });

        *self.albums.write().unwrap() = albums;
    }

    /// Fetch the tracks from the web API and save them in the local library.
    fn fetch_tracks(&self) {
        let mut tracks = Vec::new();
        let mut i = 0u32;

        loop {
            let page = self
                .spotify
                .api
                .current_user_saved_tracks(tracks.len() as u32);

            debug!("tracks page: {i}");
            i += 1;

            if page.is_err() {
                error!("Failed to fetch tracks.");
                return;
            }
            let page = page.unwrap();

            if page.offset == 0 {
                // If first page matches the first items in store and total is
                // identical, assume list is unchanged.

                let store = self.tracks.read().unwrap();

                if page.total as usize == store.len()
                    && !page
                        .items
                        .iter()
                        .enumerate()
                        .any(|(i, t)| t.track.id.as_ref().map(|id| id.to_string()) != store[i].id)
                {
                    return;
                }
            }

            tracks.extend(page.items.iter().map(|t| t.into()));

            if page.next.is_none() {
                break;
            }
        }

        *self.tracks.write().unwrap() = tracks;
    }

    fn populate_artists(&self) {
        // Remove old unfollowed artists
        {
            let mut artists = self.artists.write().unwrap();
            *artists = artists.iter().filter(|a| a.is_followed).cloned().collect();
        }

        // Add artists that aren't followed but have saved tracks
        {
            let tracks = self.tracks.read().unwrap();
            let mut track_artists: Vec<(&String, &String)> = tracks
                .iter()
                .flat_map(|t| t.artist_ids.iter().zip(t.artists.iter()))
                .collect();
            track_artists.dedup_by(|a, b| a.0 == b.0);

            for (id, name) in track_artists.iter() {
                self.insert_artist(id.to_string(), name.to_string());
            }
        }

        let mut artists = self.artists.write().unwrap();
        let mut lookup: HashMap<String, Option<usize>> = HashMap::new();

        // Make sure only saved tracks are played when playing artists
        for artist in artists.iter_mut() {
            artist.tracks = Some(Vec::new());
        }

        artists.sort_unstable_by(|a, b| {
            let a_cmp = a.name.strip_prefix("The ").unwrap_or(&a.name);
            let b_cmp = b.name.strip_prefix("The ").unwrap_or(&b.name);

            a_cmp.partial_cmp(b_cmp).unwrap()
        });

        // Add saved tracks to artists
        {
            let tracks = self.tracks.read().unwrap();
            for track in tracks.iter() {
                for artist_id in &track.artist_ids {
                    let index = if let Some(i) = lookup.get(artist_id).cloned() {
                        i
                    } else {
                        let i = artists
                            .iter()
                            .position(|a| &a.id.clone().unwrap_or_default() == artist_id);
                        lookup.insert(artist_id.clone(), i);
                        i
                    };

                    if let Some(i) = index {
                        let artist = artists.get_mut(i).unwrap();
                        if artist.tracks.is_none() {
                            artist.tracks = Some(Vec::new());
                        }

                        if let Some(tracks) = artist.tracks.as_mut() {
                            if tracks.iter().any(|t| t.id == track.id) {
                                continue;
                            }

                            tracks.push(track.clone());
                        }
                    }
                }
            }
        }
    }

    /// If there is a local version of the playlist, update it and rewrite the cache.
    pub fn playlist_update(&self, updated: &Playlist) {
        {
            let mut playlists = self.playlists.write().unwrap();
            if let Some(playlist) = playlists.iter_mut().find(|p| p.id == updated.id) {
                *playlist = updated.clone();
            }
        }

        self.save_cache(
            &config::cache_path(CACHE_PLAYLISTS),
            &self.playlists.read().unwrap(),
        );
    }

    /// Check whether `track` is saved in the user's library.
    pub fn is_saved_track(&self, track: &Playable) -> bool {
        if !*self.is_done.read().unwrap() {
            return false;
        }

        let tracks = self.tracks.read().unwrap();
        tracks.iter().any(|t| t.id == track.id())
    }

    /// Save `tracks` to the user's library.
    pub fn save_tracks(&self, tracks: &[&Track]) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        let save_tracks_result = self
            .spotify
            .api
            .current_user_saved_tracks_add(tracks.iter().filter_map(|t| t.id.as_deref()).collect());

        if save_tracks_result.is_err() {
            return;
        }

        {
            let mut store = self.tracks.write().unwrap();
            let mut i = 0;
            for track in tracks {
                if store.iter().any(|t| t.id == track.id) {
                    continue;
                }

                store.insert(i, (*track).clone());
                i += 1;
            }
        }

        self.populate_artists();

        self.save_cache(
            &config::cache_path(CACHE_TRACKS),
            &self.tracks.read().unwrap(),
        );
        self.save_cache(
            &config::cache_path(CACHE_ARTISTS),
            &self.artists.read().unwrap(),
        );
    }

    /// Remove `tracks` from the user's library.
    pub fn unsave_tracks(&self, tracks: &[&Track]) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if self
            .spotify
            .api
            .current_user_saved_tracks_delete(
                tracks.iter().filter_map(|t| t.id.as_deref()).collect(),
            )
            .is_err()
        {
            return;
        }

        {
            let mut store = self.tracks.write().unwrap();
            *store = store
                .iter()
                .filter(|t| !tracks.iter().any(|tt| t.id == tt.id))
                .cloned()
                .collect();
        }

        self.populate_artists();

        self.save_cache(
            &config::cache_path(CACHE_TRACKS),
            &self.tracks.read().unwrap(),
        );
        self.save_cache(
            &config::cache_path(CACHE_ARTISTS),
            &self.artists.read().unwrap(),
        );
    }

    /// Check whether `album` is saved to the user's library.
    pub fn is_saved_album(&self, album: &Album) -> bool {
        if !*self.is_done.read().unwrap() {
            return false;
        }

        let albums = self.albums.read().unwrap();
        albums.iter().any(|a| a.id == album.id)
    }

    /// Save `album` to the user's library.
    pub fn save_album(&self, album: &Album) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if let Some(ref album_id) = album.id {
            if self
                .spotify
                .api
                .current_user_saved_albums_add(vec![album_id.as_str()])
                .is_err()
            {
                return;
            }
        }

        {
            let mut store = self.albums.write().unwrap();
            if !store.iter().any(|a| a.id == album.id) {
                store.insert(0, album.clone());

                // resort list of albums
                store.sort_unstable_by_key(|a| format!("{}{}{}", a.artists[0], a.year, a.title));
            }
        }

        self.save_cache(
            &config::cache_path(CACHE_ALBUMS),
            &self.albums.read().unwrap(),
        );
    }

    /// Remove `album` from the user's library.
    pub fn unsave_album(&self, album: &Album) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if let Some(ref album_id) = album.id {
            if self
                .spotify
                .api
                .current_user_saved_albums_delete(vec![album_id.as_str()])
                .is_err()
            {
                return;
            }
        }

        {
            let mut store = self.albums.write().unwrap();
            *store = store.iter().filter(|a| a.id != album.id).cloned().collect();
        }

        self.save_cache(
            &config::cache_path(CACHE_ALBUMS),
            &self.albums.read().unwrap(),
        );
    }

    /// Check whether the user follows `artist`.
    pub fn is_followed_artist(&self, artist: &Artist) -> bool {
        if !*self.is_done.read().unwrap() {
            return false;
        }

        let artists = self.artists.read().unwrap();
        artists.iter().any(|a| a.id == artist.id && a.is_followed)
    }

    /// Follow `artist` as the logged in user.
    pub fn follow_artist(&self, artist: &Artist) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if let Some(ref artist_id) = artist.id {
            if self
                .spotify
                .api
                .user_follow_artists(vec![artist_id.as_str()])
                .is_err()
            {
                return;
            }
        }

        {
            let mut store = self.artists.write().unwrap();
            if let Some(i) = store.iter().position(|a| a.id == artist.id) {
                store[i].is_followed = true;
            } else {
                let mut artist = artist.clone();
                artist.is_followed = true;
                store.push(artist);
            }
        }

        self.populate_artists();

        self.save_cache(
            &config::cache_path(CACHE_ARTISTS),
            &self.artists.read().unwrap(),
        );
    }

    /// Unfollow `artist` as the logged in user.
    pub fn unfollow_artist(&self, artist: &Artist) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if let Some(ref artist_id) = artist.id {
            if self
                .spotify
                .api
                .user_unfollow_artists(vec![artist_id.as_str()])
                .is_err()
            {
                return;
            }
        }

        {
            let mut store = self.artists.write().unwrap();
            if let Some(i) = store.iter().position(|a| a.id == artist.id) {
                store[i].is_followed = false;
            }
        }

        self.populate_artists();

        self.save_cache(
            &config::cache_path(CACHE_ARTISTS),
            &self.artists.read().unwrap(),
        );
    }

    /// Check whether `playlist` is saved in the user's library.
    pub fn is_saved_playlist(&self, playlist: &Playlist) -> bool {
        if !*self.is_done.read().unwrap() {
            return false;
        }

        let playlists = self.playlists.read().unwrap();
        playlists.iter().any(|p| p.id == playlist.id)
    }

    /// Check whether `playlist` is in the library but not created by the library's owner.
    pub fn is_followed_playlist(&self, playlist: &Playlist) -> bool {
        self.user_id
            .as_ref()
            .map(|id| id != &playlist.owner_id)
            .unwrap_or(false)
    }

    /// Add `playlist` to the user's library by following it as the logged in user.
    pub fn follow_playlist(&self, mut playlist: Playlist) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        let follow_playlist_result = self.spotify.api.user_playlist_follow_playlist(&playlist.id);

        if follow_playlist_result.is_err() {
            return;
        }

        playlist.load_tracks(&self.spotify);

        {
            let mut store = self.playlists.write().unwrap();
            if !store.iter().any(|p| p.id == playlist.id) {
                store.insert(0, playlist);
            }
        }

        self.save_cache(
            &config::cache_path(CACHE_PLAYLISTS),
            &self.playlists.read().unwrap(),
        );
    }

    /// Check whether `show` is already in the user's library.
    pub fn is_saved_show(&self, show: &Show) -> bool {
        if !*self.is_done.read().unwrap() {
            return false;
        }

        let shows = self.shows.read().unwrap();
        shows.iter().any(|s| s.id == show.id)
    }

    /// Save the `show` to the user's library.
    pub fn save_show(&self, show: &Show) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if self.spotify.api.save_shows(&[show.id.as_str()]).is_ok() {
            {
                let mut store = self.shows.write().unwrap();
                if !store.iter().any(|s| s.id == show.id) {
                    store.insert(0, show.clone());
                }
            }
        }
    }

    /// Remove the `show` from the user's library.
    pub fn unsave_show(&self, show: &Show) {
        if !*self.is_done.read().unwrap() {
            return;
        }

        if self.spotify.api.unsave_shows(&[show.id.as_str()]).is_ok() {
            let mut store = self.shows.write().unwrap();
            *store = store.iter().filter(|s| s.id != show.id).cloned().collect();
        }
    }

    /// Force redraw the user interface.
    pub fn trigger_redraw(&self) {
        self.ev.trigger();
    }
}
