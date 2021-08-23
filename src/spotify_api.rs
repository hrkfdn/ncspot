use crate::album::Album;
use crate::artist::Artist;
use crate::episode::Episode;
use crate::playable::Playable;
use crate::playlist::Playlist;
use crate::spotify_worker::WorkerCommand;
use crate::track::Track;
use crate::ui::pagination::{ApiPage, ApiResult};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use failure::Error;
use futures::channel::oneshot;
use log::{debug, error, info};
use rspotify::blocking::client::ApiError;
use rspotify::blocking::client::Spotify as SpotifyAPI;
use rspotify::model::album::{FullAlbum, SavedAlbum};
use rspotify::model::artist::FullArtist;
use rspotify::model::page::{CursorBasedPage, Page};
use rspotify::model::playlist::FullPlaylist;
use rspotify::model::recommend::Recommendations;
use rspotify::model::search::SearchResult;
use rspotify::model::show::{FullEpisode, FullShow, Show};
use rspotify::model::track::{FullTrack, SavedTrack, SimplifiedTrack};
use rspotify::model::user::PrivateUser;
use rspotify::senum::{AlbumType, Country, SearchType};
use serde_json::{json, Map};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct WebApi {
    api: Arc<RwLock<SpotifyAPI>>,
    user: Option<String>,
    country: Option<Country>,
    worker_channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
    token_expiration: Arc<RwLock<DateTime<Utc>>>,
}

impl WebApi {
    pub fn new() -> WebApi {
        WebApi {
            api: Arc::new(RwLock::new(SpotifyAPI::default())),
            user: None,
            country: None,
            worker_channel: Arc::new(RwLock::new(None)),
            token_expiration: Arc::new(RwLock::new(Utc::now())),
        }
    }

    pub fn set_user(&mut self, user: Option<String>) {
        self.user = user;
    }

    pub fn set_country(&mut self, country: Option<Country>) {
        self.country = country;
    }

    pub(crate) fn set_worker_channel(
        &mut self,
        channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
    ) {
        self.worker_channel = channel;
    }

    pub fn update_token(&self) {
        {
            let token_expiration = self.token_expiration.read().unwrap();
            let now = Utc::now();
            let delta = *token_expiration - now;

            // token is valid for 5 more minutes, renewal is not necessary yet
            if delta.num_seconds() > 60 * 5 {
                return;
            }

            info!("Token will expire in {}, renewing", delta);
        }

        let (token_tx, token_rx) = oneshot::channel();
        let cmd = WorkerCommand::RequestToken(token_tx);
        if let Some(channel) = self
            .worker_channel
            .read()
            .expect("can't readlock worker channel")
            .as_ref()
        {
            channel.send(cmd).expect("can't send message to worker");
            let token = futures::executor::block_on(token_rx).unwrap();
            self.api.write().expect("can't writelock api").access_token =
                Some(token.access_token.to_string());
            *self
                .token_expiration
                .write()
                .expect("could not writelock token") =
                Utc::now() + ChronoDuration::seconds(token.expires_in.into());
        } else {
            error!("worker channel is not set");
        }
    }

    /// retries once when rate limits are hit
    fn api_with_retry<F, R>(&self, cb: F) -> Option<R>
    where
        F: Fn(&SpotifyAPI) -> Result<R, Error>,
    {
        let result = {
            let api = self.api.read().expect("can't read api");
            cb(&api)
        };
        match result {
            Ok(v) => Some(v),
            Err(e) => {
                debug!("api error: {:?}", e);
                if let Ok(apierror) = e.downcast::<ApiError>() {
                    match apierror {
                        ApiError::RateLimited(d) => {
                            debug!("rate limit hit. waiting {:?} seconds", d);
                            thread::sleep(Duration::from_secs(d.unwrap_or(0) as u64));
                            let api = self.api.read().expect("can't read api");
                            cb(&api).ok()
                        }
                        ApiError::Unauthorized => {
                            debug!("token unauthorized. trying refresh..");
                            self.update_token();
                            let api = self.api.read().expect("can't read api");
                            cb(&api).ok()
                        }
                        e => {
                            error!("unhandled api error: {}", e);
                            None
                        }
                    }
                } else {
                    None
                }
            }
        }
    }

    pub fn append_tracks(
        &self,
        playlist_id: &str,
        tracks: &[String],
        position: Option<i32>,
    ) -> bool {
        self.api_with_retry(|api| {
            api.user_playlist_add_tracks(self.user.as_ref().unwrap(), playlist_id, tracks, position)
        })
        .is_some()
    }

    pub fn delete_tracks(
        &self,
        playlist_id: &str,
        snapshot_id: &str,
        track_pos_pairs: &[(&Track, usize)],
    ) -> bool {
        let mut tracks = Vec::new();
        for (track, pos) in track_pos_pairs {
            let track_occurrence = json!({
                "uri": format!("spotify:track:{}", track.id.clone().unwrap()),
                "positions": [pos]
            });
            let track_occurrence_object = track_occurrence.as_object();
            tracks.push(track_occurrence_object.unwrap().clone());
        }
        self.api_with_retry(|api| {
            api.user_playlist_remove_specific_occurrenes_of_tracks(
                self.user.as_ref().unwrap(),
                playlist_id,
                tracks.clone(),
                Some(snapshot_id.to_string()),
            )
        })
        .is_some()
    }

    pub fn overwrite_playlist(&self, id: &str, tracks: &[Playable]) {
        // extract only track IDs
        let mut tracks: Vec<String> = tracks.iter().filter_map(|track| track.id()).collect();

        // we can only send 100 tracks per request
        let mut remainder = if tracks.len() > 100 {
            Some(tracks.split_off(100))
        } else {
            None
        };

        if let Some(()) = self.api_with_retry(|api| {
            api.user_playlist_replace_tracks(self.user.as_ref().unwrap(), id, &tracks)
        }) {
            debug!("saved {} tracks to playlist {}", tracks.len(), id);
            while let Some(ref mut tracks) = remainder.clone() {
                // grab the next set of 100 tracks
                remainder = if tracks.len() > 100 {
                    Some(tracks.split_off(100))
                } else {
                    None
                };

                debug!("adding another {} tracks to playlist", tracks.len());
                if self.append_tracks(id, tracks, None) {
                    debug!("{} tracks successfully added", tracks.len());
                } else {
                    error!("error saving tracks to playlists {}", id);
                    return;
                }
            }
        } else {
            error!("error saving tracks to playlist {}", id);
        }
    }

    pub fn delete_playlist(&self, id: &str) -> bool {
        self.api_with_retry(|api| api.user_playlist_unfollow(self.user.as_ref().unwrap(), id))
            .is_some()
    }

    pub fn create_playlist(
        &self,
        name: &str,
        public: Option<bool>,
        description: Option<String>,
    ) -> Option<String> {
        let result = self.api_with_retry(|api| {
            api.user_playlist_create(
                self.user.as_ref().unwrap(),
                name,
                public,
                description.clone(),
            )
        });
        result.map(|r| r.id)
    }

    pub fn album(&self, album_id: &str) -> Option<FullAlbum> {
        self.api_with_retry(|api| api.album(album_id))
    }

    pub fn artist(&self, artist_id: &str) -> Option<FullArtist> {
        self.api_with_retry(|api| api.artist(artist_id))
    }

    pub fn playlist(&self, playlist_id: &str) -> Option<FullPlaylist> {
        self.api_with_retry(|api| api.playlist(playlist_id, None, self.country))
    }

    pub fn track(&self, track_id: &str) -> Option<FullTrack> {
        self.api_with_retry(|api| api.track(track_id))
    }

    pub fn get_show(&self, show_id: &str) -> Option<FullShow> {
        self.api_with_retry(|api| api.get_a_show(show_id.to_string(), self.country))
    }

    pub fn episode(&self, episode_id: &str) -> Option<FullEpisode> {
        self.api_with_retry(|api| api.get_an_episode(episode_id.to_string(), self.country))
    }

    pub fn recommendations(
        &self,
        seed_artists: Option<Vec<String>>,
        seed_genres: Option<Vec<String>>,
        seed_tracks: Option<Vec<String>>,
    ) -> Option<Recommendations> {
        self.api_with_retry(|api| {
            api.recommendations(
                seed_artists.clone(),
                seed_genres.clone(),
                seed_tracks.clone(),
                100,
                self.country,
                &Map::new(),
            )
        })
    }

    pub fn search(
        &self,
        searchtype: SearchType,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Option<SearchResult> {
        self.api_with_retry(|api| api.search(query, searchtype, limit, offset, self.country, None))
            .take()
    }

    pub fn current_user_playlist(&self) -> ApiResult<Playlist> {
        const MAX_LIMIT: u32 = 50;
        let spotify = self.clone();
        let fetch_page = move |offset: u32| {
            debug!("fetching user playlists, offset: {}", offset);
            spotify.api_with_retry(|api| match api.current_user_playlists(MAX_LIMIT, offset) {
                Ok(page) => Ok(ApiPage {
                    offset: page.offset,
                    total: page.total,
                    items: page.items.iter().map(|sp| sp.into()).collect(),
                }),
                Err(e) => Err(e),
            })
        };
        ApiResult::new(MAX_LIMIT, Arc::new(fetch_page))
    }

    pub fn user_playlist_tracks(&self, playlist_id: &str) -> ApiResult<Track> {
        const MAX_LIMIT: u32 = 100;
        let spotify = self.clone();
        let playlist_id = playlist_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!(
                "fetching playlist {} tracks, offset: {}",
                playlist_id, offset
            );
            spotify.api_with_retry(|api| {
                match api.user_playlist_tracks(
                    spotify.user.as_ref().unwrap(),
                    &playlist_id,
                    None,
                    MAX_LIMIT,
                    offset,
                    spotify.country,
                ) {
                    Ok(page) => Ok(ApiPage {
                        offset: page.offset,
                        total: page.total,
                        items: page
                            .items
                            .iter()
                            .enumerate()
                            .flat_map(|(index, pt)| {
                                pt.track.as_ref().map(|t| {
                                    let mut track: Track = t.into();
                                    track.added_at = Some(pt.added_at);
                                    track.list_index = page.offset as usize + index;
                                    track
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

    pub fn full_album(&self, album_id: &str) -> Option<FullAlbum> {
        self.api_with_retry(|api| api.album(album_id))
    }

    pub fn album_tracks(
        &self,
        album_id: &str,
        limit: u32,
        offset: u32,
    ) -> Option<Page<SimplifiedTrack>> {
        self.api_with_retry(|api| api.album_track(album_id, limit, offset))
    }

    pub fn artist_albums(
        &self,
        artist_id: &str,
        album_type: Option<AlbumType>,
    ) -> ApiResult<Album> {
        const MAX_SIZE: u32 = 50;
        let spotify = self.clone();
        let artist_id = artist_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!("fetching artist {} albums, offset: {}", artist_id, offset);
            spotify.api_with_retry(|api| {
                match api.artist_albums(
                    &artist_id,
                    album_type,
                    spotify.country,
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

    pub fn show_episodes(&self, show_id: &str) -> ApiResult<Episode> {
        const MAX_SIZE: u32 = 50;
        let spotify = self.clone();
        let show_id = show_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!("fetching show {} episodes, offset: {}", &show_id, offset);
            spotify.api_with_retry(|api| {
                match api.get_shows_episodes(show_id.clone(), MAX_SIZE, offset, spotify.country) {
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

    pub fn get_saved_shows(&self, offset: u32) -> Option<Page<Show>> {
        self.api_with_retry(|api| api.get_saved_show(50, offset))
    }

    pub fn save_shows(&self, ids: Vec<String>) -> bool {
        self.api_with_retry(|api| api.save_shows(ids.clone()))
            .is_some()
    }

    pub fn unsave_shows(&self, ids: Vec<String>) -> bool {
        self.api_with_retry(|api| api.remove_users_saved_shows(ids.clone(), self.country))
            .is_some()
    }

    pub fn current_user_followed_artists(
        &self,
        last: Option<String>,
    ) -> Option<CursorBasedPage<FullArtist>> {
        self.api_with_retry(|api| api.current_user_followed_artists(50, last.clone()))
            .map(|cp| cp.artists)
    }

    pub fn user_follow_artists(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.user_follow_artists(&ids))
    }

    pub fn user_unfollow_artists(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.user_unfollow_artists(&ids))
    }

    pub fn current_user_saved_albums(&self, offset: u32) -> Option<Page<SavedAlbum>> {
        self.api_with_retry(|api| api.current_user_saved_albums(50, offset))
    }

    pub fn current_user_saved_albums_add(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.current_user_saved_albums_add(&ids))
    }

    pub fn current_user_saved_albums_delete(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.current_user_saved_albums_delete(&ids))
    }

    pub fn current_user_saved_tracks(&self, offset: u32) -> Option<Page<SavedTrack>> {
        self.api_with_retry(|api| api.current_user_saved_tracks(50, offset))
    }

    pub fn current_user_saved_tracks_add(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.current_user_saved_tracks_add(&ids))
    }

    pub fn current_user_saved_tracks_delete(&self, ids: Vec<String>) -> Option<()> {
        self.api_with_retry(|api| api.current_user_saved_tracks_delete(&ids))
    }

    pub fn user_playlist_follow_playlist(&self, owner_id: String, id: String) -> Option<()> {
        self.api_with_retry(|api| api.user_playlist_follow_playlist(&owner_id, &id, true))
    }

    pub fn artist_top_tracks(&self, id: &str) -> Option<Vec<Track>> {
        self.api_with_retry(|api| api.artist_top_tracks(id, self.country))
            .map(|ft| ft.tracks.iter().map(|t| t.into()).collect())
    }

    pub fn artist_related_artists(&self, id: String) -> Option<Vec<Artist>> {
        self.api_with_retry(|api| api.artist_related_artists(&id))
            .map(|fa| fa.artists.iter().map(|a| a.into()).collect())
    }

    pub fn current_user(&self) -> Option<PrivateUser> {
        self.api_with_retry(|api| api.current_user())
    }
}
