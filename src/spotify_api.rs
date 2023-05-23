use crate::application::ASYNC_RUNTIME;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::category::Category;
use crate::model::episode::Episode;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::track::Track;
use crate::spotify_worker::WorkerCommand;
use crate::ui::pagination::{ApiPage, ApiResult};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures::channel::oneshot;
use log::{debug, error, info};

use rspotify::http::HttpError;
use rspotify::model::{
    AlbumId, AlbumType, ArtistId, CursorBasedPage, EpisodeId, FullAlbum, FullArtist, FullEpisode,
    FullPlaylist, FullShow, FullTrack, ItemPositions, Market, Page, PlayableId, PlaylistId,
    PrivateUser, Recommendations, SavedAlbum, SavedTrack, SearchResult, SearchType, Show, ShowId,
    SimplifiedTrack, TrackId, UserId,
};
use rspotify::{prelude::*, AuthCodeSpotify, ClientError, ClientResult, Token};
use std::collections::HashSet;
use std::iter::FromIterator;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct WebApi {
    api: AuthCodeSpotify,
    user: Option<String>,
    worker_channel: Arc<RwLock<Option<mpsc::UnboundedSender<WorkerCommand>>>>,
    token_expiration: Arc<RwLock<DateTime<Utc>>>,
}

impl Default for WebApi {
    fn default() -> Self {
        Self {
            api: AuthCodeSpotify::default(),
            user: None,
            worker_channel: Arc::new(RwLock::new(None)),
            token_expiration: Arc::new(RwLock::new(Utc::now())),
        }
    }
}

impl WebApi {
    pub fn new() -> WebApi {
        Self::default()
    }

    pub fn set_user(&mut self, user: Option<String>) {
        self.user = user;
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
            let token_option = ASYNC_RUNTIME.block_on(token_rx).unwrap();
            if let Some(token) = token_option {
                *self.api.token.lock().expect("can't writelock api token") = Some(Token {
                    access_token: token.access_token,
                    expires_in: chrono::Duration::seconds(token.expires_in.into()),
                    scopes: HashSet::from_iter(token.scope),
                    expires_at: None,
                    refresh_token: None,
                });
                *self
                    .token_expiration
                    .write()
                    .expect("could not writelock token") =
                    Utc::now() + ChronoDuration::seconds(token.expires_in.into());
            } else {
                error!("Failed to update token");
            }
        } else {
            error!("worker channel is not set");
        }
    }

    /// retries once when rate limits are hit
    fn api_with_retry<F, R>(&self, cb: F) -> Option<R>
    where
        F: Fn(&AuthCodeSpotify) -> ClientResult<R>,
    {
        let result = { cb(&self.api) };
        match result {
            Ok(v) => Some(v),
            Err(ClientError::Http(error)) => {
                debug!("http error: {:?}", error);
                if let HttpError::StatusCode(response) = error.as_ref() {
                    match response.status() {
                        429 => {
                            let waiting_duration = response
                                .header("Retry-After")
                                .and_then(|v| v.parse::<u64>().ok());
                            debug!("rate limit hit. waiting {:?} seconds", waiting_duration);
                            thread::sleep(Duration::from_secs(waiting_duration.unwrap_or(0)));
                            cb(&self.api).ok()
                        }
                        401 => {
                            debug!("token unauthorized. trying refresh..");
                            self.update_token();
                            cb(&self.api).ok()
                        }
                        _ => {
                            error!("unhandled api error: {:?}", response);
                            None
                        }
                    }
                } else {
                    None
                }
            }
            Err(e) => {
                error!("unhandled api error: {}", e);
                None
            }
        }
    }

    pub fn append_tracks(
        &self,
        playlist_id: &str,
        tracks: &[Playable],
        position: Option<i32>,
    ) -> bool {
        self.api_with_retry(|api| {
            let trackids: Vec<PlayableId> = tracks.iter().map(|playable| playable.into()).collect();
            api.playlist_add_items(
                PlaylistId::from_id(playlist_id).unwrap(),
                trackids.iter().map(|id| id.as_ref()),
                position.map(|num| chrono::Duration::milliseconds(num as i64)),
            )
        })
        .is_some()
    }

    pub fn delete_tracks(
        &self,
        playlist_id: &str,
        snapshot_id: &str,
        playables: &[Playable],
    ) -> bool {
        self.api_with_retry(move |api| {
            let playable_ids: Vec<PlayableId> =
                playables.iter().map(|playable| playable.into()).collect();
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
        .is_some()
    }

    pub fn overwrite_playlist(&self, id: &str, tracks: &[Playable]) {
        // create mutable copy for chunking
        let mut tracks: Vec<Playable> = tracks.to_vec();

        // we can only send 100 tracks per request
        let mut remainder = if tracks.len() > 100 {
            Some(tracks.split_off(100))
        } else {
            None
        };

        if let Some(()) = self.api_with_retry(|api| {
            let playable_ids: Vec<PlayableId> =
                tracks.iter().map(|playable| playable.into()).collect();
            api.playlist_replace_items(
                PlaylistId::from_id(id).unwrap(),
                playable_ids.iter().map(|p| p.as_ref()),
            )
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
        self.api_with_retry(|api| api.playlist_unfollow(PlaylistId::from_id(id).unwrap()))
            .is_some()
    }

    pub fn create_playlist(
        &self,
        name: &str,
        public: Option<bool>,
        description: Option<&str>,
    ) -> Option<String> {
        let result = self.api_with_retry(|api| {
            api.user_playlist_create(
                UserId::from_id(self.user.as_ref().unwrap()).unwrap(),
                name,
                public,
                None,
                description,
            )
        });
        result.map(|r| r.id.id().to_string())
    }

    pub fn album(&self, album_id: &str) -> Option<FullAlbum> {
        let aid = AlbumId::from_id(album_id).ok()?;
        self.api_with_retry(|api| api.album(aid.clone()))
    }

    pub fn artist(&self, artist_id: &str) -> Option<FullArtist> {
        let aid = ArtistId::from_id(artist_id).ok()?;
        self.api_with_retry(|api| api.artist(aid.clone()))
    }

    pub fn playlist(&self, playlist_id: &str) -> Option<FullPlaylist> {
        let pid = PlaylistId::from_id(playlist_id).ok()?;
        self.api_with_retry(|api| api.playlist(pid.clone(), None, Some(Market::FromToken)))
    }

    pub fn track(&self, track_id: &str) -> Option<FullTrack> {
        let tid = TrackId::from_id(track_id).ok()?;
        self.api_with_retry(|api| api.track(tid.clone()))
    }

    pub fn get_show(&self, show_id: &str) -> Option<FullShow> {
        let sid = ShowId::from_id(show_id).ok()?;
        self.api_with_retry(|api| api.get_a_show(sid.clone(), Some(Market::FromToken)))
    }

    pub fn episode(&self, episode_id: &str) -> Option<FullEpisode> {
        let eid = EpisodeId::from_id(episode_id).ok()?;
        self.api_with_retry(|api| api.get_an_episode(eid.clone(), Some(Market::FromToken)))
    }

    pub fn recommendations(
        &self,
        seed_artists: Option<Vec<&str>>,
        seed_genres: Option<Vec<&str>>,
        seed_tracks: Option<Vec<&str>>,
    ) -> Option<Recommendations> {
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
    }

    pub fn search(
        &self,
        searchtype: SearchType,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Option<SearchResult> {
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
        .take()
    }

    pub fn current_user_playlist(&self) -> ApiResult<Playlist> {
        const MAX_LIMIT: u32 = 50;
        let spotify = self.clone();
        let fetch_page = move |offset: u32| {
            debug!("fetching user playlists, offset: {}", offset);
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

    pub fn user_playlist_tracks(&self, playlist_id: &str) -> ApiResult<Playable> {
        const MAX_LIMIT: u32 = 100;
        let spotify = self.clone();
        let playlist_id = playlist_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!(
                "fetching playlist {} tracks, offset: {}",
                playlist_id, offset
            );
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
                            .enumerate()
                            .flat_map(|(index, pt)| {
                                pt.track.as_ref().map(|t| {
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

    pub fn full_album(&self, album_id: &str) -> Option<FullAlbum> {
        self.api_with_retry(|api| api.album(AlbumId::from_id(album_id).unwrap()))
    }

    pub fn album_tracks(
        &self,
        album_id: &str,
        limit: u32,
        offset: u32,
    ) -> Option<Page<SimplifiedTrack>> {
        self.api_with_retry(|api| {
            api.album_track_manual(
                AlbumId::from_id(album_id).unwrap(),
                Some(limit),
                Some(offset),
            )
        })
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

    pub fn show_episodes(&self, show_id: &str) -> ApiResult<Episode> {
        const MAX_SIZE: u32 = 50;
        let spotify = self.clone();
        let show_id = show_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!("fetching show {} episodes, offset: {}", &show_id, offset);
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

    pub fn get_saved_shows(&self, offset: u32) -> Option<Page<Show>> {
        self.api_with_retry(|api| api.get_saved_show_manual(Some(50), Some(offset)))
    }

    pub fn save_shows(&self, ids: Vec<&str>) -> bool {
        self.api_with_retry(|api| {
            api.save_shows(
                ids.iter()
                    .map(|id| ShowId::from_id(*id).unwrap())
                    .collect::<Vec<ShowId>>(),
            )
        })
        .is_some()
    }

    pub fn unsave_shows(&self, ids: Vec<&str>) -> bool {
        self.api_with_retry(|api| {
            api.remove_users_saved_shows(
                ids.iter()
                    .map(|id| ShowId::from_id(*id).unwrap())
                    .collect::<Vec<ShowId>>(),
                Some(Market::FromToken),
            )
        })
        .is_some()
    }

    pub fn current_user_followed_artists(
        &self,
        last: Option<&str>,
    ) -> Option<CursorBasedPage<FullArtist>> {
        self.api_with_retry(|api| api.current_user_followed_artists(last, Some(50)))
    }

    pub fn user_follow_artists(&self, ids: Vec<&str>) -> Option<()> {
        self.api_with_retry(|api| {
            api.user_follow_artists(
                ids.iter()
                    .map(|id| ArtistId::from_id(*id).unwrap())
                    .collect::<Vec<ArtistId>>(),
            )
        })
    }

    pub fn user_unfollow_artists(&self, ids: Vec<&str>) -> Option<()> {
        self.api_with_retry(|api| {
            api.user_unfollow_artists(
                ids.iter()
                    .map(|id| ArtistId::from_id(*id).unwrap())
                    .collect::<Vec<ArtistId>>(),
            )
        })
    }

    pub fn current_user_saved_albums(&self, offset: u32) -> Option<Page<SavedAlbum>> {
        self.api_with_retry(|api| {
            api.current_user_saved_albums_manual(Some(Market::FromToken), Some(50), Some(offset))
        })
    }

    pub fn current_user_saved_albums_add(&self, ids: Vec<&str>) -> Option<()> {
        self.api_with_retry(|api| {
            api.current_user_saved_albums_add(
                ids.iter()
                    .map(|id| AlbumId::from_id(*id).unwrap())
                    .collect::<Vec<AlbumId>>(),
            )
        })
    }

    pub fn current_user_saved_albums_delete(&self, ids: Vec<&str>) -> Option<()> {
        self.api_with_retry(|api| {
            api.current_user_saved_albums_delete(
                ids.iter()
                    .map(|id| AlbumId::from_id(*id).unwrap())
                    .collect::<Vec<AlbumId>>(),
            )
        })
    }

    pub fn current_user_saved_tracks(&self, offset: u32) -> Option<Page<SavedTrack>> {
        self.api_with_retry(|api| {
            api.current_user_saved_tracks_manual(Some(Market::FromToken), Some(50), Some(offset))
        })
    }

    pub fn current_user_saved_tracks_add(&self, ids: Vec<&str>) -> Option<()> {
        self.api_with_retry(|api| {
            api.current_user_saved_tracks_add(
                ids.iter()
                    .map(|id| TrackId::from_id(*id).unwrap())
                    .collect::<Vec<TrackId>>(),
            )
        })
    }

    pub fn current_user_saved_tracks_delete(&self, ids: Vec<&str>) -> Option<()> {
        self.api_with_retry(|api| {
            api.current_user_saved_tracks_delete(
                ids.iter()
                    .map(|id| TrackId::from_id(*id).unwrap())
                    .collect::<Vec<TrackId>>(),
            )
        })
    }

    pub fn user_playlist_follow_playlist(&self, id: &str) -> Option<()> {
        self.api_with_retry(|api| api.playlist_follow(PlaylistId::from_id(id).unwrap(), None))
    }

    pub fn artist_top_tracks(&self, id: &str) -> Option<Vec<Track>> {
        self.api_with_retry(|api| {
            api.artist_top_tracks(ArtistId::from_id(id).unwrap(), Market::FromToken)
        })
        .map(|ft| ft.iter().map(|t| t.into()).collect())
    }

    pub fn artist_related_artists(&self, id: &str) -> Option<Vec<Artist>> {
        self.api_with_retry(|api| api.artist_related_artists(ArtistId::from_id(id).unwrap()))
            .map(|fa| fa.iter().map(|a| a.into()).collect())
    }

    pub fn categories(&self) -> ApiResult<Category> {
        const MAX_LIMIT: u32 = 50;
        let spotify = self.clone();
        let fetch_page = move |offset: u32| {
            debug!("fetching categories, offset: {}", offset);
            spotify.api_with_retry(|api| {
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

    pub fn category_playlists(&self, category_id: &str) -> ApiResult<Playlist> {
        const MAX_LIMIT: u32 = 50;
        let spotify = self.clone();
        let category_id = category_id.to_string();
        let fetch_page = move |offset: u32| {
            debug!("fetching category playlists, offset: {}", offset);
            spotify.api_with_retry(|api| {
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

    pub fn current_user(&self) -> Option<PrivateUser> {
        self.api_with_retry(|api| api.current_user())
    }
}
