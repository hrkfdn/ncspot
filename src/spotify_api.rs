use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::application::ASYNC_RUNTIME;
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

use crate::authentication::DEFAULT_WEB_API_CLIENT_ID;
use crate::events::{Event, EventManager};
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::category::Category;
use crate::model::episode::Episode;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::track::Track;
use crate::rate_limit::{ClientLabel, DEFAULT_RETRY_AFTER, ThrottleState, parse_retry_after};
use crate::spotify_worker::WorkerCommand;
use crate::ui::pagination::{ApiPage, ApiResult};

/// Which Web API client a call goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    /// Allowed for Development Mode apps: uses the personal client if one is configured and not
    /// disabled.
    Bulk,
    /// Restricted for apps created after November 2024 or in Development Mode (recommendations,
    /// related artists, top tracks, browse, search with more than 10 results): always uses the
    /// shared client.
    Restricted,
}

/// Whether a call that failed on the personal client with HTTP `status` should be retried once
/// on the shared client. A 401 means the personal token needs a refresh (started in the
/// background), so any call is retried. A 403 or 404 on a playlist read means a Spotify-owned
/// playlist, which Development Mode apps can't read.
fn should_retry_on_shared(status: u16, playlist_read: bool) -> bool {
    status == 401 || (playlist_read && matches!(status, 403 | 404))
}

/// One rspotify client: a client ID with its own token and rate-limit state.
#[derive(Clone)]
struct ApiClient {
    client_id: String,
    label: ClientLabel,
    api: AuthCodeSpotify,
    /// Time at which the token expires.
    token_expiration: Arc<RwLock<DateTime<Utc>>>,
    /// Rate-limit window of this client.
    throttle: Arc<RwLock<ThrottleState>>,
    /// Set when the personal client's token can't be refreshed without a new login. Shared by
    /// all clones, so every copy of [WebApi] stops using it at once.
    disabled: Arc<AtomicBool>,
    /// Set while a personal token refresh is running, so a burst of 401s starts only one.
    refreshing: Arc<AtomicBool>,
}

impl ApiClient {
    fn new(client_id: &str, label: ClientLabel) -> Self {
        let config = Config {
            token_refreshing: false,
            ..Default::default()
        };
        let api = AuthCodeSpotify::with_config(
            rspotify::Credentials::new(client_id, ""),
            rspotify::OAuth::default(),
            config,
        );
        Self {
            client_id: client_id.to_string(),
            label,
            api,
            token_expiration: Arc::new(RwLock::new(Utc::now())),
            throttle: Arc::new(RwLock::new(ThrottleState::default())),
            disabled: Arc::new(AtomicBool::new(false)),
            refreshing: Arc::new(AtomicBool::new(false)),
        }
    }

    fn is_disabled(&self) -> bool {
        self.disabled.load(Ordering::Relaxed)
    }
}

/// Convenient wrapper around the rspotify web API functionality.
#[derive(Clone)]
pub struct WebApi {
    /// Client for the shared ncspot client ID. Always present.
    shared: ApiClient,
    /// Client for the personal client ID, if one is configured.
    personal: Option<ApiClient>,
    /// The username of the logged in user.
    user: Option<String>,
    /// Sender of the mpsc channel to the [Spotify](crate::spotify::Spotify) worker thread.
    worker_channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
    /// Event manager used to report rate limits to the UI. Unset in tests.
    events: Option<EventManager>,
}

impl WebApi {
    /// Create the Web API wrapper. A personal client is set up only if `cfg` has a personal
    /// client ID.
    pub fn new(cfg: &crate::config::Config) -> Self {
        let personal = cfg
            .personal_client_id()
            .map(|personal| ApiClient::new(&personal.id, ClientLabel::Personal));
        Self {
            shared: ApiClient::new(DEFAULT_WEB_API_CLIENT_ID, ClientLabel::Shared),
            personal,
            user: None,
            worker_channel: Arc::new(RwLock::new(None)),
            events: None,
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

    /// Set the event manager used to report rate limits to the UI.
    pub(crate) fn set_events(&mut self, events: EventManager) {
        self.events = Some(events);
    }

    /// The personal client, if configured and not disabled.
    fn active_personal(&self) -> Option<&ApiClient> {
        self.personal
            .as_ref()
            .filter(|client| !client.is_disabled())
    }

    /// The client that calls on `route` go through.
    fn client(&self, route: Route) -> &ApiClient {
        match route {
            Route::Bulk => self.active_personal().unwrap_or(&self.shared),
            Route::Restricted => &self.shared,
        }
    }

    /// Refresh `client`'s token in the background. Unless `force` is set, nothing happens while
    /// the token is valid for more than 5 minutes.
    ///
    /// The shared client may fall back to the browser login, as it always has. The personal
    /// client never does: if its token can't be refreshed, it is disabled for the rest of the
    /// session and its traffic goes to the shared client.
    fn refresh_client(&self, client: &ApiClient, force: bool) -> Option<JoinHandle<()>> {
        if client.is_disabled() {
            return None;
        }
        if !force {
            let token_expiration = client.token_expiration.read().unwrap();
            let now = Utc::now();
            let delta = *token_expiration - now;

            // token is valid for 5 more minutes, renewal is not necessary yet
            if delta.num_seconds() > 60 * 5 {
                return None;
            }

            info!(
                "{} client token will expire in {delta}, renewing",
                client.label
            );
        }

        // Only one personal refresh at a time: a burst of 401s during a sync would otherwise
        // start one refresh per failed call. The shared client keeps its old behavior.
        if client.label == ClientLabel::Personal && client.refreshing.swap(true, Ordering::AcqRel) {
            return None;
        }

        let api_token = client.api.token.clone();
        let api_token_expiration = client.token_expiration.clone();
        let client_id = client.client_id.clone();
        let label = client.label;
        let disabled = client.disabled.clone();
        let refreshing = client.refreshing.clone();
        let events = self.events.clone();
        Some(ASYNC_RUNTIME.get().unwrap().spawn_blocking(move || {
            let result = match label {
                ClientLabel::Shared => crate::authentication::get_rspotify_token(&client_id),
                ClientLabel::Personal => {
                    crate::authentication::refresh_rspotify_token(&client_id, force)
                }
            };
            match result {
                Ok(token) => {
                    let expires_at = token
                        .expires_at
                        .unwrap_or_else(|| Utc::now() + ChronoDuration::hours(1));
                    *api_token.lock().unwrap() = Some(token);
                    *api_token_expiration.write().unwrap() = expires_at;
                }
                Err(e) if label == ClientLabel::Personal => {
                    error!(
                        "Failed to refresh the personal client ID's token, using the shared \
                         client ID until restart: {e}"
                    );
                    disabled.store(true, Ordering::Relaxed);
                    if let Some(events) = &events {
                        events.try_send(Event::PersonalClientDisabled { reason: e });
                    }
                }
                Err(e) => {
                    error!("Failed to update token: {e}");
                }
            }
            refreshing.store(false, Ordering::Release);
        }))
    }

    /// Update the authentication tokens of all active clients when they expire. The returned
    /// handle finishes when every client's renewal has finished.
    pub fn update_token(&self) -> Option<JoinHandle<()>> {
        let handles: Vec<JoinHandle<()>> = std::iter::once(&self.shared)
            .chain(self.active_personal())
            .filter_map(|client| self.refresh_client(client, false))
            .collect();
        if handles.is_empty() {
            return None;
        }
        Some(ASYNC_RUNTIME.get().unwrap().spawn(async move {
            for handle in handles {
                let _ = handle.await;
            }
        }))
    }

    /// Remaining rate-limit window of each throttled client, personal first.
    pub fn rate_limit_status(&self) -> Vec<(ClientLabel, Duration)> {
        let now = Instant::now();
        self.personal
            .iter()
            .chain(std::iter::once(&self.shared))
            .filter_map(|client| {
                client
                    .throttle
                    .read()
                    .unwrap()
                    .remaining(now)
                    .map(|remaining| (client.label, remaining))
            })
            .collect()
    }

    /// Record a 429 on `client`. At the start of a throttle episode, tell the UI (which shows a
    /// notification) and keep the status bar countdown redrawing until the window has passed.
    fn on_rate_limited(&self, client: &ApiClient, retry_after: Option<Duration>) {
        let new_episode = client
            .throttle
            .write()
            .unwrap()
            .record_429(Instant::now(), retry_after);
        if !new_episode {
            return;
        }
        let Some(events) = self.events.clone() else {
            return;
        };
        events.try_send(Event::RateLimited {
            client: client.label,
            retry_after: retry_after.unwrap_or(DEFAULT_RETRY_AFTER),
        });

        // The UI refresh tick doesn't run while playback is stopped, so redraw the countdown
        // from here until the window has passed, then once more to clear it.
        let throttle = client.throttle.clone();
        thread::spawn(move || {
            while throttle.read().unwrap().is_active(Instant::now()) {
                if !events.try_trigger() {
                    return;
                }
                thread::sleep(Duration::from_secs(1));
            }
            events.try_trigger();
        });
    }

    /// Execute `api_call` on `client`, retrying once after a rate limit (429). A 401 on the
    /// shared client refreshes and retries, as before; a 401 on the personal client starts a
    /// forced background refresh and fails, so the caller can retry on the shared client. On
    /// failure, returns the HTTP status if the server sent one.
    fn call_on<F, R>(&self, client: &ApiClient, api_call: &F) -> Result<R, Option<u16>>
    where
        F: Fn(&AuthCodeSpotify) -> ClientResult<R>,
    {
        match api_call(&client.api) {
            Ok(v) => {
                client
                    .throttle
                    .write()
                    .unwrap()
                    .record_success(Instant::now());
                Ok(v)
            }
            Err(ClientError::Http(error)) => {
                debug!("http error: {error:?}");
                match error.as_ref() {
                    HttpError::StatusCode(response) => match response.status() {
                        429 => {
                            let header = response.header("Retry-After");
                            let retry_after = parse_retry_after(header);
                            warn!(
                                "rate limited on the {} client ID at {}, Retry-After: {header:?}",
                                client.label,
                                response.get_url()
                            );
                            self.on_rate_limited(client, retry_after);
                            thread::sleep(retry_after.unwrap_or(Duration::ZERO));
                            api_call(&client.api).map_err(|_| None)
                        }
                        401 if client.label == ClientLabel::Personal => {
                            debug!("personal token unauthorized, refreshing in the background");
                            self.refresh_client(client, true);
                            Err(Some(401))
                        }
                        401 => {
                            debug!("token unauthorized. trying refresh..");
                            self.refresh_client(client, false)
                                .ok_or(None)
                                .and_then(|_| api_call(&client.api).map_err(|_| None))
                        }
                        status => Err(Some(status)),
                    },
                    _ => Err(None),
                }
            }
            Err(e) => {
                error!("unhandled api error: {e}");
                Err(None)
            }
        }
    }

    /// Execute `api_call` on the client for `route`. If it fails on the personal client in a way
    /// [should_retry_on_shared] accepts, retry it once on the shared client.
    fn call_with_fallback<F, R>(&self, route: Route, playlist_read: bool, api_call: F) -> Option<R>
    where
        F: Fn(&AuthCodeSpotify) -> ClientResult<R>,
    {
        let client = self.client(route);
        match self.call_on(client, &api_call) {
            Ok(v) => Some(v),
            Err(Some(status))
                if client.label == ClientLabel::Personal
                    && should_retry_on_shared(status, playlist_read) =>
            {
                debug!("personal client ID got HTTP {status}, retrying on the shared client ID");
                self.call_with_fallback(Route::Restricted, false, api_call)
            }
            Err(Some(status)) => {
                error!("unhandled api error: HTTP {status}");
                None
            }
            Err(None) => None,
        }
    }

    /// Execute `api_call` on the client for `route`, retrying once if a rate limit occurs.
    fn api_with_retry<F, R>(&self, route: Route, api_call: F) -> Option<R>
    where
        F: Fn(&AuthCodeSpotify) -> ClientResult<R>,
    {
        self.call_with_fallback(route, false, api_call)
    }

    /// Like [Self::api_with_retry] on [Route::Bulk], but a 403 or 404 on the personal client is
    /// also retried on the shared client. Development Mode apps can't read Spotify-owned
    /// playlists (Discover Weekly, editorial playlists, ...).
    fn api_with_playlist_fallback<F, R>(&self, api_call: F) -> Option<R>
    where
        F: Fn(&AuthCodeSpotify) -> ClientResult<R>,
    {
        self.call_with_fallback(Route::Bulk, true, api_call)
    }

    /// Append `tracks` at `position` in the playlist with `playlist_id`.
    pub fn append_tracks(
        &self,
        playlist_id: &str,
        tracks: &[Playable],
        position: Option<u32>,
    ) -> Result<PlaylistResult, ()> {
        self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, move |api| {
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

        let replace_items = self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
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
        let result = self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
            api.album(aid.clone(), Some(Market::FromToken))
        })
        .ok_or(())
    }

    /// Fetch the artist with the given `artist_id`.
    pub fn artist(&self, artist_id: &str) -> Result<FullArtist, ()> {
        let aid = ArtistId::from_id(artist_id).map_err(|_| ())?;
        self.api_with_retry(Route::Bulk, |api| api.artist(aid.clone()))
            .ok_or(())
    }

    /// Fetch the playlist with the given `playlist_id`.
    pub fn playlist(&self, playlist_id: &str) -> Result<FullPlaylist, ()> {
        let pid = PlaylistId::from_id(playlist_id).map_err(|_| ())?;
        self.api_with_playlist_fallback(|api| {
            api.playlist(pid.clone(), None, Some(Market::FromToken))
        })
        .ok_or(())
    }

    /// Fetch the track with the given `track_id`.
    pub fn track(&self, track_id: &str) -> Result<FullTrack, ()> {
        let tid = TrackId::from_id(track_id).map_err(|_| ())?;
        self.api_with_retry(Route::Bulk, |api| {
            api.track(tid.clone(), Some(Market::FromToken))
        })
        .ok_or(())
    }

    /// Fetch the show with the given `show_id`.
    pub fn show(&self, show_id: &str) -> Result<FullShow, ()> {
        let sid = ShowId::from_id(show_id).map_err(|_| ())?;
        self.api_with_retry(Route::Bulk, |api| {
            api.get_a_show(sid.clone(), Some(Market::FromToken))
        })
        .ok_or(())
    }

    /// Fetch the episode with the given `episode_id`.
    pub fn episode(&self, episode_id: &str) -> Result<FullEpisode, ()> {
        let eid = EpisodeId::from_id(episode_id).map_err(|_| ())?;
        self.api_with_retry(Route::Bulk, |api| {
            api.get_an_episode(eid.clone(), Some(Market::FromToken))
        })
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
        self.api_with_retry(Route::Restricted, |api| {
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
        self.api_with_retry(Route::Restricted, |api| {
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
            spotify.api_with_retry(Route::Bulk, |api| {
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
            spotify.api_with_playlist_fallback(|api| {
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
        self.api_with_retry(Route::Bulk, |api| {
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
            spotify.api_with_retry(Route::Bulk, |api| {
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
            spotify.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
            api.get_saved_show_manual(Some(50), Some(offset))
        })
        .ok_or(())
    }

    /// Add the shows with the given `ids` to the user's library.
    pub fn save_shows(&self, ids: &[&str]) -> Result<(), ()> {
        self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
            api.current_user_followed_artists(last, Some(50))
        })
        .ok_or(())
    }

    /// Add the logged in user to the followers of the artists with the given `ids`.
    pub fn user_follow_artists(&self, ids: Vec<&str>) -> Result<(), ()> {
        self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
            api.current_user_saved_albums_manual(Some(Market::FromToken), Some(50), Some(offset))
        })
        .ok_or(())
    }

    /// Add the albums with the given `ids` to the user's saved albums.
    pub fn current_user_saved_albums_add(&self, ids: Vec<&str>) -> Result<(), ()> {
        self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
            api.current_user_saved_tracks_manual(Some(Market::FromToken), Some(50), Some(offset))
        })
        .ok_or(())
    }

    /// Add the tracks with the given `ids` to the user's saved tracks.
    pub fn current_user_saved_tracks_add(&self, ids: Vec<&str>) -> Result<(), ()> {
        self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
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
        self.api_with_retry(Route::Bulk, |api| {
            api.library_add([LibraryId::Playlist(PlaylistId::from_id(id).unwrap())])
        })
        .ok_or(())
    }

    /// Get the top tracks of the artist with the given `id`.
    pub fn artist_top_tracks(&self, id: &str) -> Result<Vec<Track>, ()> {
        #[allow(deprecated)]
        self.api_with_retry(Route::Restricted, |api| {
            api.artist_top_tracks(ArtistId::from_id(id).unwrap(), Some(Market::FromToken))
        })
        .map(|ft| ft.iter().map(|t| t.into()).collect())
        .ok_or(())
    }

    /// Get artists related to the artist with the given `id`.
    pub fn artist_related_artists(&self, id: &str) -> Result<Vec<Artist>, ()> {
        #[allow(deprecated)]
        self.api_with_retry(Route::Restricted, |api| {
            api.artist_related_artists(ArtistId::from_id(id).unwrap())
        })
        .map(|fa| fa.iter().map(|a| a.into()).collect())
        .ok_or(())
    }

    /// Get the available categories.
    pub fn categories(&self) -> ApiResult<Category> {
        const MAX_LIMIT: u32 = 50;
        let spotify = self.clone();
        let fetch_page = move |offset: u32| {
            debug!("fetching categories, offset: {offset}");
            spotify.api_with_retry(Route::Restricted, |api| {
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
            spotify.api_with_retry(Route::Restricted, |api| {
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
        self.api_with_retry(Route::Bulk, |api| api.current_user())
            .ok_or(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::Config;

    const PERSONAL_ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn refresh_client_does_nothing_for_a_disabled_client() {
        let api = WebApi::new(&Config::new_for_test_with_client_id(PERSONAL_ID));
        let personal = api.personal.as_ref().unwrap();
        personal.disabled.store(true, Ordering::SeqCst);
        assert!(api.refresh_client(personal, true).is_none());
    }

    #[test]
    fn without_personal_id_both_routes_use_the_shared_client() {
        let api = WebApi::new(&Config::new_for_test());
        assert_eq!(api.client(Route::Bulk).label, ClientLabel::Shared);
        assert_eq!(api.client(Route::Bulk).client_id, DEFAULT_WEB_API_CLIENT_ID);
        assert_eq!(api.client(Route::Restricted).label, ClientLabel::Shared);
    }

    #[test]
    fn with_personal_id_bulk_is_personal_and_restricted_is_shared() {
        let api = WebApi::new(&Config::new_for_test_with_client_id(PERSONAL_ID));
        assert_eq!(api.client(Route::Bulk).label, ClientLabel::Personal);
        assert_eq!(api.client(Route::Bulk).client_id, PERSONAL_ID);
        assert_eq!(api.client(Route::Restricted).label, ClientLabel::Shared);
        assert_eq!(
            api.client(Route::Restricted).client_id,
            DEFAULT_WEB_API_CLIENT_ID
        );
    }

    #[test]
    fn disabled_personal_client_routes_bulk_to_shared() {
        let api = WebApi::new(&Config::new_for_test_with_client_id(PERSONAL_ID));
        api.personal
            .as_ref()
            .unwrap()
            .disabled
            .store(true, Ordering::Relaxed);
        assert_eq!(api.client(Route::Bulk).label, ClientLabel::Shared);
    }

    #[test]
    fn disabling_is_visible_from_clones() {
        let api = WebApi::new(&Config::new_for_test_with_client_id(PERSONAL_ID));
        let clone = api.clone();
        api.personal
            .as_ref()
            .unwrap()
            .disabled
            .store(true, Ordering::Relaxed);
        assert_eq!(clone.client(Route::Bulk).label, ClientLabel::Shared);
    }

    #[test]
    fn personal_failures_retried_on_shared() {
        // A 401 is retried for any call.
        assert!(should_retry_on_shared(401, false));
        assert!(should_retry_on_shared(401, true));
        // 403 and 404 only for playlist reads (Spotify-owned playlists).
        assert!(should_retry_on_shared(403, true));
        assert!(should_retry_on_shared(404, true));
        assert!(!should_retry_on_shared(403, false));
        assert!(!should_retry_on_shared(404, false));
        // Everything else is not retried.
        assert!(!should_retry_on_shared(400, true));
        assert!(!should_retry_on_shared(429, true));
        assert!(!should_retry_on_shared(500, true));
    }

    #[test]
    fn rate_limit_status_is_empty_when_nothing_is_throttled() {
        let api = WebApi::new(&Config::new_for_test_with_client_id(PERSONAL_ID));
        assert!(api.rate_limit_status().is_empty());
    }

    #[test]
    fn rate_limit_status_lists_throttled_clients_personal_first() {
        let api = WebApi::new(&Config::new_for_test_with_client_id(PERSONAL_ID));
        let now = Instant::now();
        api.shared
            .throttle
            .write()
            .unwrap()
            .record_429(now, Some(Duration::from_secs(60)));
        api.personal
            .as_ref()
            .unwrap()
            .throttle
            .write()
            .unwrap()
            .record_429(now, Some(Duration::from_secs(30)));
        let labels: Vec<ClientLabel> = api
            .rate_limit_status()
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        assert_eq!(labels, vec![ClientLabel::Personal, ClientLabel::Shared]);
    }

    #[test]
    fn on_rate_limited_opens_the_window_of_that_client_only() {
        let api = WebApi::new(&Config::new_for_test_with_client_id(PERSONAL_ID));
        api.on_rate_limited(api.client(Route::Bulk), Some(Duration::from_secs(30)));
        let status = api.rate_limit_status();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].0, ClientLabel::Personal);
    }
}
