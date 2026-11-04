use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::activity::ACTIVITY;
use crate::application::ASYNC_RUNTIME;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use librespot_core::{Session, SpotifyUri};
use librespot_protocol::playlist4_external::ListChanges;
use log::{debug, error, info, warn};
use rspotify::http::HttpError;
use rspotify::model::{
    AlbumId, AlbumType, ArtistId, CursorBasedPage, EpisodeId, FullAlbum, FullArtist, FullEpisode,
    FullPlaylist, FullShow, FullTrack, ItemPositions, LibraryId, Market, Page, PlayableId,
    PlaylistId, PlaylistResult, PrivateUser, Recommendations, SavedAlbum, SavedTrack, SearchResult,
    SearchType, Show, ShowId, SimplifiedAlbum, SimplifiedEpisode, SimplifiedTrack, TrackId, UserId,
};
use rspotify::{AuthCodeSpotify, ClientError, ClientResult, Config, prelude::*};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::authentication::DEFAULT_WEB_API_CLIENT_ID;
use crate::events::{Event, EventManager};
#[cfg(any(test, feature = "harness"))]
use crate::internal_api::AreaCounts;
use crate::internal_api::collection::{self, Delta, Listing};
use crate::internal_api::error::InternalError;
use crate::internal_api::metadata::Missing;
use crate::internal_api::playlist::{self, Head, PlaylistItem, RootHead, Rootlist};
use crate::internal_api::playlist_write::{self, Created};
use crate::internal_api::{Area, InternalApi, Settings, bridge, fallback, metadata, run_attempts};
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::category::Category;
use crate::model::episode::Episode;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::track::Track;
use crate::playlist_items;
use crate::rate_limit::{
    Attempt, ClientLabel, Stamp, ThrottleState, is_ui_thread, parse_retry_after, random_jitter,
    run_rate_limited,
};
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

/// How long the personal client skips refreshing after a transient failure.
const TRANSIENT_REFRESH_BACKOFF: Duration = Duration::from_secs(30);

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
    /// Set after a transient personal refresh failure: no new refresh starts before this instant.
    refresh_backoff_until: Arc<RwLock<Option<Instant>>>,
    /// Calls made on this client since startup, shared by all clones.
    calls: Arc<AtomicU64>,
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
            refresh_backoff_until: Arc::new(RwLock::new(None)),
            calls: Arc::new(AtomicU64::new(0)),
        }
    }

    fn is_disabled(&self) -> bool {
        self.disabled.load(Ordering::Relaxed)
    }
}

/// Web API calls per client since startup.
#[cfg(any(test, feature = "harness"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct WebCalls {
    pub shared: u64,
    pub personal: u64,
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
    /// Spotify's internal API client, shared by all clones.
    internal: InternalApi,
}

/// `call` once per `size` items of `items`, in order, with the results concatenated; the first
/// failed call fails the whole.
fn in_batches<T: Clone, U, E>(
    items: &[T],
    size: usize,
    mut call: impl FnMut(Vec<T>) -> Result<Vec<U>, E>,
) -> Result<Vec<U>, E> {
    let mut out = Vec::with_capacity(items.len());
    for batch in items.chunks(size) {
        out.extend(call(batch.to_vec())?);
    }
    Ok(out)
}

/// The Spotify URIs among `uris`; the others are dropped with a debug log.
fn parse_uris(uris: &[String]) -> Vec<SpotifyUri> {
    uris.iter()
        .filter_map(|u| match SpotifyUri::from_uri(u) {
            Ok(uri) => Some(uri),
            Err(_) => {
                debug!("library: skipping an unparseable uri");
                None
            }
        })
        .collect()
}

impl WebApi {
    /// Create the Web API wrapper. A personal client is set up only if `cfg` has a personal
    /// client ID.
    pub fn new(cfg: &crate::config::Config) -> Self {
        let personal = cfg
            .personal_client_id()
            .map(|personal| ApiClient::new(&personal.id, ClientLabel::Personal));
        let settings = Settings::from_values(&cfg.values());
        info!("{}", settings.describe());
        Self {
            shared: ApiClient::new(DEFAULT_WEB_API_CLIENT_ID, ClientLabel::Shared),
            personal,
            user: None,
            worker_channel: Arc::new(RwLock::new(None)),
            events: None,
            internal: InternalApi::new(settings),
        }
    }

    /// Set the username for use with the API.
    pub fn set_user(&mut self, user: Option<String>) {
        self.user = user;
    }

    /// The username set with [WebApi::set_user].
    pub fn user_id(&self) -> Option<&str> {
        self.user.as_deref()
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

    /// Web API calls per client since startup.
    #[cfg(any(test, feature = "harness"))]
    pub fn web_calls(&self) -> WebCalls {
        WebCalls {
            shared: self.shared.calls.load(Ordering::Relaxed),
            personal: self
                .personal
                .as_ref()
                .map_or(0, |client| client.calls.load(Ordering::Relaxed)),
        }
    }

    /// Internal API counters of `area` since startup.
    #[cfg(any(test, feature = "harness"))]
    pub fn area_counts(&self, area: Area) -> AreaCounts {
        self.internal.area_counts(area)
    }

    /// Use `session` (filled by the Spotify worker when it connects) for internal API calls.
    pub(crate) fn set_session(&mut self, session: Arc<RwLock<Option<Session>>>) {
        self.internal.set_session(session);
    }

    /// Run one call through the internal API for `area`, falling back to `web` per the rules in
    /// [fallback::run_with_fallback]. `internal` builds the future from a connected session.
    fn with_fallback<T, F, Fut>(
        &self,
        area: Area,
        internal: F,
        web: impl FnOnce() -> Option<T>,
    ) -> Option<T>
    where
        F: Fn(Session) -> Fut,
        Fut: Future<Output = Result<T, InternalError>> + Send + 'static,
        T: Send + 'static,
    {
        let skip = self.internal.skip(area);
        fallback::run_with_fallback(
            skip,
            || self.run_internal(area, internal),
            || {
                if !skip {
                    self.count_fallback(area);
                }
                web()
            },
            |err| self.latch_area(area, err),
        )
    }

    /// Run one call through the internal API for `area` with no Web API fallback, for callers
    /// that choose the fallback themselves. Fails with `ProtocolChanged` when the area is off or
    /// latched.
    pub fn run_internal<T, F, Fut>(&self, area: Area, internal: F) -> Result<T, InternalError>
    where
        F: Fn(Session) -> Fut,
        Fut: Future<Output = Result<T, InternalError>> + Send + 'static,
        T: Send + 'static,
    {
        self.run_internal_with(area, true, internal)
    }

    /// Like [Self::run_internal] but the request is sent at most once on our side: no transient
    /// retry and no waiting for a rate-limit window. For writes, which a retry could apply twice.
    pub fn run_internal_once<T, F, Fut>(&self, area: Area, internal: F) -> Result<T, InternalError>
    where
        F: Fn(Session) -> Fut,
        Fut: Future<Output = Result<T, InternalError>> + Send + 'static,
        T: Send + 'static,
    {
        self.run_internal_with(area, false, internal)
    }

    fn run_internal_with<T, F, Fut>(
        &self,
        area: Area,
        may_retry: bool,
        internal: F,
    ) -> Result<T, InternalError>
    where
        F: Fn(Session) -> Fut,
        Fut: Future<Output = Result<T, InternalError>> + Send + 'static,
        T: Send + 'static,
    {
        if self.internal.skip(area) {
            return Err(InternalError::ProtocolChanged("area off or latched".into()));
        }
        let session = self.internal.session().ok_or(InternalError::NoSession)?;
        // Inside the runtime (MPRIS handlers, spawn_blocking) a sleeping retry would hold
        // a runtime thread, so treat it like the UI thread: no retries, no throttle waits.
        let no_waits =
            !may_retry || is_ui_thread() || tokio::runtime::Handle::try_current().is_ok();
        // Held across retries so backoff sleeps and throttle waits count as loading.
        let _call = ACTIVITY.begin();
        let result = run_attempts(
            no_waits,
            &self.internal.throttle,
            || bridge::run(internal(session.clone()), bridge::BRIDGE_TIMEOUT),
            |window| self.on_rate_limited(ClientLabel::Internal, &self.internal.throttle, window),
            thread::sleep,
        );
        if result.is_ok() {
            self.internal.count_ok(area);
        }
        result
    }

    /// Latch `area` to the Web API after `err`, and tell the UI once.
    pub fn latch_area(&self, area: Area, err: &InternalError) {
        if self.internal.latch(area) {
            // The InternalApiFallback handler logs the warning with the user-facing text.
            // The class only: error texts may carry endpoint paths with the username.
            debug!(
                "internal API {} latched to the Web API ({})",
                area.name(),
                err.class()
            );
            if let Some(events) = &self.events {
                events.try_send(Event::InternalApiFallback {
                    area: area.name(),
                    reason: err.class().to_string(),
                });
            }
        }
    }

    /// Count a call in `area` that fell back to the Web API after an internal failure.
    pub fn count_fallback(&self, area: Area) {
        self.internal.count_fallback(area);
    }

    /// Whether calls in `area` would try the internal API right now (on and not latched).
    pub fn internal_usable(&self, area: Area) -> bool {
        !self.internal.skip(area)
    }

    /// Whether `area` is switched on in the settings, ignoring the latch.
    pub fn internal_enabled(&self, area: Area) -> bool {
        self.internal.settings().area_enabled(area)
    }

    /// Every item of a collection set, page by page (each page is one internal call in
    /// `area`, so a set read for the playlists, such as the pins, follows the Playlists switch
    /// and latch and counts there).
    pub fn collection_page_all(
        &self,
        area: Area,
        set: &'static str,
    ) -> Result<Listing, InternalError> {
        let mut listing = Listing::default();
        let mut token = String::new();
        loop {
            let page = self.run_internal(area, |session| {
                collection::page(session, set, token.clone())
            })?;
            match collection::absorb(&mut listing, page) {
                Some(next) => token = next,
                None => return Ok(listing),
            }
        }
    }

    /// The whole rootlist (playlists and folder markers): one internal call per page.
    pub fn playlist_rootlist(&self) -> Result<Rootlist, InternalError> {
        self.run_internal(Area::Playlists, playlist::rootlist)
    }

    /// The head (revision, length, name) of one playlist, by base62 id, without its items.
    pub fn playlist_head(&self, id: &str) -> Result<Head, InternalError> {
        let id = id.to_string();
        self.run_internal(Area::Playlists, |session| {
            playlist::head(session, id.clone())
        })
    }

    /// Items `from..from + length` of one playlist (their uris) with the head they were read
    /// at; past the end the items are empty.
    pub fn playlist_range(
        &self,
        id: &str,
        from: usize,
        length: usize,
    ) -> Result<(Head, Vec<String>), InternalError> {
        let id = id.to_string();
        self.run_internal(Area::Playlists, |session| {
            playlist::range(session, id.clone(), from, length)
        })
        .map(|(head, items)| (head, items.into_iter().map(|item| item.uri).collect()))
    }

    /// The rootlist revision and length, without entries.
    pub fn rootlist_head(&self) -> Result<RootHead, InternalError> {
        self.run_internal(Area::Playlists, playlist::rootlist_head)
    }

    /// Create a playlist named `name`. Sent once: a retried create would make a second playlist.
    pub fn playlist_create(&self, name: &str) -> Result<Created, InternalError> {
        let name = name.to_string();
        self.run_internal_once(Area::Playlists, |session| {
            playlist_write::create(session, name.clone())
        })
    }

    /// Apply a change set to one playlist, by base62 id. Sent once, never retried.
    pub fn playlist_changes(&self, id: &str, body: ListChanges) -> Result<(), InternalError> {
        let id = id.to_string();
        self.run_internal_once(Area::Playlists, |session| {
            playlist_write::changes(session, id.clone(), body.clone())
        })
    }

    /// Apply a change set to the rootlist. Sent once, never retried.
    pub fn rootlist_changes(&self, body: ListChanges) -> Result<(), InternalError> {
        self.run_internal_once(Area::Playlists, |session| {
            playlist_write::rootlist_changes(session, body.clone())
        })
    }

    /// The items of one playlist, by base62 id, with the head they were read at.
    pub fn playlist_contents(&self, id: &str) -> Result<(Head, Vec<PlaylistItem>), InternalError> {
        let id = id.to_string();
        self.run_internal(Area::Playlists, |session| {
            playlist::contents(session, id.clone())
        })
    }

    /// The items of one playlist as `Playable`s, in playlist order: metadata for URIs not in
    /// `known` comes from the internal API (tracks and episodes in batches), local files are
    /// decoded from their URI, and anything without metadata is an unplayable placeholder.
    pub fn playlist_playables(
        &self,
        id: &str,
        known: &HashMap<String, Playable>,
    ) -> Result<Vec<Playable>, InternalError> {
        self.playlist_playables_at(id, known)
            .map(|(_, playables)| playables)
    }

    /// Like [WebApi::playlist_playables], with the head of the read the items come from (a
    /// write checked against these items must be based on its revision).
    pub fn playlist_playables_at(
        &self,
        id: &str,
        known: &HashMap<String, Playable>,
    ) -> Result<(Head, Vec<Playable>), InternalError> {
        let (head, items) = self.playlist_contents(id)?;
        let wanted = playlist_items::unknown_uris(&items, known);
        let (episodes, tracks): (Vec<String>, Vec<String>) = wanted
            .into_iter()
            .partition(|uri| uri.starts_with("spotify:episode:"));
        let mut fetched = HashMap::new();
        for track in self.tracks_in(Area::Playlists, &tracks)? {
            fetched.insert(track.uri.clone(), Playable::Track(track));
        }
        for episode in self.episodes_in(Area::Playlists, &episodes)? {
            fetched.insert(episode.uri.clone(), Playable::Episode(episode));
        }
        let playables =
            playlist_items::assemble(&items, |uri| fetched.get(uri).or_else(|| known.get(uri)));
        Ok((head, playables))
    }

    pub fn collection_delta(&self, set: &'static str, token: &str) -> Result<Delta, InternalError> {
        let token = token.to_string();
        self.run_internal(Area::Library, |session| {
            collection::delta(session, set, token.clone())
        })
    }

    /// Add or remove `uris` in a collection set (calls count in `area`): one internal call per
    /// [collection::WRITE_BATCH] items. A failed batch stops the write; batches before it stay
    /// written, and the next delta sync brings them into the store.
    pub fn collection_write(
        &self,
        area: Area,
        set: &'static str,
        uris: &[String],
        removed: bool,
    ) -> Result<(), InternalError> {
        Self::write_batches(uris, |batch| {
            self.run_internal(area, |session| {
                collection::write(session, set, batch.to_vec(), removed)
            })
        })
    }

    /// [Self::collection_write] with each call sent once. For a library save: on the write worker
    /// `run_internal` would add its own retries and sleeps, which a quit cannot wake. The save job
    /// retries instead.
    pub fn collection_write_once(
        &self,
        area: Area,
        set: &'static str,
        uris: &[String],
        removed: bool,
    ) -> Result<(), InternalError> {
        Self::write_batches(uris, |batch| {
            self.run_internal_once(area, |session| {
                collection::write(session, set, batch.to_vec(), removed)
            })
        })
    }

    /// Run `write` on each [collection::WRITE_BATCH] of `uris`, stopping at the first failure.
    fn write_batches(
        uris: &[String],
        write: impl Fn(&[String]) -> Result<(), InternalError>,
    ) -> Result<(), InternalError> {
        uris.chunks(collection::WRITE_BATCH).try_for_each(write)
    }

    /// Metadata for library items by Spotify URI, through the internal API: one internal call
    /// (with its own bridge timeout and retries) per [metadata::BATCH_SIZE] URIs, results in
    /// request order. URIs that do not parse are dropped, and so are items the service no
    /// longer has; with nothing left to ask for, no session is needed.
    fn library_items<M, T, Fut>(
        &self,
        area: Area,
        uris: &[String],
        fetch: impl Fn(Session, Vec<SpotifyUri>) -> Fut,
    ) -> Result<Vec<T>, InternalError>
    where
        Fut: Future<Output = Result<Vec<M>, InternalError>> + Send + 'static,
        M: Send + 'static,
        T: for<'a> From<&'a M>,
    {
        if uris.is_empty() {
            return Ok(Vec::new());
        }
        let uris = parse_uris(uris);
        in_batches(&uris, metadata::BATCH_SIZE, |batch| {
            self.run_internal(area, |session| fetch(session, batch.clone()))
                .map(|items| items.iter().map(T::from).collect())
        })
    }

    pub fn library_tracks(&self, uris: &[String]) -> Result<Vec<Track>, InternalError> {
        self.tracks_in(Area::Library, uris)
    }

    fn tracks_in(&self, area: Area, uris: &[String]) -> Result<Vec<Track>, InternalError> {
        self.library_items(area, uris, metadata::tracks_by_uri)
    }

    pub fn library_albums(&self, uris: &[String]) -> Result<Vec<Album>, InternalError> {
        self.library_items(Area::Library, uris, |session, uris| {
            metadata::albums_by_uri(session, uris, Missing::Skip)
        })
    }

    pub fn library_artists(&self, uris: &[String]) -> Result<Vec<Artist>, InternalError> {
        self.library_items(Area::Library, uris, metadata::artists_by_uri)
    }

    pub fn library_shows(
        &self,
        uris: &[String],
    ) -> Result<Vec<crate::model::show::Show>, InternalError> {
        self.library_items(Area::Library, uris, metadata::shows_by_uri)
    }

    pub fn library_episodes(&self, uris: &[String]) -> Result<Vec<Episode>, InternalError> {
        self.episodes_in(Area::Library, uris)
    }

    fn episodes_in(&self, area: Area, uris: &[String]) -> Result<Vec<Episode>, InternalError> {
        self.library_items(area, uris, |session, uris| {
            metadata::episodes_by_uri(session, uris, Missing::Skip)
        })
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
    /// client never does. A permanent failure (see
    /// `crate::authentication::is_permanent_refresh_failure`) disables it for the rest of the
    /// session. A transient failure backs it off for `TRANSIENT_REFRESH_BACKOFF` and retries
    /// afterwards, and the failed call is retried on the shared client in the meantime.
    fn refresh_client(&self, client: &ApiClient, force: bool) -> Option<JoinHandle<()>> {
        if client.is_disabled() {
            return None;
        }
        if client.label == ClientLabel::Personal
            && client
                .refresh_backoff_until
                .read()
                .unwrap()
                .is_some_and(|until| until > Instant::now())
        {
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
        let refresh_backoff_until = client.refresh_backoff_until.clone();
        let events = self.events.clone();
        Some(ASYNC_RUNTIME.get().unwrap().spawn_blocking(move || {
            let result = match label {
                ClientLabel::Shared => crate::authentication::get_rspotify_token(&client_id),
                ClientLabel::Personal => {
                    crate::authentication::refresh_rspotify_token(&client_id, force)
                }
                ClientLabel::Internal => {
                    unreachable!("the internal API has no Web API token")
                }
            };
            match result {
                Ok(token) => {
                    let expires_at = token
                        .expires_at
                        .unwrap_or_else(|| Utc::now() + ChronoDuration::hours(1));
                    *api_token.lock().unwrap() = Some(token);
                    *api_token_expiration.write().unwrap() = expires_at;
                    *refresh_backoff_until.write().unwrap() = None;
                }
                Err(e)
                    if label == ClientLabel::Personal
                        && !crate::authentication::is_permanent_refresh_failure(&e) =>
                {
                    warn!(
                        "Temporary failure refreshing the personal client ID's token, retrying \
                         later: {e}"
                    );
                    *refresh_backoff_until.write().unwrap() =
                        Some(Instant::now() + TRANSIENT_REFRESH_BACKOFF);
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

    /// What is left of the internal API's rate-limit window.
    pub fn internal_window(&self) -> Option<Duration> {
        self.internal
            .throttle
            .read()
            .unwrap()
            .remaining(Stamp::now())
    }

    /// Remaining rate-limit window of each throttled client, personal first.
    pub fn rate_limit_status(&self) -> Vec<(ClientLabel, Duration)> {
        let now = Stamp::now();
        self.personal
            .iter()
            .chain(std::iter::once(&self.shared))
            .map(|client| (client.label, &client.throttle))
            .chain(std::iter::once((
                ClientLabel::Internal,
                &self.internal.throttle,
            )))
            .filter_map(|(label, throttle)| {
                throttle
                    .read()
                    .unwrap()
                    .remaining(now)
                    .map(|remaining| (label, remaining))
            })
            .collect()
    }

    /// Record a 429 on the client `label` (rate-limit state `throttle`) with a window of
    /// `window`. At the start of a throttle episode, keep the status bar countdown redrawing
    /// until the window has passed; tell the UI (log line and notification) at most once per
    /// `NOTIFY_COOLDOWN`, so short backoff windows do not repeat the notification.
    fn on_rate_limited(
        &self,
        label: ClientLabel,
        throttle: &Arc<RwLock<ThrottleState>>,
        window: Duration,
    ) {
        let now = Stamp::now();
        let (new_episode, announce) = {
            let mut throttle = throttle.write().unwrap();
            let new_episode = throttle.record_429(now, window);
            (new_episode, new_episode && throttle.should_announce(now))
        };
        let Some(events) = self.events.clone() else {
            return;
        };
        if announce {
            events.try_send(Event::RateLimited {
                client: label,
                retry_after: window,
            });
        }
        if !new_episode {
            return;
        }

        // The UI refresh tick doesn't run while playback is stopped, so redraw the countdown
        // from here until the window has passed, then once more to clear it.
        let throttle = throttle.clone();
        thread::spawn(move || {
            while throttle.read().unwrap().is_active(Stamp::now()) {
                if !events.try_trigger() {
                    return;
                }
                thread::sleep(Duration::from_secs(1));
            }
            events.try_trigger();
        });
    }

    /// Whether any client got a 429 at or after `since`, even if its window has closed since.
    pub fn rate_limited_since(&self, since: Stamp) -> bool {
        self.personal
            .iter()
            .chain(std::iter::once(&self.shared))
            .any(|client| client.throttle.read().unwrap().rate_limited_since(since))
            || self
                .internal
                .throttle
                .read()
                .unwrap()
                .rate_limited_since(since)
    }

    /// Send `api_call` once on `client` and classify the result for [run_rate_limited].
    fn attempt<F, R>(client: &ApiClient, api_call: &F) -> Attempt<R>
    where
        F: Fn(&AuthCodeSpotify) -> ClientResult<R>,
    {
        match api_call(&client.api) {
            Ok(v) => Attempt::Done(v),
            Err(ClientError::Http(error)) => {
                debug!("http error: {error:?}");
                match error.as_ref() {
                    HttpError::StatusCode(response) if response.status() == 429 => {
                        let header = response.header("Retry-After");
                        warn!(
                            "rate limited on the {} client ID at {}, Retry-After: {header:?}",
                            client.label,
                            response.get_url()
                        );
                        Attempt::RateLimited(parse_retry_after(header))
                    }
                    HttpError::StatusCode(response) => Attempt::Failed(Some(response.status())),
                    _ => Attempt::Failed(None),
                }
            }
            Err(e) => {
                error!("unhandled api error: {e}");
                Attempt::Failed(None)
            }
        }
    }

    /// Execute `api_call` on `client` with bounded rate-limit handling (see [run_rate_limited]):
    /// the UI thread never waits on a rate limit, background threads wait through short
    /// windows. A 401 on the shared client refreshes and retries once, through the same rate
    /// limit checks; a 401 on the personal client starts a forced background refresh and fails,
    /// so the caller can retry on the shared client. On failure, returns the HTTP status if the
    /// server sent one.
    fn call_on<F, R>(&self, client: &ApiClient, api_call: &F) -> Result<R, Option<u16>>
    where
        F: Fn(&AuthCodeSpotify) -> ClientResult<R>,
    {
        // One guard for the whole call: rate-limit waits, the 401 refresh loop and the retry
        // all count as loading.
        let _call = ACTIVITY.begin();
        client.calls.fetch_add(1, Ordering::Relaxed);
        let mut refreshed = false;
        loop {
            let result = run_rate_limited(
                client.label,
                &client.throttle,
                is_ui_thread(),
                random_jitter,
                || Self::attempt(client, api_call),
                |window| self.on_rate_limited(client.label, &client.throttle, window),
            );
            match result {
                Err(Some(401)) if client.label == ClientLabel::Personal => {
                    debug!("personal token unauthorized, refreshing in the background");
                    self.refresh_client(client, true);
                    return Err(Some(401));
                }
                Err(Some(401)) if !refreshed => {
                    debug!("token unauthorized. trying refresh..");
                    if self.refresh_client(client, false).is_none() {
                        return Err(None);
                    }
                    refreshed = true;
                }
                other => return other,
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
            Err(Some(429)) => {
                warn!("{} client ID is rate limited, call failed", client.label);
                None
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

    /// Move the item at `from` to before `insert_before` (an index before the move), against
    /// `snapshot_id`: Spotify applies the move to that version or refuses it. Returns the new
    /// snapshot id.
    pub fn reorder_playlist_item(
        &self,
        playlist_id: &str,
        from: usize,
        insert_before: usize,
        snapshot_id: &str,
    ) -> Result<String, ()> {
        let from = i32::try_from(from).map_err(|_| ())?;
        let insert_before = i32::try_from(insert_before).map_err(|_| ())?;
        self.api_with_retry(Route::Bulk, |api| {
            api.playlist_reorder_items(
                PlaylistId::from_id(playlist_id).unwrap(),
                Some(from),
                Some(insert_before),
                Some(1),
                Some(snapshot_id),
            )
        })
        .map(|result| result.snapshot_id)
        .ok_or(())
    }

    /// Set the name of the playlist with `playlist_id`.
    pub fn rename_playlist(&self, playlist_id: &str, name: &str) -> Result<(), ()> {
        self.api_with_retry(Route::Bulk, |api| {
            api.playlist_change_detail(
                PlaylistId::from_id(playlist_id).unwrap(),
                Some(name),
                None,
                None,
                None,
            )
        })
        .map(|_| ())
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
        let id = album_id.to_string();
        self.with_fallback(
            Area::Metadata,
            move |session| metadata::album(session, id.clone()),
            || {
                self.api_with_retry(Route::Bulk, |api| {
                    api.album(aid.clone(), Some(Market::FromToken))
                })
            },
        )
        .ok_or(())
    }

    /// Fetch the artist with the given `artist_id`.
    pub fn artist(&self, artist_id: &str) -> Result<FullArtist, ()> {
        let aid = ArtistId::from_id(artist_id).map_err(|_| ())?;
        let id = artist_id.to_string();
        self.with_fallback(
            Area::Metadata,
            move |session| metadata::artist(session, id.clone()),
            || self.api_with_retry(Route::Bulk, |api| api.artist(aid.clone())),
        )
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
        let id = track_id.to_string();
        self.with_fallback(
            Area::Metadata,
            move |session| metadata::track(session, id.clone()),
            || {
                self.api_with_retry(Route::Bulk, |api| {
                    api.track(tid.clone(), Some(Market::FromToken))
                })
            },
        )
        .ok_or(())
    }

    /// Fetch the show with the given `show_id`.
    pub fn show(&self, show_id: &str) -> Result<FullShow, ()> {
        let sid = ShowId::from_id(show_id).map_err(|_| ())?;
        let id = show_id.to_string();
        self.with_fallback(
            Area::Metadata,
            move |session| metadata::show(session, id.clone()),
            || {
                self.api_with_retry(Route::Bulk, |api| {
                    api.get_a_show(sid.clone(), Some(Market::FromToken))
                })
            },
        )
        .ok_or(())
    }

    /// Fetch the episode with the given `episode_id`.
    pub fn episode(&self, episode_id: &str) -> Result<FullEpisode, ()> {
        let eid = EpisodeId::from_id(episode_id).map_err(|_| ())?;
        let id = episode_id.to_string();
        self.with_fallback(
            Area::Metadata,
            move |session| metadata::episode(session, id.clone()),
            || {
                self.api_with_retry(Route::Bulk, |api| {
                    api.get_an_episode(eid.clone(), Some(Market::FromToken))
                })
            },
        )
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
        let id = album_id.to_string();
        self.with_fallback(
            Area::Metadata,
            move |session| metadata::album_tracks(session, id.clone(), limit, offset),
            || {
                self.api_with_retry(Route::Bulk, |api| {
                    api.album_track_manual(
                        AlbumId::from_id(album_id).unwrap(),
                        Some(Market::FromToken),
                        Some(limit),
                        Some(offset),
                    )
                })
            },
        )
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
        // Album ids of the artist, fetched once from the internal API and paged locally.
        let ids: Arc<Mutex<Option<Vec<SpotifyUri>>>> = Default::default();
        let fetch_page = move |offset: u32| {
            debug!("fetching artist {artist_id} albums, offset: {offset}");
            let to_page = |items: Vec<SimplifiedAlbum>, total: u32| {
                let mut albums: Vec<Album> = items.iter().map(|sa| sa.into()).collect();
                albums.sort_by(|a, b| b.year.cmp(&a.year));
                ApiPage {
                    offset,
                    total,
                    items: albums,
                }
            };
            let (artist, ids_cache) = (artist_id.clone(), ids.clone());
            spotify
                .with_fallback(
                    Area::Metadata,
                    move |session| {
                        let (artist, ids_cache) = (artist.clone(), ids_cache.clone());
                        async move {
                            // Clone out of the lock so no guard is held across an await.
                            let cached = ids_cache.lock().unwrap().clone();
                            let all = match cached {
                                Some(all) => all,
                                None => {
                                    let all = metadata::artist_album_ids(
                                        session.clone(),
                                        artist,
                                        album_type,
                                    )
                                    .await?;
                                    *ids_cache.lock().unwrap() = Some(all.clone());
                                    all
                                }
                            };
                            let page_ids =
                                metadata::window(&all, offset as usize, MAX_SIZE as usize);
                            let items =
                                metadata::albums_by_uri(session, page_ids, Missing::Fail).await?;
                            Ok((items, all.len() as u32))
                        }
                    },
                    || {
                        spotify
                            .api_with_retry(Route::Bulk, |api| {
                                api.artist_albums_manual(
                                    ArtistId::from_id(&artist_id).unwrap(),
                                    album_type.as_ref().copied(),
                                    Some(Market::FromToken),
                                    Some(MAX_SIZE),
                                    Some(offset),
                                )
                            })
                            .map(|page| (page.items, page.total))
                    },
                )
                .map(|(items, total)| to_page(items, total))
        };

        ApiResult::new(MAX_SIZE, Arc::new(fetch_page))
    }

    /// Get all the episodes of the show with the given `show_id`.
    pub fn show_episodes(&self, show_id: &str) -> ApiResult<Episode> {
        const MAX_SIZE: u32 = 50;
        let spotify = self.clone();
        let show_id = show_id.to_string();
        // Episode ids of the show, fetched once from the internal API and paged locally.
        let ids: Arc<Mutex<Option<Vec<SpotifyUri>>>> = Default::default();
        let fetch_page = move |offset: u32| {
            debug!("fetching show {} episodes, offset: {}", show_id, offset);
            let to_page = |items: Vec<SimplifiedEpisode>, total: u32| ApiPage {
                offset,
                total,
                items: items.iter().map(|se| se.into()).collect(),
            };
            let (show, ids_cache) = (show_id.clone(), ids.clone());
            spotify
                .with_fallback(
                    Area::Metadata,
                    move |session| {
                        let (show, ids_cache) = (show.clone(), ids_cache.clone());
                        async move {
                            // Clone out of the lock so no guard is held across an await.
                            let cached = ids_cache.lock().unwrap().clone();
                            let all = match cached {
                                Some(all) => all,
                                None => {
                                    let all =
                                        metadata::show_episode_ids(session.clone(), show).await?;
                                    *ids_cache.lock().unwrap() = Some(all.clone());
                                    all
                                }
                            };
                            let page_ids =
                                metadata::window(&all, offset as usize, MAX_SIZE as usize);
                            let items =
                                metadata::episodes_by_uri(session, page_ids, Missing::Fail).await?;
                            Ok((items, all.len() as u32))
                        }
                    },
                    || {
                        spotify
                            .api_with_retry(Route::Bulk, |api| {
                                api.get_shows_episodes_manual(
                                    ShowId::from_id(&show_id).unwrap(),
                                    Some(Market::FromToken),
                                    Some(50),
                                    Some(offset),
                                )
                            })
                            .map(|page| (page.items, page.total))
                    },
                )
                .map(|(items, total)| to_page(items, total))
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
        let artist = id.to_string();
        #[allow(deprecated)]
        self.with_fallback(
            Area::Metadata,
            move |session| metadata::artist_top_tracks(session, artist.clone()),
            || {
                self.api_with_retry(Route::Restricted, |api| {
                    api.artist_top_tracks(ArtistId::from_id(id).unwrap(), Some(Market::FromToken))
                })
            },
        )
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
        self.with_fallback(Area::Metadata, metadata::current_user, || {
            self.api_with_retry(Route::Bulk, |api| api.current_user())
        })
        .ok_or(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::Config;

    const PERSONAL_ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn collection_write_once_needs_a_session_unless_empty() {
        let api = WebApi::new(&Config::new_for_test());
        let uris = vec!["spotify:track:4uLU6hMCjMI75M1A2tKUQC".to_string()];
        assert_eq!(
            api.collection_write_once(Area::Library, "collection", &uris, false),
            Err(InternalError::NoSession)
        );
        assert_eq!(
            api.collection_write_once(Area::Library, "collection", &[], true),
            Ok(())
        );
    }

    #[test]
    fn collection_write_needs_a_session_unless_empty() {
        let api = WebApi::new(&Config::new_for_test());
        let uris = vec!["spotify:track:4uLU6hMCjMI75M1A2tKUQC".to_string()];
        assert_eq!(
            api.collection_write(Area::Library, "collection", &uris, false),
            Err(InternalError::NoSession)
        );
        assert_eq!(
            api.collection_write(Area::Library, "collection", &[], true),
            Ok(())
        );
    }

    #[test]
    fn playlist_metadata_is_fetched_under_the_playlists_area() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        api.latch_area(Area::Library, &InternalError::ProtocolChanged("x".into()));
        let uris = vec!["spotify:track:4uLU6hMCjMI75M1A2tKUQC".to_string()];
        assert!(matches!(
            api.library_tracks(&uris),
            Err(InternalError::ProtocolChanged(_))
        ));
        assert!(matches!(
            api.tracks_in(Area::Playlists, &uris),
            Err(InternalError::NoSession)
        ));
        let episodes = vec!["spotify:episode:4uLU6hMCjMI75M1A2tKUQC".to_string()];
        assert!(matches!(
            api.episodes_in(Area::Playlists, &episodes),
            Err(InternalError::NoSession)
        ));
    }

    #[test]
    fn pins_are_read_under_the_playlists_area() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        api.latch_area(Area::Library, &InternalError::ProtocolChanged("x".into()));
        assert!(matches!(
            api.collection_page_all(Area::Library, "ylpin"),
            Err(InternalError::ProtocolChanged(_))
        ));
        assert!(matches!(
            api.collection_page_all(Area::Playlists, "ylpin"),
            Err(InternalError::NoSession)
        ));
    }

    #[test]
    fn playlist_playables_without_a_session_is_no_session() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        assert!(matches!(
            api.playlist_playables("37i9dQZF1DXcBWIGoYBM5M", &HashMap::new()),
            Err(InternalError::NoSession)
        ));
    }

    #[test]
    fn playlist_writes_without_a_session_are_no_session() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        assert_eq!(api.playlist_create("x"), Err(InternalError::NoSession));
        let body = playlist_write::list_changes(Vec::new(), Vec::new());
        assert_eq!(
            api.playlist_changes("37i9dQZF1DXcBWIGoYBM5M", body.clone()),
            Err(InternalError::NoSession)
        );
        assert_eq!(api.rootlist_changes(body), Err(InternalError::NoSession));
    }

    #[test]
    fn run_internal_without_a_session_is_no_session() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        let result: Result<u8, _> = api.run_internal(Area::Library, |_session| async { Ok(1u8) });
        assert_eq!(result, Err(InternalError::NoSession));
    }

    #[test]
    fn library_metadata_without_a_session_is_no_session() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        let uris = vec!["spotify:track:4uLU6hMCjMI75M1A2tKUQC".to_string()];
        assert!(matches!(
            api.library_tracks(&uris),
            Err(InternalError::NoSession)
        ));
        assert_eq!(api.library_episodes(&[]).map(|v| v.len()), Ok(0));
    }

    #[test]
    fn run_internal_on_a_latched_area_does_not_run() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        api.latch_area(Area::Library, &InternalError::ProtocolChanged("x".into()));
        let result: Result<u8, _> = api.run_internal(Area::Library, |_session| async { Ok(1u8) });
        assert!(matches!(result, Err(InternalError::ProtocolChanged(_))));
        assert!(!api.internal_usable(Area::Library));
        assert!(api.internal_enabled(Area::Library));
    }

    #[test]
    fn call_on_counts_the_call_as_in_flight() {
        let api = WebApi::new(&Config::new_for_test());
        let seen = api
            .call_on(&api.shared, &|_api| {
                Ok(crate::activity::ACTIVITY.in_flight())
            })
            .unwrap();
        assert!(seen >= 1, "the Web API call itself is in flight");
    }

    #[test]
    fn refresh_client_waits_out_the_backoff() {
        let api = WebApi::new(&Config::new_for_test_with_client_id(PERSONAL_ID));
        let personal = api.personal.as_ref().unwrap();
        *personal.refresh_backoff_until.write().unwrap() =
            Some(Instant::now() + Duration::from_secs(60));
        assert!(api.refresh_client(personal, true).is_none());
    }

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
        let now = Stamp::now();
        api.shared
            .throttle
            .write()
            .unwrap()
            .record_429(now, Duration::from_secs(60));
        api.personal
            .as_ref()
            .unwrap()
            .throttle
            .write()
            .unwrap()
            .record_429(now, Duration::from_secs(30));
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
        let client = api.client(Route::Bulk);
        api.on_rate_limited(client.label, &client.throttle, Duration::from_secs(30));
        let status = api.rate_limit_status();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].0, ClientLabel::Personal);
    }

    #[test]
    fn rate_limited_since_sees_a_429_on_any_client() {
        let api = WebApi::new(&Config::new_for_test_with_client_id(PERSONAL_ID));
        let start = Stamp::now();
        assert!(!api.rate_limited_since(start));
        api.on_rate_limited(
            api.shared.label,
            &api.shared.throttle,
            Duration::from_millis(1),
        );
        assert!(api.rate_limited_since(start));
        assert!(!api.rate_limited_since(Stamp::now() + Duration::from_secs(1)));
    }

    #[test]
    fn with_fallback_without_a_session_uses_the_web_api_once() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        let out = api.with_fallback(
            Area::Metadata,
            |_session| async { Ok::<u8, InternalError>(1) },
            || Some(2),
        );
        assert_eq!(out, Some(2));
        assert!(
            !api.internal.skip(Area::Metadata),
            "no session must not latch"
        );
        assert_eq!(
            api.area_counts(Area::Metadata),
            AreaCounts {
                internal_ok: 0,
                fallback: 1,
                latched: false
            }
        );
    }

    #[test]
    fn with_fallback_latches_once_on_a_protocol_change() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let rt = ASYNC_RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
        });
        // An unconnected session is enough: the internal closure below never uses it.
        let session = {
            let _guard = rt.enter();
            Session::new(librespot_core::SessionConfig::default(), None)
        };
        let mut api = WebApi::new(&crate::config::Config::new_for_test());
        api.set_session(Arc::new(RwLock::new(Some(session))));
        let internal_calls = Arc::new(AtomicUsize::new(0));
        let call = || {
            let calls = internal_calls.clone();
            api.with_fallback(
                Area::Metadata,
                move |_session| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Err::<u8, _>(InternalError::ProtocolChanged("412".into())) }
                },
                || Some(2),
            )
        };
        assert_eq!(call(), Some(2));
        assert_eq!(internal_calls.load(Ordering::SeqCst), 1);
        assert!(api.internal.skip(Area::Metadata), "protocol change latches");
        assert_eq!(call(), Some(2));
        assert_eq!(
            internal_calls.load(Ordering::SeqCst),
            1,
            "a latched area skips the internal API"
        );
        assert!(!api.internal.latch(Area::Metadata), "latched exactly once");
        assert_eq!(
            api.area_counts(Area::Metadata),
            AreaCounts {
                internal_ok: 0,
                fallback: 1,
                latched: true
            },
            "the skipped second call is not a fallback"
        );
    }

    #[test]
    fn with_fallback_with_internal_api_off_never_tries_internal() {
        let values = crate::config::ConfigValues {
            internal_api: Some(false),
            ..Default::default()
        };
        let api = WebApi::new(&crate::config::Config::new_for_test_with_values(values));
        assert!(api.internal.skip(Area::Metadata));
        assert_eq!(
            api.with_fallback(
                Area::Metadata,
                |_s| async { Ok::<u8, InternalError>(1) },
                || Some(2)
            ),
            Some(2)
        );
        assert_eq!(api.area_counts(Area::Metadata), AreaCounts::default());
    }

    #[test]
    fn with_fallback_counts_an_internal_success() {
        let rt = ASYNC_RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
        });
        let session = {
            let _guard = rt.enter();
            Session::new(librespot_core::SessionConfig::default(), None)
        };
        let mut api = WebApi::new(&crate::config::Config::new_for_test());
        api.set_session(Arc::new(RwLock::new(Some(session))));
        let out = api.with_fallback(
            Area::Metadata,
            |_session| async { Ok::<u8, InternalError>(1) },
            || Some(2),
        );
        assert_eq!(out, Some(1));
        assert_eq!(
            api.area_counts(Area::Metadata),
            AreaCounts {
                internal_ok: 1,
                fallback: 0,
                latched: false
            }
        );
    }

    #[test]
    fn call_on_counts_calls_per_client() {
        let api = WebApi::new(&Config::new_for_test_with_client_id(PERSONAL_ID));
        api.call_on(&api.shared, &|_api| Ok(())).unwrap();
        let personal = api.personal.as_ref().unwrap();
        api.call_on(personal, &|_api| Ok(())).unwrap();
        api.call_on(personal, &|_api| Ok(())).unwrap();
        assert_eq!(
            api.web_calls(),
            WebCalls {
                shared: 1,
                personal: 2
            }
        );
    }

    #[test]
    fn in_batches_makes_one_call_per_batch_and_keeps_the_order() {
        let items: Vec<u32> = (0..250).collect();
        let mut sizes = Vec::new();
        let got: Result<Vec<u32>, ()> = super::in_batches(&items, 100, |batch| {
            sizes.push(batch.len());
            Ok(batch.into_iter().map(|i| i * 2).collect())
        });
        assert_eq!(sizes, vec![100, 100, 50]);
        assert_eq!(
            got.unwrap(),
            items.iter().map(|i| i * 2).collect::<Vec<_>>()
        );
    }

    #[test]
    fn in_batches_stops_at_the_first_failed_batch() {
        let items: Vec<u32> = (0..250).collect();
        let mut calls = 0;
        let got: Result<Vec<u32>, &str> = super::in_batches(&items, 100, |batch| {
            calls += 1;
            if batch[0] == 100 {
                Err("boom")
            } else {
                Ok(batch)
            }
        });
        assert_eq!(got, Err("boom"));
        assert_eq!(calls, 2);
    }

    #[test]
    fn in_batches_of_nothing_makes_no_call() {
        let got: Result<Vec<u32>, ()> =
            super::in_batches(&[] as &[u32], 100, |_| panic!("no call expected"));
        assert_eq!(got, Ok(Vec::new()));
    }

    #[test]
    fn library_metadata_of_only_unparseable_uris_needs_no_session() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        assert_eq!(
            api.library_tracks(&["not a uri".into()]).map(|t| t.len()),
            Ok(0)
        );
        assert_eq!(
            api.library_tracks(&["spotify:track:4uLU6hMCjMI75M1A2tKUQC".into()])
                .map(|t| t.len()),
            Err(InternalError::NoSession)
        );
    }
}
