use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::iter::Iterator;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::thread;

use log::{debug, error, info};
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
    pub display_name: Arc<RwLock<Option<String>>>,
    ev: EventManager,
    spotify: Spotify,
    pub cfg: Arc<Config>,
}

impl Library {
    /// Create an empty library for use in tests. No cache is loaded and no API calls are made.
    #[cfg(test)]
    pub fn new_for_test(ev: EventManager, spotify: Spotify, cfg: Arc<Config>) -> Arc<Self> {
        Arc::new(Self {
            tracks: Arc::new(RwLock::new(Vec::new())),
            albums: Arc::new(RwLock::new(Vec::new())),
            artists: Arc::new(RwLock::new(Vec::new())),
            playlists: Arc::new(RwLock::new(Vec::new())),
            shows: Arc::new(RwLock::new(Vec::new())),
            is_done: Arc::new(RwLock::new(false)),
            user_id: None,
            display_name: Arc::new(RwLock::new(None)),
            ev,
            spotify,
            cfg,
        })
    }

    pub fn new(ev: EventManager, spotify: Spotify, cfg: Arc<Config>) -> Self {
        let user_id = spotify.api.user();
        let display_name = Arc::new(RwLock::new(None));
        // fetched in the background, a rate limit would otherwise block startup
        if !cfg.values().hide_display_names.unwrap_or(false) {
            if let Some((id, name)) = &cfg.state().display_name
                && user_id.as_ref() == Some(id)
            {
                *display_name.write().unwrap() = Some(name.clone());
            }
            let display_name = display_name.clone();
            let spotify = spotify.clone();
            let cfg = cfg.clone();
            let user_id = user_id.clone();
            let ev = ev.clone();
            thread::spawn(move || {
                if let Ok(user) = spotify.api.current_user() {
                    let cached = user_id.zip(user.display_name.clone());
                    cfg.with_state_mut(|state| state.display_name = cached.clone());
                    *display_name.write().unwrap() = user.display_name;
                    ev.trigger();
                }
            });
        } else {
            cfg.with_state_mut(|state| state.display_name = None);
        }

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

        library.update_library(false);
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

    /// Whether `remote` is missing locally or the local copy [is_stale].
    fn needs_download(&self, remote: &Playlist) -> bool {
        self.playlists
            .read()
            .unwrap()
            .iter()
            .find(|local| local.id == remote.id)
            .is_none_or(|local| is_stale(local, remote))
    }

    /// Replace the local playlist that has the same ID with `updated`, or add `updated` at the end
    /// (at the front if `add_at_front`). If `updated` has no tracks because they failed to load,
    /// keep the cached tracks and old snapshot, so it reloads next time.
    fn append_or_update(&self, mut updated: Playlist, add_at_front: bool) {
        let mut store = self.playlists.write().unwrap();
        match store.iter_mut().find(|local| local.id == updated.id) {
            Some(local) => {
                if updated.tracks.is_none() && local.tracks.is_some() {
                    updated.tracks = local.tracks.take();
                    updated.snapshot_id = std::mem::take(&mut local.snapshot_id);
                }
                *local = updated;
            }
            None if add_at_front => store.insert(0, updated),
            None => store.push(updated),
        }
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

        if let Some(position) = position
            && self.spotify.api.delete_playlist(id).is_ok()
        {
            self.playlists.write().unwrap().remove(position);
            self.save_cache(
                &config::cache_path(CACHE_PLAYLISTS),
                &self.playlists.read().unwrap(),
            );
        }
    }

    /// Set the playlist with `id` to contain only `tracks`. If the playlist already contains
    /// tracks, they will be removed. Update the cache to match the new state.
    pub fn overwrite_playlist(&self, id: &str, tracks: &[Playable]) {
        debug!("saving {} tracks to list {}", tracks.len(), id);
        self.spotify.api.overwrite_playlist(id, tracks);

        // only reload this playlist, as a full sync runs on the UI thread here
        if let Ok(remote) = self.spotify.api.playlist(id) {
            let mut playlist = Playlist::from(&remote);
            playlist.load_tracks(&self.spotify);
            // a new playlist goes first, as on Spotify
            self.append_or_update(playlist, true);
            self.trigger_redraw();
        }
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
    ///
    /// # Arguments
    ///
    /// * `refetch_unchanged` - Download all liked songs and saved albums, even if they haven't
    ///   changed.
    pub fn update_library(&self, refetch_unchanged: bool) {
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
                    if library.fetch_tracks(refetch_unchanged) {
                        library.save_cache(
                            &config::cache_path(CACHE_TRACKS),
                            &library.tracks.read().unwrap(),
                        );
                    }
                })
            };

            let t_albums = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_ALBUMS),
                        library.albums.write().unwrap().as_mut(),
                    );
                    if library.fetch_albums(refetch_unchanged) {
                        library.save_cache(
                            &config::cache_path(CACHE_ALBUMS),
                            &library.albums.read().unwrap(),
                        );
                    }
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
        let mut offset = 0;

        loop {
            let Ok(shows) = self.spotify.api.get_saved_shows(offset) else {
                error!("Failed to load all saved shows");
                // shows aren't cached on disk, so a partial list beats none at startup
                let mut store = self.shows.write().unwrap();
                if store.is_empty() {
                    *store = saved_shows;
                }
                return;
            };
            saved_shows.extend(shows.items.iter().map(|show| (&show.show).into()));

            // load next batch if necessary
            if shows.next.is_none() {
                break;
            }
            debug!("requesting shows again..");
            offset = shows.offset + shows.items.len() as u32;
        }

        *self.shows.write().unwrap() = saved_shows;
    }

    /// Fetch the playlists from the web API and save them to the local library. This synchronizes
    /// the local version with the remote, pruning removed playlists in the process.
    fn fetch_playlists(&self) {
        debug!("loading playlists");
        let cached: HashSet<String> = self
            .playlists
            .read()
            .unwrap()
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let mut list_order: HashMap<String, usize> = HashMap::new();
        let rate_limit_failures = self.spotify.api.rate_limit_failures();

        let lists_page = self.spotify.api.current_user_playlist();
        let mut lists_batch = Some(lists_page.items.read().unwrap().clone());
        while let Some(lists) = lists_batch {
            for (index, remote) in lists.iter().enumerate() {
                list_order.insert(remote.id.clone(), list_order.len());
                if !self.needs_download(remote) {
                    continue;
                }

                let mut playlist: Playlist = remote.clone();
                playlist.tracks = None;
                // once rate limited, load the rest on the next update
                if self.spotify.api.rate_limit_failures() != rate_limit_failures {
                    if !cached.contains(&playlist.id) {
                        self.append_or_update(playlist, false);
                    }
                    continue;
                }

                info!("updating playlist {} (index: {})", remote.name, index);
                playlist.load_tracks(&self.spotify);
                self.append_or_update(playlist, false);
                // trigger redraw
                self.trigger_redraw();
            }
            lists_batch = lists_page.next();
        }

        // only a complete listing shows which playlists were removed
        if lists_page.is_complete() {
            self.playlists.write().unwrap().retain(|p| {
                let removed = cached.contains(&p.id) && !list_order.contains_key(&p.id);
                if removed {
                    debug!("removing stale list: {:?}", p.name);
                }
                !removed
            });
        } else {
            error!("Failed to load all playlists, keeping cached playlists");
        }

        // sort by remote order, unlisted ones last
        self.playlists
            .write()
            .unwrap()
            .sort_by_key(|p| list_order.get(&p.id).copied().unwrap_or(usize::MAX));

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
    ///
    /// # Returns
    ///
    /// `false` if the cache doesn't need saving.
    fn fetch_albums(&self, refetch_unchanged: bool) -> bool {
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
                // Save anyway, so a cache from an older version is replaced.
                return true;
            }

            let page = page.unwrap();
            let items: Vec<Album> = page.items.iter().map(|a| a.into()).collect();

            if !refetch_unchanged
                && page.offset == 0
                && albums_unchanged(page.total, &items, &self.albums.read().unwrap())
            {
                // Keep the cache and skip the other pages. A cache written by an older version may
                // use the old sort.
                return sort_albums(&mut self.albums.write().unwrap());
            }

            albums.extend(items);

            if page.next.is_none() {
                break;
            }
        }

        sort_albums(&mut albums);

        *self.albums.write().unwrap() = albums;
        true
    }

    /// Fetch the tracks from the web API and save them in the local library.
    ///
    /// # Returns
    ///
    /// `false` if the cache doesn't need saving.
    fn fetch_tracks(&self, refetch_unchanged: bool) -> bool {
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
                // Save anyway, so a cache from an older version is replaced.
                return true;
            }
            let page = page.unwrap();
            let items: Vec<Track> = page.items.iter().map(|t| t.into()).collect();

            if !refetch_unchanged
                && page.offset == 0
                && tracks_unchanged(page.total, &items, &self.tracks.read().unwrap())
            {
                // Keep the cache and skip the other pages.
                return false;
            }

            tracks.extend(items);

            if page.next.is_none() {
                break;
            }
        }

        *self.tracks.write().unwrap() = tracks;
        true
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
            trim_leading_the(&a.name)
                .partial_cmp(trim_leading_the(&b.name))
                .unwrap()
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

        if let Some(ref album_id) = album.id
            && self
                .spotify
                .api
                .current_user_saved_albums_add(vec![album_id.as_str()])
                .is_err()
        {
            return;
        }

        {
            let mut store = self.albums.write().unwrap();
            if !store.iter().any(|a| a.id == album.id) {
                store.insert(0, album.clone());
                sort_albums(&mut store);
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

        if let Some(ref album_id) = album.id
            && self
                .spotify
                .api
                .current_user_saved_albums_delete(vec![album_id.as_str()])
                .is_err()
        {
            return;
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

        if let Some(ref artist_id) = artist.id
            && self
                .spotify
                .api
                .user_follow_artists(vec![artist_id.as_str()])
                .is_err()
        {
            return;
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

        if let Some(ref artist_id) = artist.id
            && self
                .spotify
                .api
                .user_unfollow_artists(vec![artist_id.as_str()])
                .is_err()
        {
            return;
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

/// Check whether the cached liked songs are still up to date, using only the first page.
///
/// Spotify lists liked songs newest first, so a new like shows up at the top of the first page,
/// and an unlike lowers the `total`. If the first page and the total both match the cache, the
/// cache is kept and the other pages aren't fetched.
///
/// A change made in another app is missed only if both still match afterwards. For example, one
/// song is liked and another unliked, then the 50 newest songs are unliked and liked again in
/// their old order. Comparing `added_at` would catch it, but songs liked in ncspot have none, so
/// each of those likes would refetch every page, even though the cache is already up to date.
/// Fixed by `:update` or any later like or unlike in another app, once the library next loads.
///
/// An empty cache never counts as up to date. It may be empty because an older version wrote the
/// file, so it wasn't loaded, and the file still needs replacing.
fn tracks_unchanged(total: u32, first_page: &[Track], cached: &[Track]) -> bool {
    !cached.is_empty()
        && total as usize == cached.len()
        && first_page
            .iter()
            .zip(cached)
            .all(|(new, old)| new.id == old.id)
}

/// Check whether the cached saved albums are still up to date, using only the first page.
///
/// This works like [tracks_unchanged], but saved albums are cached sorted by artist, so instead of
/// comparing the order, check that every album on the first page is cached.
///
/// Albums saved in ncspot can lack data the API gives (such as their tracks) and have no
/// `added_at`, so refetch once if any are cached.
fn albums_unchanged(total: u32, first_page: &[Album], cached: &[Album]) -> bool {
    !cached.is_empty()
        && total as usize == cached.len()
        && cached.iter().all(|a| a.added_at.is_some())
        && first_page
            .iter()
            .all(|new| cached.iter().any(|old| old.id == new.id))
}

/// Returns `true` if the order changed.
fn sort_albums(albums: &mut [Album]) -> bool {
    if albums.is_sorted_by_key(album_sort_key) {
        return false;
    }
    albums.sort_unstable_by_key(album_sort_key);
    true
}

fn album_sort_key(album: &Album) -> String {
    format!(
        "{}{}{}",
        trim_leading_the(&album.artists[0]).to_lowercase(),
        album.year,
        trim_leading_the(&album.title).to_lowercase()
    )
}

fn trim_leading_the(name: &str) -> &str {
    name.strip_prefix("The ").unwrap_or(name)
}

/// Whether `local` must be reloaded: the snapshot changed or the tracks failed to load. Empty
/// tracks with a non-zero count are included, to repair caches from before this check.
fn is_stale(local: &Playlist, remote: &Playlist) -> bool {
    local.snapshot_id != remote.snapshot_id || !local.is_loaded()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracks(ids: &[&str]) -> Vec<Track> {
        ids.iter()
            .map(|id| Track {
                id: Some(id.to_string()),
                uri: String::new(),
                title: String::new(),
                track_number: 0,
                disc_number: 0,
                duration: 0,
                artists: Vec::new(),
                artist_ids: Vec::new(),
                album: None,
                album_id: None,
                album_artists: Vec::new(),
                cover_url: None,
                url: String::new(),
                added_at: None,
                list_index: 0,
                is_local: false,
                is_playable: None,
            })
            .collect()
    }

    fn album(id: &str, artist: &str, title: &str, added_at: Option<&str>) -> Album {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "title": title,
            "artists": [artist],
            "artist_ids": [],
            "year": "2000",
            "added_at": added_at,
        }))
        .unwrap()
    }

    fn albums(ids: &[&str]) -> Vec<Album> {
        ids.iter()
            .map(|id| album(id, "artist", "title", Some("2026-01-01T00:00:00Z")))
            .collect()
    }

    #[test]
    fn tracks_with_same_total_and_first_page_are_unchanged() {
        assert!(tracks_unchanged(
            3,
            &tracks(&["c", "b"]),
            &tracks(&["c", "b", "a"])
        ));
    }

    #[test]
    fn track_removal_changes_the_total() {
        assert!(!tracks_unchanged(
            2,
            &tracks(&["c", "b"]),
            &tracks(&["c", "b", "a"])
        ));
    }

    #[test]
    fn track_addition_shows_in_the_first_page() {
        assert!(!tracks_unchanged(
            3,
            &tracks(&["d", "c"]),
            &tracks(&["c", "b", "a"])
        ));
    }

    #[test]
    fn track_saved_again_moves_to_the_top() {
        assert!(!tracks_unchanged(
            3,
            &tracks(&["a", "c"]),
            &tracks(&["c", "b", "a"])
        ));
    }

    #[test]
    fn albums_cached_in_another_order_are_unchanged() {
        assert!(albums_unchanged(
            3,
            &albums(&["c", "b"]),
            &albums(&["a", "b", "c"])
        ));
    }

    #[test]
    fn album_removal_changes_the_total() {
        assert!(!albums_unchanged(
            2,
            &albums(&["c", "b"]),
            &albums(&["a", "b", "c"])
        ));
    }

    #[test]
    fn album_addition_shows_in_the_first_page() {
        assert!(!albums_unchanged(
            3,
            &albums(&["d", "c"]),
            &albums(&["a", "b", "c"])
        ));
    }

    #[test]
    fn album_saved_in_ncspot_is_refreshed() {
        let mut cached = albums(&["a", "b", "c"]);
        cached[0].added_at = None;
        assert!(!albums_unchanged(3, &albums(&["c", "b"]), &cached));
    }

    #[test]
    fn empty_cache_is_never_unchanged() {
        assert!(!tracks_unchanged(0, &[], &[]));
        assert!(!albums_unchanged(0, &[], &[]));
    }

    #[test]
    fn albums_sort_ignores_case_and_leading_the() {
        let mut list = vec![
            album("1", "The Beatles", "Abbey Road", None),
            album("2", "abba", "Arrival", None),
            album("3", "Cream", "Disraeli Gears", None),
        ];
        assert!(sort_albums(&mut list));
        let ids: Vec<_> = list.iter().map(|a| a.id.clone().unwrap()).collect();
        assert_eq!(ids, ["2", "1", "3"]);
        assert!(!sort_albums(&mut list));
    }

    fn playlist(snapshot_id: &str, num_tracks: usize, tracks: Option<Vec<Playable>>) -> Playlist {
        Playlist {
            id: "id".to_string(),
            name: "name".to_string(),
            owner_id: "owner".to_string(),
            owner_name: None,
            snapshot_id: snapshot_id.to_string(),
            num_tracks,
            tracks,
            collaborative: false,
        }
    }

    #[test]
    fn playlist_is_stale_when_snapshot_changed() {
        assert!(is_stale(
            &playlist("a", 0, Some(vec![])),
            &playlist("b", 0, None)
        ));
    }

    #[test]
    fn playlist_is_stale_when_tracks_failed_to_load() {
        let remote = playlist("a", 53, None);
        assert!(is_stale(&playlist("a", 53, Some(vec![])), &remote));
        assert!(is_stale(&playlist("a", 53, None), &remote));
    }

    #[test]
    fn loaded_playlist_without_playable_tracks_is_not_stale() {
        assert!(!is_stale(
            &playlist("a", 0, Some(vec![])),
            &playlist("a", 3, None)
        ));
    }

    #[test]
    fn empty_playlist_is_not_stale() {
        assert!(!is_stale(
            &playlist("a", 0, Some(vec![])),
            &playlist("a", 0, None)
        ));
    }

    #[test]
    fn failed_reload_keeps_cached_tracks_and_snapshot() {
        let cfg = Config::new_for_test();
        let ev = EventManager::new_for_test();
        let spotify = Spotify::new_for_test(cfg.clone(), ev.clone());
        let library = Library::new_for_test(ev, spotify, cfg);

        let tracks = vec![crate::queue::tests::make_track(1)];
        library.append_or_update(playlist("old", 1, Some(tracks)), false);
        let mut renamed = playlist("new", 2, None);
        renamed.name = "renamed".to_string();
        library.append_or_update(renamed, false);

        let store = library.playlists.read().unwrap();
        assert_eq!(store.len(), 1);
        assert_eq!(store[0].name, "renamed");
        assert_eq!(store[0].snapshot_id, "old");
        assert_eq!(store[0].tracks.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn failed_repair_stays_stale() {
        let cfg = Config::new_for_test();
        let ev = EventManager::new_for_test();
        let spotify = Spotify::new_for_test(cfg.clone(), ev.clone());
        let library = Library::new_for_test(ev, spotify, cfg);

        library.append_or_update(playlist("a", 53, Some(vec![])), false);
        library.append_or_update(playlist("a", 53, None), false);

        let store = library.playlists.read().unwrap();
        assert!(is_stale(&store[0], &playlist("a", 53, None)));
    }

    #[test]
    fn new_playlist_can_be_added_first() {
        let cfg = Config::new_for_test();
        let ev = EventManager::new_for_test();
        let spotify = Spotify::new_for_test(cfg.clone(), ev.clone());
        let library = Library::new_for_test(ev, spotify, cfg);

        library.append_or_update(playlist("a", 0, None), false);
        let mut new = playlist("a", 0, None);
        new.id = "new".to_string();
        library.append_or_update(new, true);

        let store = library.playlists.read().unwrap();
        assert_eq!(store[0].id, "new");
    }

    #[test]
    fn loaded_playlist_is_not_stale() {
        let local = playlist("a", 1, Some(vec![crate::queue::tests::make_track(1)]));
        assert!(!is_stale(&local, &playlist("a", 1, None)));
    }
}
