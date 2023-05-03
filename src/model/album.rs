use rand::{seq::IteratorRandom, thread_rng};
use rspotify::model::Id;
use std::fmt;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use log::debug;
use rspotify::model::album::{FullAlbum, SavedAlbum, SimplifiedAlbum};

use crate::library::Library;
use crate::model::artist::Artist;
use crate::model::playable::Playable;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::{album::AlbumView, listview::ListView};

#[derive(Clone, Deserialize, Serialize)]
pub struct Album {
    pub id: Option<String>,
    pub title: String,
    pub artists: Vec<String>,
    pub artist_ids: Vec<String>,
    pub year: String,
    pub cover_url: Option<String>,
    pub url: Option<String>,
    pub tracks: Option<Vec<Track>>,
    pub added_at: Option<DateTime<Utc>>,
    total_tracks: Option<usize>,
}

impl Album {
    pub fn load_all_tracks(&mut self, spotify: Spotify) {
        if self.tracks.is_some() && self.tracks.as_ref().map(|t| t.len()) == self.total_tracks {
            return;
        }

        if let Some(ref album_id) = self.id {
            let mut collected_tracks = Vec::new();
            if let Some(full_album) = spotify.api.full_album(album_id) {
                let mut tracks_result = Some(full_album.tracks.clone());
                while let Some(ref tracks) = tracks_result {
                    for t in &tracks.items {
                        collected_tracks.push(Track::from_simplified_track(t, &full_album));
                    }

                    debug!("got {} tracks", tracks.items.len());

                    // load next batch if necessary
                    tracks_result = match tracks.next {
                        Some(_) => {
                            debug!("requesting tracks again..");
                            spotify.api.album_tracks(
                                album_id,
                                50,
                                tracks.offset + tracks.items.len() as u32,
                            )
                        }
                        None => None,
                    }
                }
            }

            self.total_tracks = Some(collected_tracks.len());
            self.tracks = Some(collected_tracks);
        }
    }
}

impl From<&SimplifiedAlbum> for Album {
    fn from(sa: &SimplifiedAlbum) -> Self {
        Self {
            id: sa.id.as_ref().map(|id| id.id().to_string()),
            title: sa.name.clone(),
            artists: sa.artists.iter().map(|sa| sa.name.clone()).collect(),
            artist_ids: sa
                .artists
                .iter()
                .filter_map(|a| a.id.as_ref().map(|id| id.id().to_string()))
                .collect(),
            year: sa
                .release_date
                .clone()
                .unwrap_or_default()
                .split('-')
                .next()
                .unwrap()
                .into(),
            cover_url: sa.images.get(0).map(|i| i.url.clone()),
            url: sa.id.as_ref().map(|id| id.url()),
            tracks: None,
            added_at: None,
            total_tracks: None,
        }
    }
}

impl From<&FullAlbum> for Album {
    fn from(fa: &FullAlbum) -> Self {
        let tracks = Some(
            fa.tracks
                .items
                .iter()
                .map(|st| Track::from_simplified_track(st, fa))
                .collect(),
        );

        Self {
            id: Some(fa.id.id().to_string()),
            title: fa.name.clone(),
            artists: fa.artists.iter().map(|sa| sa.name.clone()).collect(),
            artist_ids: fa
                .artists
                .iter()
                .filter_map(|a| a.id.as_ref().map(|id| id.id().to_string()))
                .collect(),
            year: fa.release_date.split('-').next().unwrap().into(),
            cover_url: fa.images.get(0).map(|i| i.url.clone()),
            url: Some(fa.id.uri()),
            tracks,
            added_at: None,
            total_tracks: Some(fa.tracks.total as usize),
        }
    }
}

impl From<&SavedAlbum> for Album {
    fn from(sa: &SavedAlbum) -> Self {
        let mut album: Self = (&sa.album).into();
        album.added_at = Some(sa.added_at);
        album
    }
}

impl fmt::Display for Album {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} - {}", self.artists.join(", "), self.title)
    }
}

impl fmt::Debug for Album {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "({} - {} ({:?}))",
            self.artists.join(", "),
            self.title,
            self.id
        )
    }
}

impl ListItem for Album {
    fn is_playing(&self, queue: &Queue) -> bool {
        if let Some(tracks) = self.tracks.as_ref() {
            let playing: Vec<String> = queue
                .queue
                .read()
                .unwrap()
                .iter()
                .filter_map(|t| t.id())
                .collect();

            let ids: Vec<String> = tracks.iter().filter_map(|t| t.id.clone()).collect();
            !ids.is_empty() && playing == ids
        } else {
            false
        }
    }

    fn display_left(&self, _library: &Library) -> String {
        format!("{self}")
    }

    fn display_right(&self, library: &Library) -> String {
        let saved = if library.is_saved_album(self) {
            if library.cfg.values().use_nerdfont.unwrap_or(false) {
                "\u{f012c} "
            } else {
                "✓ "
            }
        } else {
            ""
        };
        format!("{}{}", saved, self.year)
    }

    fn play(&mut self, queue: &Queue) {
        self.load_all_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            let tracks: Vec<Playable> = tracks
                .iter()
                .map(|track| Playable::Track(track.clone()))
                .collect();
            let index = queue.append_next(&tracks);
            queue.play(index, true, true);
        }
    }

    fn play_next(&mut self, queue: &Queue) {
        self.load_all_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for t in tracks.iter().rev() {
                queue.insert_after_current(Playable::Track(t.clone()));
            }
        }
    }

    fn queue(&mut self, queue: &Queue) {
        self.load_all_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for t in tracks {
                queue.append(Playable::Track(t.clone()));
            }
        }
    }

    fn toggle_saved(&mut self, library: &Library) {
        if library.is_saved_album(self) {
            library.unsave_album(self);
        } else {
            library.save_album(self);
        }
    }

    fn save(&mut self, library: &Library) {
        library.save_album(self);
    }

    fn unsave(&mut self, library: &Library) {
        library.unsave_album(self);
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(AlbumView::new(queue, library, self).into_boxed_view_ext())
    }

    fn open_recommendations(
        &mut self,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> Option<Box<dyn ViewExt>> {
        self.load_all_tracks(queue.get_spotify());
        const MAX_SEEDS: usize = 5;
        let track_ids: Vec<&str> = self
            .tracks
            .as_ref()?
            .iter()
            .filter_map(|t| t.id.as_deref())
            // spotify allows at max 5 seed items, so choose 4 random tracks...
            .choose_multiple(&mut thread_rng(), MAX_SEEDS - 1);

        let artist_id: Option<String> = self
            .artist_ids
            .iter()
            .cloned()
            // ...and one artist
            .choose(&mut thread_rng());

        if track_ids.is_empty() && artist_id.is_some() {
            return None;
        }

        let spotify = queue.get_spotify();
        let recommendations: Option<Vec<Track>> = spotify
            .api
            .recommendations(
                artist_id.as_ref().map(|aid| vec![aid.as_str()]),
                None,
                Some(track_ids),
            )
            .map(|r| r.tracks)
            .map(|tracks| tracks.iter().map(Track::from).collect());
        recommendations.map(|tracks| {
            ListView::new(
                Arc::new(RwLock::new(tracks)),
                queue.clone(),
                library.clone(),
            )
            .with_title(&format!("Similar to Album \"{}\"", self.title))
            .into_boxed_view_ext()
        })
    }

    fn share_url(&self) -> Option<String> {
        self.id
            .clone()
            .map(|id| format!("https://open.spotify.com/album/{id}"))
    }

    fn artists(&self) -> Option<Vec<Artist>> {
        Some(
            self.artist_ids
                .iter()
                .zip(self.artists.iter())
                .map(|(id, name)| Artist::new(id.clone(), name.clone()))
                .collect(),
        )
    }

    #[inline]
    fn is_saved(&self, library: &Library) -> Option<bool> {
        Some(library.is_saved_album(self))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
