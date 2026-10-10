use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::thread;
use std::time::Duration;

use crate::application::ASYNC_RUNTIME;
use crate::authentication::WebApiClient;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use log::{debug, error, info, warn};
use rspotify::http::HttpError;
use rspotify::model::{
    AlbumId, AlbumType, ArtistId, CursorBasedPage, EpisodeId, FullAlbum, FullArtist, FullEpisode,
    FullPlaylist, FullShow, FullTrack, ItemPositions, LibraryId, Market, Page, PlayableId,
    PlaylistId, PlaylistResult, PrivateUser, Recommendations, SavedAlbum, SavedTrack, SearchResult,
    SearchType, Show, ShowId, SimplifiedTrack, TrackId, UserId,
};
use rspotify::{AuthCodeSpotify, ClientError, ClientResult, Config, prelude::*};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::category::Category;
use crate::model::episode::Episode;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::track::Track;
use crate::spotify_worker::WorkerCommand;
use crate::ui::pagination::{ApiPage, ApiResult};

/// HTTP status of `error`, 401 for a missing token, or 0 if it has none.
fn status_of(error: &ClientError) -> u16 {
    match error {
        ClientError::Http(e) => match e.as_ref() {
            HttpError::StatusCode(response) => response.status(),
            _ => 0,
        },
        ClientError::InvalidToken => 401,
        _ => 0,
    }
}

/// A Web API client for one client ID.
#[derive(Clone)]
struct ApiClient {
    /// Rspotify web API.
    api: AuthCodeSpotify,
    kind: WebApiClient,
    /// Time at which the token expires.
    token_expiration: Arc<RwLock<DateTime<Utc>>>,
    /// Held while refreshing, so refreshes don't overlap.
    refresh_lock: Arc<Mutex<()>>,
    /// Calls that ended rate limited.
    rate_limit_failures: Arc<AtomicUsize>,
}

impl ApiClient {
    fn new(kind: WebApiClient) -> Self {
        let config = Config {
            token_refreshing: false,
            ..Default::default()
        };
        let api = AuthCodeSpotify::with_config(
            rspotify::Credentials::new(kind.client_id(), ""),
            rspotify::OAuth::default(),
            config,
        );
        Self {
            api,
            kind,
            token_expiration: Arc::new(RwLock::new(Utc::now())),
            refresh_lock: Arc::new(Mutex::new(())),
            rate_limit_failures: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Whether the token expires within 5 minutes.
    fn needs_token_update(&self) -> bool {
        let delta = *self.token_expiration.read().unwrap() - Utc::now();
        delta.num_seconds() <= 60 * 5
    }

    fn update_token_blocking(&self) {
        let _guard = self
            .refresh_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // another refresh may have finished meanwhile
        if !self.needs_token_update() {
            return;
        }
        info!("Token for {:?} is about to expire, renewing", self.kind);

        // logins only happen at startup, while stdout is usable. The blocking refresh would panic
        // in async callers like MPRIS without block_in_place.
        let token = tokio::task::block_in_place(|| {
            crate::authentication::get_rspotify_token(&self.kind, false)
        });
        match token {
            Ok(token) => {
                let expires_at = token
                    .expires_at
                    .unwrap_or_else(|| Utc::now() + ChronoDuration::hours(1));
                *self.api.token.lock().unwrap() = Some(token);
                *self.token_expiration.write().unwrap() = expires_at;
            }
            Err(e) => {
                error!("Failed to update token: {e}");
                if self.kind != WebApiClient::Shared {
                    // due again in 10 minutes, once inside the renewal window
                    *self.token_expiration.write().unwrap() =
                        Utc::now() + ChronoDuration::minutes(15);
                }
            }
        }
    }
}

/// Whether the own token is for the logged in account.
enum OwnAccount {
    /// Time of the next check.
    Unchecked(DateTime<Utc>),
    Verified,
    Mismatch,
}

/// Rate limit failures per client.
#[derive(Clone, Copy)]
pub struct RateLimits {
    shared: usize,
    own: usize,
}

/// Convenient wrapper around the rspotify web API functionality.
#[derive(Clone)]
pub struct WebApi {
    shared: ApiClient,
    /// Tried first once its account is verified.
    own: Option<ApiClient>,
    own_account: Arc<Mutex<OwnAccount>>,
    /// Held during an account check, so only one runs.
    own_check: Arc<Mutex<()>>,
    /// The username of the logged in user.
    user: Option<String>,
    /// Sender of the mpsc channel to the [Spotify](crate::spotify::Spotify) worker thread.
    worker_channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
}

impl WebApi {
    /// Calls try `client_id` first.
    pub fn new(client_id: Option<String>) -> Self {
        Self {
            shared: ApiClient::new(WebApiClient::Shared),
            own: client_id.map(|id| ApiClient::new(WebApiClient::Own(id))),
            own_account: Arc::new(Mutex::new(OwnAccount::Unchecked(Utc::now()))),
            own_check: Arc::new(Mutex::new(())),
            user: None,
            worker_channel: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the username for use with the API.
    pub fn set_user(&mut self, user: Option<String>) {
        self.user = user;
    }

    /// Set the sending end of the channel to the worker thread, managed by
    /// [Spotify](crate::spotify::Spotify).
    pub(crate) fn set_worker_channel(
        &mut self,
        channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
    ) {
        self.worker_channel = channel;
    }

    /// Update the authentication tokens when they expire.
    pub fn update_token(&self) -> Option<JoinHandle<()>> {
        let expiring: Vec<ApiClient> = std::iter::once(&self.shared)
            .chain(self.own.as_ref())
            .filter(|c| c.needs_token_update())
            .cloned()
            .collect();
        if expiring.is_empty() {
            return None;
        }

        Some(ASYNC_RUNTIME.get().unwrap().spawn_blocking(move || {
            for client in expiring {
                client.update_token_blocking();
            }
        }))
    }

    /// Calls so far that ended rate limited.
    pub fn rate_limits(&self) -> RateLimits {
        let failures = |c: &ApiClient| c.rate_limit_failures.load(Ordering::Relaxed);
        RateLimits {
            shared: failures(&self.shared),
            own: self.own.as_ref().map_or(0, failures),
        }
    }

    /// Whether a call would end on a client rate limited since `since`. Only `user_owned` data
    /// can use the own client ID, which falls back to the shared one.
    pub fn rate_limited_since(&self, since: RateLimits, user_owned: bool) -> bool {
        let now = self.rate_limits();
        let shared_limited = now.shared != since.shared;
        if user_owned && self.own_verified() == Some(true) {
            shared_limited && now.own != since.own
        } else {
            shared_limited
        }
    }

    /// Whether the own account is verified, or `None` if a check is due.
    fn own_verified(&self) -> Option<bool> {
        match *self
            .own_account
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
        {
            OwnAccount::Verified => Some(true),
            OwnAccount::Mismatch => Some(false),
            OwnAccount::Unchecked(next) => (Utc::now() < next).then_some(false),
        }
    }

    /// The own client once its account is verified. Rechecks 10 minutes after a failed check.
    fn own_client(&self) -> Option<&ApiClient> {
        let own = self.own.as_ref()?;
        let user = self.user.as_ref()?;
        if let Some(verified) = self.own_verified() {
            return verified.then_some(own);
        }
        let _guard = self
            .own_check
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // another check may have finished meanwhile
        if let Some(verified) = self.own_verified() {
            return verified.then_some(own);
        }

        own.update_token_blocking();
        // no retry, as a Retry-After wait would block all calls
        let account = match own.api.current_user() {
            Ok(me) if me.id.id() == user => OwnAccount::Verified,
            Ok(me) => {
                error!(
                    "Own client ID is logged in as {}, not {user}. Remove {:?} to log in again.",
                    me.id.id(),
                    own.kind.token_path()
                );
                OwnAccount::Mismatch
            }
            Err(e) => {
                warn!("Own client ID account check failed, retrying in 10 minutes: {e}");
                OwnAccount::Unchecked(Utc::now() + ChronoDuration::minutes(10))
            }
        };
        let verified = matches!(account, OwnAccount::Verified);
        *self
            .own_account
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = account;
        verified.then_some(own)
    }

    /// Execute `api_call`, own client ID first. Refusals and rate limits (401/403/404/429) are
    /// repeated with the shared one, other errors aren't, as a write may have gone through.
    fn api_with_retry<F, R>(&self, api_call: F) -> Option<R>
    where
        F: Fn(&AuthCodeSpotify) -> ClientResult<R>,
    {
        if let Some(own) = self.own_client() {
            match self.call_with(own, &api_call) {
                Ok(v) => return Some(v),
                Err(status @ (401 | 403 | 404 | 429)) => {
                    debug!("falling back to shared client id after {status}");
                    if status == 401 {
                        warn!("Own client ID unauthorized, retrying in 10 minutes");
                        *self
                            .own_account
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner) =
                            OwnAccount::Unchecked(Utc::now() + ChronoDuration::minutes(10));
                    }
                }
                Err(_) => return None,
            }
        }
        self.call_with(&self.shared, &api_call).ok()
    }

    /// Execute `api_call` with `client`. On a rate limit, the shared client ID waits and retries
    /// once, the own one fails right away for the fallback. Errors are the HTTP status, or 0.
    fn call_with<F, R>(&self, client: &ApiClient, api_call: &F) -> Result<R, u16>
    where
        F: Fn(&AuthCodeSpotify) -> ClientResult<R>,
    {
        let error = match api_call(&client.api) {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        debug!("api error: {error:?}");
        let own = client.kind != WebApiClient::Shared;
        let result = match status_of(&error) {
            status @ (403 | 404 | 429) if own => Err(status),
            429 => {
                let waiting_duration = if let ClientError::Http(e) = &error
                    && let HttpError::StatusCode(response) = e.as_ref()
                {
                    response
                        .header("Retry-After")
                        .and_then(|v| v.parse::<u64>().ok())
                } else {
                    None
                };
                debug!("rate limit hit. waiting {waiting_duration:?} seconds");
                thread::sleep(Duration::from_secs(waiting_duration.unwrap_or(0)));
                api_call(&client.api).map_err(|e| status_of(&e))
            }
            401 => {
                debug!("token unauthorized. trying refresh..");
                // refresh if due and wait for it, keeping a failed refresh's backoff
                client.update_token_blocking();
                api_call(&client.api).map_err(|e| status_of(&e))
            }
            status => {
                error!("unhandled api error: {error:?}");
                Err(status)
            }
        };
        if matches!(result, Err(429)) {
            client.rate_limit_failures.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    /// Append `tracks` at `position` in the playlist with `playlist_id`.
    pub fn append_tracks(
        &self,
        playlist_id: &str,
        tracks: &[Playable],
        position: Option<u32>,
    ) -> Result<PlaylistResult, ()> {
        self.api_with_retry(|api| {
            let trackids: Vec<PlayableId> = tracks
                .iter()
                .filter_map(|playable| playable.into())
                .collect();
            api.playlist_add_items(
                PlaylistId::from_id(playlist_id).unwrap(),
                trackids.iter().map(|id| id.as_ref()),
                position,
            )
        })
        .ok_or(())
    }

    pub fn delete_tracks(
        &self,
        playlist_id: &str,
        snapshot_id: &str,
        playables: &[Playable],
    ) -> Result<PlaylistResult, ()> {
        self.api_with_retry(move |api| {
            let playable_ids: Vec<PlayableId> = playables
                .iter()
                .filter_map(|playable| playable.into())
                .collect();
            let positions = playables
                .iter()
                .map(|playable| [playable.list_index() as u32])
                .collect::<Vec<_>>();
            let item_pos: Vec<ItemPositions> = playable_ids
                .iter()
                .zip(positions.iter())
                .map(|(id, positions)| ItemPositions {
                    id: id.as_ref(),
                    positions,
                })
                .collect();
            api.playlist_remove_specific_occurrences_of_items(
                PlaylistId::from_id(playlist_id).unwrap(),
                item_pos,
                Some(snapshot_id),
            )
        })
        .ok_or(())
    }

    /// Set the playlist with `id` to contain only `tracks`. If the playlist already contains
    /// tracks, they will be removed.
    pub fn overwrite_playlist(&self, id: &str, tracks: &[Playable]) {
        // create mutable copy for chunking
        let mut tracks: Vec<Playable> = tracks.to_vec();

        // we can only send 100 tracks per request
        let mut remainder = if tracks.len() > 100 {
            Some(tracks.split_off(100))
        } else {
            None
        };

        let replace_items = self.api_with_retry(|api| {
            let playable_ids: Vec<PlayableId> = tracks
                .iter()
                .filter_map(|playable| playable.into())
                .collect();
            api.playlist_replace_items(
                PlaylistId::from_id(id).unwrap(),
                playable_ids.iter().map(|p| p.as_ref()),
            )
        });

        if replace_items.is_some() {
            debug!("saved {} tracks to playlist {}", tracks.len(), id);
            while let Some(ref mut tracks) = remainder.clone() {
                // grab the next set of 100 tracks
                remainder = if tracks.len() > 100 {
                    Some(tracks.split_off(100))
                } else {
                    None
                };

                debug!("adding another {} tracks to playlist", tracks.len());
                if self.append_tracks(id, tracks, None).is_ok() {
                    debug!("{} tracks successfully added", tracks.len());
                } else {
                    error!("error saving tracks to playlists {id}");
                    return;
                }
            }
        } else {
            error!("error saving tracks to playlist {id}");
        }
    }

    /// Delete the playlist with the given `id`.
    pub fn delete_playlist(&self, id: &str) -> Result<(), ()> {
        self.api_with_retry(|api| {
            api.library_remove([LibraryId::Playlist(PlaylistId::from_id(id).unwrap())])
        })
        .ok_or(())
    }

    /// Create a playlist with the given `name`, `public` visibility and `description`. Returns the
    /// id of the newly created playlist.
    pub fn create_playlist(
        &self,
        name: &str,
        public: Option<bool>,
        description: Option<&str>,
    ) -> Result<String, ()> {
        let result = self.api_with_retry(|api| {
            api.user_playlist_create(
                UserId::from_id(self.user.as_ref().unwrap()).unwrap(),
                name,
                public,
                None,
                description,
            )
        });
        result.map(|r| r.id.id().to_string()).ok_or(())
    }

    /// Fetch the album with the given `album_id`.
    pub fn album(&self, album_id: &str) -> Result<FullAlbum, ()> {
        debug!("fetching album {album_id}");
        let aid = AlbumId::from_id(album_id).map_err(|_| ())?;
        self.api_with_retry(|api| api.album(aid.clone(), Some(Market::FromToken)))
            .ok_or(())
    }

    /// Fetch the artist with the given `artist_id`.
    pub fn artist(&self, artist_id: &str) -> Result<FullArtist, ()> {
        let aid = ArtistId::from_id(artist_id).map_err(|_| ())?;
        self.api_with_retry(|api| api.artist(aid.clone())).ok_or(())
    }

    /// Fetch the playlist with the given `playlist_id`.
    pub fn playlist(&self, playlist_id: &str) -> Result<FullPlaylist, ()> {
        let pid = PlaylistId::from_id(playlist_id).map_err(|_| ())?;
        self.api_with_retry(|api| api.playlist(pid.clone(), None, Some(Market::FromToken)))
            .ok_or(())
    }

    /// Fetch the track with the given `track_id`.
    pub fn track(&self, track_id: &str) -> Result<FullTrack, ()> {
        let tid = TrackId::from_id(track_id).map_err(|_| ())?;
        self.api_with_retry(|api| api.track(tid.clone(), Some(Market::FromToken)))
            .ok_or(())
    }

    /// Fetch the show with the given `show_id`.
    pub fn show(&self, show_id: &str) -> Result<FullShow, ()> {
        let sid = ShowId::from_id(show_id).map_err(|_| ())?;
        self.api_with_retry(|api| api.get_a_show(sid.clone(), Some(Market::FromToken)))
            .ok_or(())
    }

    /// Fetch the episode with the given `episode_id`.
    pub fn episode(&self, episode_id: &str) -> Result<FullEpisode, ()> {
        let eid = EpisodeId::from_id(episode_id).map_err(|_| ())?;
        self.api_with_retry(|api| api.get_an_episode(eid.clone(), Some(Market::FromToken)))
            .ok_or(())
    }

    /// Get recommendations based on the seeds provided with `seed_artists`, `seed_genres` and
    /// `seed_tracks`.
    pub fn recommendations(
        &self,
        seed_artists: Option<Vec<&str>>,
        seed_genres: Option<Vec<&str>>,
        seed_tracks: Option<Vec<&str>>,
    ) -> Result<Recommendations, ()> {
        self.api_with_retry(|api| {
            let seed_artistids = seed_artists.as_ref().map(|artistids| {
                artistids
                    .iter()
                    .map(|id| ArtistId::from_id(*id).unwrap())
                    .collect::<Vec<ArtistId>>()
            });
            let seed_trackids = seed_tracks.as_ref().map(|trackids| {
                trackids
                    .iter()
                    .map(|id| TrackId::from_id(*id).unwrap())
                    .collect::<Vec<TrackId>>()
            });
            api.recommendations(
                std::iter::empty(),
                seed_artistids,
                seed_genres.clone(),
                seed_trackids,
                Some(Market::FromToken),
                Some(100),
            )
        })
        .ok_or(())
    }

    /// Search for items of `searchtype` using the provided `query`. Limit the results to `limit`
    /// items with the given `offset` from the start.
    pub fn search(
        &self,
        searchtype: SearchType,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Result<SearchResult, ()> {
        self.api_with_retry(|api| {
            api.search(
                query,
                searchtype,
                Some(Market::FromToken),
                None,
                Some(limit),
                Some(offset),
            )
        })
        .ok_or(())
    }

    /// Fetch all the current user's playlists.
    pub fn current_user_playlist(&self) -> ApiResult<Playlist> {
        const MAX_LIMIT: u32 = 50;
        let spotify = self.clone();
        let fetch_page = move |offset: u32| {
            debug!("fetching user playlists, offset: {offset}");
            spotify.api_with_retry(|api| {
                match api.current_user_playlists_manual(Some(MAX_LIMIT), Some(offset)) {
                    Ok(page) => Ok(ApiPage {
                        offset: page.offset,
                        total: page.total,
                        items: page.items.iter().map(|sp| sp.into()).collect(),
                    }),
                    Err(e) => Err(e),
                }
            })
        };
        ApiResult::new(MAX_LIMIT, Arc::new(fetch_page))
    }

    /// Get the tracks in the playlist given by `playlist_id`.
    pub fn user_playlist_tracks(&self, playlist_id: &str) -> ApiResult<Playable> {
        const MAX_LIMIT: u32 = 100;
        let spotify = self.clone();
        let playlist_id = playlist_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!("fetching playlist {playlist_id} tracks, offset: {offset}");
            spotify.api_with_retry(|api| {
                match api.playlist_items_manual(
                    PlaylistId::from_id(&playlist_id).unwrap(),
                    None,
                    Some(Market::FromToken),
                    Some(MAX_LIMIT),
                    Some(offset),
                ) {
                    Ok(page) => Ok(ApiPage {
                        offset: page.offset,
                        total: page.total,
                        items: page
                            .items
                            .iter()
                            .filter(|pt| {
                                if let Some(t) = pt.item.as_ref()
                                    && !t.is_unknown()
                                {
                                    true
                                } else {
                                    error!("Could not process item {pt:?}, ignoring");
                                    false
                                }
                            })
                            .enumerate()
                            .flat_map(|(index, pt)| {
                                pt.item.as_ref().map(|t| {
                                    let mut playable: Playable = t.into();
                                    // TODO: set these
                                    playable.set_added_at(pt.added_at);
                                    playable.set_list_index(page.offset as usize + index);
                                    playable
                                })
                            })
                            .collect(),
                    }),
                    Err(e) => Err(e),
                }
            })
        };
        ApiResult::new(MAX_LIMIT, Arc::new(fetch_page))
    }

    /// Fetch all the tracks in the album with the given `album_id`. Limit the results to `limit`
    /// items, with `offset` from the beginning.
    pub fn album_tracks(
        &self,
        album_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Page<SimplifiedTrack>, ()> {
        debug!("fetching album tracks {album_id}");
        self.api_with_retry(|api| {
            api.album_track_manual(
                AlbumId::from_id(album_id).unwrap(),
                Some(Market::FromToken),
                Some(limit),
                Some(offset),
            )
        })
        .ok_or(())
    }

    /// Fetch all the albums of the given `artist_id`. `album_type` determines which type of albums
    /// to fetch.
    pub fn artist_albums(
        &self,
        artist_id: &str,
        album_type: Option<AlbumType>,
    ) -> ApiResult<Album> {
        const MAX_SIZE: u32 = 50;
        let spotify = self.clone();
        let artist_id = artist_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!("fetching artist {artist_id} albums, offset: {offset}");
            spotify.api_with_retry(|api| {
                match api.artist_albums_manual(
                    ArtistId::from_id(&artist_id).unwrap(),
                    album_type.as_ref().copied(),
                    Some(Market::FromToken),
                    Some(MAX_SIZE),
                    Some(offset),
                ) {
                    Ok(page) => {
                        let mut albums: Vec<Album> =
                            page.items.iter().map(|sa| sa.into()).collect();
                        albums.sort_by(|a, b| b.year.cmp(&a.year));
                        Ok(ApiPage {
                            offset: page.offset,
                            total: page.total,
                            items: albums,
                        })
                    }
                    Err(e) => Err(e),
                }
            })
        };

        ApiResult::new(MAX_SIZE, Arc::new(fetch_page))
    }

    /// Get all the episodes of the show with the given `show_id`.
    pub fn show_episodes(&self, show_id: &str) -> ApiResult<Episode> {
        const MAX_SIZE: u32 = 50;
        let spotify = self.clone();
        let show_id = show_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!("fetching show {} episodes, offset: {}", show_id, offset);
            spotify.api_with_retry(|api| {
                match api.get_shows_episodes_manual(
                    ShowId::from_id(&show_id).unwrap(),
                    Some(Market::FromToken),
                    Some(50),
                    Some(offset),
                ) {
                    Ok(page) => Ok(ApiPage {
                        offset: page.offset,
                        total: page.total,
                        items: page.items.iter().map(|se| se.into()).collect(),
                    }),
                    Err(e) => Err(e),
                }
            })
        };

        ApiResult::new(MAX_SIZE, Arc::new(fetch_page))
    }

    /// Get the user's saved shows.
    pub fn get_saved_shows(&self, offset: u32) -> Result<Page<Show>, ()> {
        self.api_with_retry(|api| api.get_saved_show_manual(Some(50), Some(offset)))
            .ok_or(())
    }

    /// Add the shows with the given `ids` to the user's library.
    pub fn save_shows(&self, ids: &[&str]) -> Result<(), ()> {
        self.api_with_retry(|api| {
            api.library_add(
                ids.iter()
                    .map(|id| LibraryId::Show(ShowId::from_id(*id).unwrap()))
                    .collect::<Vec<LibraryId>>(),
            )
        })
        .ok_or(())
    }

    /// Remove the shows with `ids` from the user's library.
    pub fn unsave_shows(&self, ids: &[&str]) -> Result<(), ()> {
        self.api_with_retry(|api| {
            api.library_remove(
                ids.iter()
                    .map(|id| LibraryId::Show(ShowId::from_id(*id).unwrap()))
                    .collect::<Vec<LibraryId>>(),
            )
        })
        .ok_or(())
    }

    /// Get the user's followed artists. `last` is an artist id. If it is specified, the artists
    /// after the one with this id will be retrieved.
    pub fn current_user_followed_artists(
        &self,
        last: Option<&str>,
    ) -> Result<CursorBasedPage<FullArtist>, ()> {
        self.api_with_retry(|api| api.current_user_followed_artists(last, Some(50)))
            .ok_or(())
    }

    /// Add the logged in user to the followers of the artists with the given `ids`.
    pub fn user_follow_artists(&self, ids: Vec<&str>) -> Result<(), ()> {
        self.api_with_retry(|api| {
            api.library_add(
                ids.iter()
                    .map(|id| LibraryId::Artist(ArtistId::from_id(*id).unwrap()))
                    .collect::<Vec<LibraryId>>(),
            )
        })
        .ok_or(())
    }

    /// Remove the logged in user to the followers of the artists with the given `ids`.
    pub fn user_unfollow_artists(&self, ids: Vec<&str>) -> Result<(), ()> {
        self.api_with_retry(|api| {
            api.library_remove(
                ids.iter()
                    .map(|id| LibraryId::Artist(ArtistId::from_id(*id).unwrap()))
                    .collect::<Vec<LibraryId>>(),
            )
        })
        .ok_or(())
    }

    /// Get the user's saved albums, starting at the given `offset`. The result is paginated.
    pub fn current_user_saved_albums(&self, offset: u32) -> Result<Page<SavedAlbum>, ()> {
        self.api_with_retry(|api| {
            api.current_user_saved_albums_manual(Some(Market::FromToken), Some(50), Some(offset))
        })
        .ok_or(())
    }

    /// Add the albums with the given `ids` to the user's saved albums.
    pub fn current_user_saved_albums_add(&self, ids: Vec<&str>) -> Result<(), ()> {
        self.api_with_retry(|api| {
            api.library_add(
                ids.iter()
                    .map(|id| LibraryId::Album(AlbumId::from_id(*id).unwrap()))
                    .collect::<Vec<LibraryId>>(),
            )
        })
        .ok_or(())
    }

    /// Remove the albums with the given `ids` from the user's saved albums.
    pub fn current_user_saved_albums_delete(&self, ids: Vec<&str>) -> Result<(), ()> {
        self.api_with_retry(|api| {
            api.library_remove(
                ids.iter()
                    .map(|id| LibraryId::Album(AlbumId::from_id(*id).unwrap()))
                    .collect::<Vec<LibraryId>>(),
            )
        })
        .ok_or(())
    }

    /// Get the user's saved tracks, starting at the given `offset`. The result is paginated.
    pub fn current_user_saved_tracks(&self, offset: u32) -> Result<Page<SavedTrack>, ()> {
        self.api_with_retry(|api| {
            api.current_user_saved_tracks_manual(Some(Market::FromToken), Some(50), Some(offset))
        })
        .ok_or(())
    }

    /// Add the tracks with the given `ids` to the user's saved tracks.
    pub fn current_user_saved_tracks_add(&self, ids: Vec<&str>) -> Result<(), ()> {
        self.api_with_retry(|api| {
            api.library_add(
                ids.iter()
                    .map(|id| LibraryId::Track(TrackId::from_id(*id).unwrap()))
                    .collect::<Vec<LibraryId>>(),
            )
        })
        .ok_or(())
    }

    /// Remove the tracks with the given `ids` from the user's saved tracks.
    pub fn current_user_saved_tracks_delete(&self, ids: Vec<&str>) -> Result<(), ()> {
        self.api_with_retry(|api| {
            api.library_remove(
                ids.iter()
                    .map(|id| LibraryId::Track(TrackId::from_id(*id).unwrap()))
                    .collect::<Vec<LibraryId>>(),
            )
        })
        .ok_or(())
    }

    /// Add the logged in user to the followers of the playlist with the given `id`.
    pub fn user_playlist_follow_playlist(&self, id: &str) -> Result<(), ()> {
        self.api_with_retry(|api| {
            api.library_add([LibraryId::Playlist(PlaylistId::from_id(id).unwrap())])
        })
        .ok_or(())
    }

    /// Get the top tracks of the artist with the given `id`.
    pub fn artist_top_tracks(&self, id: &str) -> Result<Vec<Track>, ()> {
        #[allow(deprecated)]
        self.api_with_retry(|api| {
            api.artist_top_tracks(ArtistId::from_id(id).unwrap(), Some(Market::FromToken))
        })
        .map(|ft| ft.iter().map(|t| t.into()).collect())
        .ok_or(())
    }

    /// Get artists related to the artist with the given `id`.
    pub fn artist_related_artists(&self, id: &str) -> Result<Vec<Artist>, ()> {
        #[allow(deprecated)]
        self.api_with_retry(|api| api.artist_related_artists(ArtistId::from_id(id).unwrap()))
            .map(|fa| fa.iter().map(|a| a.into()).collect())
            .ok_or(())
    }

    /// Get the available categories.
    pub fn categories(&self) -> ApiResult<Category> {
        const MAX_LIMIT: u32 = 50;
        let spotify = self.clone();
        let fetch_page = move |offset: u32| {
            debug!("fetching categories, offset: {offset}");
            spotify.api_with_retry(|api| {
                #[allow(deprecated)]
                match api.categories_manual(
                    None,
                    Some(Market::FromToken),
                    Some(MAX_LIMIT),
                    Some(offset),
                ) {
                    Ok(page) => Ok(ApiPage {
                        offset: page.offset,
                        total: page.total,
                        items: page.items.iter().map(|cat| cat.into()).collect(),
                    }),
                    Err(e) => Err(e),
                }
            })
        };
        ApiResult::new(MAX_LIMIT, Arc::new(fetch_page))
    }

    /// Get the playlists in the category given by `category_id`.
    pub fn category_playlists(&self, category_id: &str) -> ApiResult<Playlist> {
        const MAX_LIMIT: u32 = 50;
        let spotify = self.clone();
        let category_id = category_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!("fetching category playlists, offset: {offset}");
            spotify.api_with_retry(|api| {
                #[allow(deprecated)]
                match api.category_playlists_manual(
                    &category_id,
                    Some(Market::FromToken),
                    Some(MAX_LIMIT),
                    Some(offset),
                ) {
                    Ok(page) => Ok(ApiPage {
                        offset: page.offset,
                        total: page.total,
                        items: page.items.iter().map(|sp| sp.into()).collect(),
                    }),
                    Err(e) => Err(e),
                }
            })
        };
        ApiResult::new(MAX_LIMIT, Arc::new(fetch_page))
    }

    /// Get details about the logged in user.
    pub fn current_user(&self) -> Result<PrivateUser, ()> {
        self.api_with_retry(|api| api.current_user()).ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bump(client: &ApiClient) {
        client.rate_limit_failures.fetch_add(1, Ordering::Relaxed);
    }

    #[test]
    fn rate_limited_since_uses_the_client_that_would_be_called() {
        let api = WebApi::new(Some("id".to_string()));
        *api.own_account.lock().unwrap() = OwnAccount::Verified;
        let own = api.own.as_ref().unwrap();

        let start = api.rate_limits();
        bump(&api.shared);
        assert!(!api.rate_limited_since(start, true));
        assert!(api.rate_limited_since(start, false));

        // own falls back to shared
        let start = api.rate_limits();
        bump(own);
        assert!(!api.rate_limited_since(start, true));
        assert!(!api.rate_limited_since(start, false));
        bump(&api.shared);
        assert!(api.rate_limited_since(start, true));

        // unverified, own data uses the shared client ID too
        *api.own_account.lock().unwrap() = OwnAccount::Mismatch;
        let start = api.rate_limits();
        bump(&api.shared);
        assert!(api.rate_limited_since(start, true));

        let api = WebApi::new(None);
        let start = api.rate_limits();
        bump(&api.shared);
        assert!(api.rate_limited_since(start, true));
    }
}
