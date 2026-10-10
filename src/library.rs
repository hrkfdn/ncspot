use std::collections::HashMap;
use std::fs::File;
use std::iter::Iterator;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use log::{debug, error, info, warn};
use rspotify::model::Id;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::Config;
use crate::config::{self, CACHE_VERSION};
use crate::events::EventManager;
use crate::library_sync::{SavedItem, SavedPage, Synced, full_fetch_due, sync_liked, sync_saved};
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::show::Show;
use crate::model::track::Track;
use crate::rate_limit::{MAX_SYNC_RESUMES, RESUME_MIN_DELAY, random_jitter};
use crate::spotify::Spotify;

/// Keys of `UserState::library_full_fetch`.
const FULL_FETCH_TRACKS: &str = "tracks";
const FULL_FETCH_ALBUMS: &str = "albums";
const FULL_FETCH_SHOWS: &str = "shows";

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
    /// Set while a library sync runs, so a second one (Shift+U twice, or during the startup
    /// sync) is skipped instead of doubling the API traffic.
    syncing: Arc<AtomicBool>,
    /// Set while an automatic resume of an incomplete sync is scheduled.
    resume_pending: Arc<AtomicBool>,
    /// Automatic resumes since the last complete sync.
    resume_count: Arc<AtomicU32>,
    pub user_id: Option<String>,
    pub display_name: Option<String>,
    ev: EventManager,
    spotify: Spotify,
    pub cfg: Arc<Config>,
}

/// Outcome of [Library::fetch_playlists].
struct PlaylistSync {
    /// The remote playlist listing was read to the end.
    listing_complete: bool,
    /// Every playlist that needed a track download got it.
    items_complete: bool,
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
            syncing: Arc::new(AtomicBool::new(false)),
            resume_pending: Arc::new(AtomicBool::new(false)),
            resume_count: Arc::new(AtomicU32::new(0)),
            user_id: None,
            display_name: None,
            ev,
            spotify,
            cfg,
        })
    }

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
            syncing: Arc::new(AtomicBool::new(false)),
            resume_pending: Arc::new(AtomicBool::new(false)),
            resume_count: Arc::new(AtomicU32::new(0)),
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

        if let Some(position) = position
            && self.spotify.api.delete_playlist(id).is_ok()
        {
            self.playlists.write().unwrap().remove(position);
            self.save_cache(
                &config::cache_path(CACHE_PLAYLISTS),
                &self.playlists.read().unwrap(),
            );
            #[cfg(feature = "mpris")]
            self.spotify
                .send_mpris(crate::mpris::MprisCommand::EmitPlaylistChanged(Some(
                    id.to_string(),
                )));
        }
    }

    /// Set the playlist with `id` to contain only `tracks`. If the playlist already contains
    /// tracks, they will be removed. Update the cache to match the new state.
    pub fn overwrite_playlist(&self, id: &str, tracks: &[Playable]) {
        debug!("saving {} tracks to list {}", tracks.len(), id);
        self.spotify.api.overwrite_playlist(id, tracks);

        if self.fetch_playlists().listing_complete {
            self.save_cache(
                &config::cache_path(CACHE_PLAYLISTS),
                &self.playlists.read().unwrap(),
            );
        }
    }

    /// Create a playlist with the given `name` and add `tracks` to it.
    pub fn save_playlist(&self, name: &str, tracks: &[Playable]) {
        debug!("saving {} tracks to new list {}", tracks.len(), name);
        match self.spotify.api.create_playlist(name, None, None) {
            Ok(id) => self.overwrite_playlist(&id, tracks),
            Err(_) => error!("could not create new playlist.."),
        }
    }

    /// Update the local library and its cache on disk. Does nothing if a sync is already
    /// running. If the sync is left incomplete by a rate limit, a resume is scheduled for when
    /// the limit has passed.
    pub fn update_library(&self) {
        if self
            .syncing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            info!("library sync already running, skipped");
            return;
        }
        *self.is_done.write().unwrap() = false;

        let library = self.clone();
        thread::spawn(move || {
            let _guard = SyncGuard(library.syncing.clone());
            let sync_start = Instant::now();

            let t_tracks = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_TRACKS),
                        library.tracks.write().unwrap().as_mut(),
                    );
                    let complete = library.fetch_tracks();
                    library.save_cache(
                        &config::cache_path(CACHE_TRACKS),
                        &library.tracks.read().unwrap(),
                    );
                    complete
                })
            };

            let t_albums = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_ALBUMS),
                        library.albums.write().unwrap().as_mut(),
                    );
                    let complete = library.fetch_albums();
                    library.save_cache(
                        &config::cache_path(CACHE_ALBUMS),
                        &library.albums.read().unwrap(),
                    );
                    complete
                })
            };

            let t_artists = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_ARTISTS),
                        library.artists.write().unwrap().as_mut(),
                    );
                    library.fetch_artists()
                })
            };

            let t_playlists = {
                let library = library.clone();
                thread::spawn(move || {
                    library.load_cache(
                        &config::cache_path(CACHE_PLAYLISTS),
                        library.playlists.write().unwrap().as_mut(),
                    );
                    let sync = library.fetch_playlists();
                    if sync.listing_complete {
                        library.save_cache(
                            &config::cache_path(CACHE_PLAYLISTS),
                            &library.playlists.read().unwrap(),
                        );
                    }
                    sync.listing_complete && sync.items_complete
                })
            };

            let t_shows = {
                let library = library.clone();
                thread::spawn(move || library.fetch_shows())
            };

            let tracks_complete = t_tracks.join().unwrap();
            let artists_complete = t_artists.join().unwrap();

            library.populate_artists();
            library.save_cache(
                &config::cache_path(CACHE_ARTISTS),
                &library.artists.read().unwrap(),
            );

            let albums_complete = t_albums.join().unwrap();
            let playlists_complete = t_playlists.join().unwrap();
            let shows_complete = t_shows.join().unwrap();

            *library.is_done.write().unwrap() = true;
            library.ev.trigger();

            let complete = tracks_complete
                && artists_complete
                && albums_complete
                && playlists_complete
                && shows_complete;
            library.schedule_resume(complete, sync_start);
        });
    }

    /// After a sync that started at `sync_start`: reset the resume count if it was complete,
    /// or schedule one resume if it was left incomplete by a rate limit.
    fn schedule_resume(&self, complete: bool, sync_start: Instant) {
        if complete {
            self.resume_count.store(0, Ordering::SeqCst);
            return;
        }
        let rate_limited = self.spotify.api.rate_limited_since(sync_start);
        let count = self.resume_count.load(Ordering::SeqCst);
        let pending = self.resume_pending.load(Ordering::SeqCst);
        if !should_schedule_resume(complete, rate_limited, count, pending) {
            if !rate_limited {
                info!("library sync incomplete, not rate limited: no automatic resume");
            } else if count >= MAX_SYNC_RESUMES {
                warn!(
                    "library sync still incomplete after {MAX_SYNC_RESUMES} automatic resumes, \
                     waiting for a manual update"
                );
            } else {
                info!("library sync resume already pending");
            }
            return;
        }
        if self
            .resume_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            info!("library sync resume already pending");
            return;
        }

        let longest = self
            .spotify
            .api
            .rate_limit_status()
            .into_iter()
            .map(|(_, remaining)| remaining)
            .max()
            .unwrap_or_default();
        let delay = resume_delay(longest, random_jitter());
        info!(
            "library sync incomplete after a rate limit, resuming in {}s (attempt {}/{MAX_SYNC_RESUMES})",
            delay.as_secs(),
            count + 1
        );

        let library = self.clone();
        thread::spawn(move || {
            thread::sleep(delay);
            library.resume_pending.store(false, Ordering::SeqCst);
            library.resume_count.fetch_add(1, Ordering::SeqCst);
            library.update_library();
        });
    }

    /// Returns the current Unix time and whether the list stored under `key` needs a full
    /// fetch now, logging when it does. The state lock is released before returning.
    fn full_fetch_check(&self, key: &str, what: &str) -> (i64, bool) {
        let now = chrono::Utc::now().timestamp();
        let last = self.cfg.state().library_full_fetch.get(key).copied();
        let force_full = full_fetch_due(last, now);
        if force_full {
            debug!("{what}: full refresh (cache older than 24h)");
        }
        (now, force_full)
    }

    /// Record that the list stored under `key` was fully fetched at `now` (Unix seconds).
    /// Persisted with the rest of the user state on quit.
    fn record_full_fetch(&self, key: &str, now: i64) {
        self.cfg.with_state_mut(|state| {
            state.library_full_fetch.insert(key.to_string(), now);
        });
    }

    /// Fetch the saved shows from the web API and save them to the local library, unless they
    /// are unchanged. Returns false, leaving the stored shows unchanged, if a page failed.
    fn fetch_shows(&self) -> bool {
        debug!("loading shows");
        let stored_ids: Vec<Option<String>> = self
            .shows
            .read()
            .unwrap()
            .iter()
            .map(|show| Some(show.id.clone()))
            .collect();
        let (now, force_full) = self.full_fetch_check(FULL_FETCH_SHOWS, "shows");
        let synced = sync_saved(&stored_ids, force_full, |offset| {
            debug!("shows page at offset {offset}");
            let page = self.spotify.api.get_saved_shows(offset as u32).ok()?;
            Some(SavedPage {
                total: page.total as usize,
                has_next: page.next.is_some(),
                items: page
                    .items
                    .iter()
                    .map(|saved| SavedItem {
                        id: Some(saved.show.id.id().to_string()),
                        added_at: DateTime::parse_from_rfc3339(&saved.added_at)
                            .ok()
                            .map(|date| date.with_timezone(&Utc)),
                        value: Show::from(&saved.show),
                    })
                    .collect(),
            })
        });
        match synced {
            Synced::Full(shows) => {
                *self.shows.write().unwrap() = shows;
                self.record_full_fetch(FULL_FETCH_SHOWS, now);
                true
            }
            Synced::Unchanged => {
                debug!("shows unchanged");
                true
            }
            Synced::Prepended { .. } => unreachable!("sync_saved never prepends"),
            Synced::Failed => {
                error!("Failed to fetch shows.");
                false
            }
        }
    }

    /// Fetch the playlists from the web API and save them to the local library. This synchronizes
    /// the local version with the remote, pruning removed playlists in the process. Reports
    /// whether the remote listing was complete (if not, the cache must not be saved as
    /// authoritative) and whether every playlist that needed it had its tracks loaded (if not,
    /// the cached copies were kept and the sync is incomplete).
    fn fetch_playlists(&self) -> PlaylistSync {
        debug!("loading playlists");
        let known_at_start: Vec<String> = self
            .playlists
            .read()
            .unwrap()
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let mut list_order = Vec::new();
        let mut items_complete = true;

        let lists_page = self.spotify.api.current_user_playlist();
        let mut lists_batch = Some(lists_page.items.read().unwrap().clone());
        while let Some(lists) = lists_batch {
            for (index, remote) in lists.iter().enumerate() {
                list_order.push(remote.id.clone());

                if self.needs_download(remote) {
                    info!("updating playlist {} (index: {})", remote.name, index);
                    let mut playlist: Playlist = remote.clone();
                    playlist.tracks = None;
                    if playlist.load_tracks(&self.spotify) {
                        self.append_or_update(playlist);
                        // trigger redraw
                        self.trigger_redraw();
                    } else {
                        items_complete = false;
                        warn!(
                            "failed to load tracks for playlist {}, keeping cached copy",
                            remote.name
                        );
                    }
                } else {
                    // Snapshot unchanged, so the cached tracks are current and we
                    // skip the costly reload. But a cache written before the
                    // cover_url field existed stores it as None; backfill it from
                    // the freshly-fetched list metadata so playlist art appears
                    // without forcing a full re-download.
                    let mut store = self.playlists.write().unwrap();
                    if let Some(local) = store.iter_mut().find(|p| p.id == remote.id)
                        && local.cover_url != remote.cover_url
                    {
                        local.cover_url = remote.cover_url.clone();
                    }
                }
            }
            lists_batch = lists_page.next();
        }

        let listing_complete = !lists_page.failed();
        if listing_complete {
            reconcile_playlists(
                &mut self.playlists.write().unwrap(),
                &known_at_start,
                &list_order,
                true,
            );
        } else {
            warn!("playlist list incomplete, keeping cached playlists");
        }

        // trigger redraw
        self.trigger_redraw();

        // Notify MPRIS clients that the playlist set has been refreshed.
        #[cfg(feature = "mpris")]
        self.spotify
            .send_mpris(crate::mpris::MprisCommand::EmitPlaylistChanged(None));

        PlaylistSync {
            listing_complete,
            items_complete,
        }
    }

    /// Fetch the artists from the web API and save them to the local library. Returns false,
    /// leaving the stored items unchanged, if a page failed.
    fn fetch_artists(&self) -> bool {
        let mut artists: Vec<Artist> = Vec::new();
        let mut last: Option<&str> = None;
        let mut i = 0u32;

        loop {
            let page = self.spotify.api.current_user_followed_artists(last);
            debug!("artists page: {i}");
            i += 1;
            if page.is_err() {
                error!("Failed to fetch artists.");
                return false;
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
        true
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

    /// Fetch the saved albums from the web API and store them in the local library, unless they
    /// are unchanged. Returns false, leaving the stored albums unchanged, if a page failed.
    fn fetch_albums(&self) -> bool {
        let stored_ids: Vec<Option<String>> = self
            .albums
            .read()
            .unwrap()
            .iter()
            .map(|album| album.id.clone())
            .collect();
        let (now, force_full) = self.full_fetch_check(FULL_FETCH_ALBUMS, "albums");
        let synced = sync_saved(&stored_ids, force_full, |offset| {
            debug!("albums page at offset {offset}");
            let page = self
                .spotify
                .api
                .current_user_saved_albums(offset as u32)
                .ok()?;
            Some(SavedPage {
                total: page.total as usize,
                has_next: page.next.is_some(),
                items: page
                    .items
                    .iter()
                    .map(|saved| SavedItem {
                        id: Some(saved.album.id.id().to_string()),
                        added_at: Some(saved.added_at),
                        value: Album::from(saved),
                    })
                    .collect(),
            })
        });
        let mut albums = match synced {
            Synced::Full(albums) => albums,
            Synced::Unchanged => {
                debug!("albums unchanged");
                return true;
            }
            Synced::Prepended { .. } => unreachable!("sync_saved never prepends"),
            Synced::Failed => {
                error!("Failed to fetch albums.");
                return false;
            }
        };

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
        self.record_full_fetch(FULL_FETCH_ALBUMS, now);
        true
    }

    /// Fetch the liked songs from the web API and save them in the local library. Only new
    /// likes are downloaded when nothing else changed. Returns false, leaving the stored songs
    /// unchanged, if a page failed.
    fn fetch_tracks(&self) -> bool {
        let cached: Vec<Track> = self.tracks.read().unwrap().clone();
        let (now, force_full) = self.full_fetch_check(FULL_FETCH_TRACKS, "liked songs");
        let synced = sync_liked(
            &cached,
            force_full,
            |track: &Track| track.id.clone(),
            |offset| {
                debug!("tracks page at offset {offset}");
                let page = self
                    .spotify
                    .api
                    .current_user_saved_tracks(offset as u32)
                    .ok()?;
                Some(SavedPage {
                    total: page.total as usize,
                    has_next: page.next.is_some(),
                    items: page
                        .items
                        .iter()
                        .map(|saved| SavedItem {
                            id: saved.track.id.as_ref().map(|id| id.id().to_string()),
                            added_at: Some(saved.added_at),
                            value: Track::from(saved),
                        })
                        .collect(),
                })
            },
        );
        match synced {
            Synced::Prepended { items, new_count } => {
                debug!("liked songs: {new_count} new");
                *self.tracks.write().unwrap() = items;
                true
            }
            Synced::Full(items) => {
                *self.tracks.write().unwrap() = items;
                self.record_full_fetch(FULL_FETCH_TRACKS, now);
                true
            }
            Synced::Unchanged => true,
            Synced::Failed => {
                error!("Failed to fetch tracks.");
                false
            }
        }
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

        let already_present = {
            let mut store = self.playlists.write().unwrap();
            if !store.iter().any(|p| p.id == playlist.id) {
                store.insert(0, playlist.clone());
                false
            } else {
                true
            }
        };

        if !already_present {
            self.save_cache(
                &config::cache_path(CACHE_PLAYLISTS),
                &self.playlists.read().unwrap(),
            );

            #[cfg(feature = "mpris")]
            self.spotify
                .send_mpris(crate::mpris::MprisCommand::EmitPlaylistChanged(Some(
                    playlist.id.clone(),
                )));
        }
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

/// Remove local playlists that existed when the sync started (`known_at_start`) but are missing
/// from `remote_ids`, and sort the rest in remote order. Only done when the remote listing was
/// complete; a partial listing must never delete cached playlists, and a playlist created while
/// the sync was running must survive.
fn reconcile_playlists(
    local: &mut Vec<Playlist>,
    known_at_start: &[String],
    remote_ids: &[String],
    listing_complete: bool,
) {
    if !listing_complete {
        return;
    }
    local.retain(|p| !known_at_start.contains(&p.id) || remote_ids.contains(&p.id));
    local.sort_by_key(|p| remote_ids.iter().position(|x| x == &p.id));
}

/// Clears a library's `syncing` flag when dropped, so the flag is released even if the sync
/// thread panics.
struct SyncGuard(Arc<AtomicBool>);

impl Drop for SyncGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Whether to schedule an automatic resume after a sync: only if it was incomplete because of
/// a rate limit, the resume cap is not reached, and no resume is already pending.
fn should_schedule_resume(complete: bool, rate_limited: bool, count: u32, pending: bool) -> bool {
    !complete && rate_limited && count < MAX_SYNC_RESUMES && !pending
}

/// Delay before a resumed sync: the longest open rate-limit window, but at least
/// [RESUME_MIN_DELAY], plus `jitter`.
fn resume_delay(longest_remaining: Duration, jitter: Duration) -> Duration {
    longest_remaining.max(RESUME_MIN_DELAY) + jitter
}

#[cfg(test)]
mod test {
    use super::*;

    fn playlist(id: &str) -> Playlist {
        Playlist {
            id: id.to_string(),
            name: id.to_string(),
            owner_id: String::new(),
            owner_name: None,
            snapshot_id: String::new(),
            num_tracks: 0,
            tracks: None,
            collaborative: false,
            cover_url: None,
        }
    }

    fn ids(list: &[Playlist]) -> Vec<&str> {
        list.iter().map(|p| p.id.as_str()).collect()
    }

    fn ids_owned(list: &[Playlist]) -> Vec<String> {
        list.iter().map(|p| p.id.clone()).collect()
    }

    #[test]
    fn incomplete_listing_keeps_all_cached_playlists() {
        let mut local = vec![playlist("a"), playlist("b"), playlist("c")];
        let known = ids_owned(&local);
        reconcile_playlists(&mut local, &known, &["a".to_string()], false);
        assert_eq!(ids(&local), ["a", "b", "c"]);
    }

    #[test]
    fn complete_listing_prunes_and_sorts() {
        let mut local = vec![playlist("a"), playlist("b"), playlist("c")];
        let known = ids_owned(&local);
        reconcile_playlists(
            &mut local,
            &known,
            &["c".to_string(), "a".to_string()],
            true,
        );
        assert_eq!(ids(&local), ["c", "a"]);
    }

    #[test]
    fn playlist_created_during_sync_is_not_pruned() {
        let mut local = vec![playlist("a"), playlist("b"), playlist("new")];
        let known = vec!["a".to_string(), "b".to_string()];
        reconcile_playlists(&mut local, &known, &["a".to_string()], true);
        assert_eq!(ids(&local), ["new", "a"]);
    }

    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use crate::config::Config;
    use crate::events::EventManager;
    use crate::rate_limit::{MAX_SYNC_RESUMES, RESUME_MIN_DELAY};
    use crate::spotify::Spotify;

    #[test]
    fn resume_only_for_an_incomplete_rate_limited_sync() {
        assert!(should_schedule_resume(false, true, 0, false));
        assert!(!should_schedule_resume(true, true, 0, false));
        assert!(!should_schedule_resume(false, false, 0, false));
    }

    #[test]
    fn resume_only_one_pending_at_a_time() {
        assert!(!should_schedule_resume(false, true, 0, true));
    }

    #[test]
    fn resume_stops_at_the_cap() {
        assert!(should_schedule_resume(
            false,
            true,
            MAX_SYNC_RESUMES - 1,
            false
        ));
        assert!(!should_schedule_resume(
            false,
            true,
            MAX_SYNC_RESUMES,
            false
        ));
    }

    #[test]
    fn resume_delay_never_below_the_minimum() {
        let jitter = Duration::from_millis(250);
        assert_eq!(
            resume_delay(Duration::ZERO, jitter),
            RESUME_MIN_DELAY + jitter
        );
        assert_eq!(
            resume_delay(Duration::from_secs(5), jitter),
            RESUME_MIN_DELAY + jitter
        );
        assert_eq!(
            resume_delay(Duration::from_secs(90), jitter),
            Duration::from_secs(90) + jitter
        );
    }

    #[test]
    fn sync_guard_releases_the_flag_on_panic() {
        let flag = Arc::new(AtomicBool::new(true));
        let in_thread = flag.clone();
        let result = std::thread::spawn(move || {
            let _guard = SyncGuard(in_thread);
            panic!("fetch thread failed");
        })
        .join();
        assert!(result.is_err());
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn a_second_sync_is_skipped_while_one_runs() {
        let cfg = Config::new_for_test();
        let ev = EventManager::new_for_test();
        let spotify = Spotify::new_for_test(cfg.clone(), ev.clone());
        let library = Library::new_for_test(ev, spotify, cfg);
        library.syncing.store(true, Ordering::SeqCst);
        *library.is_done.write().unwrap() = true;

        library.update_library();

        assert!(*library.is_done.read().unwrap());
        assert!(library.syncing.load(Ordering::SeqCst));
    }
}
