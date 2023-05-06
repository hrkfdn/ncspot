use std::fmt;
use std::sync::{Arc, RwLock};

use crate::config;
use crate::utils::ms_to_hms;
use chrono::{DateTime, Utc};
use rspotify::model::album::FullAlbum;
use rspotify::model::track::{FullTrack, SavedTrack, SimplifiedTrack};
use rspotify::model::Id;

use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::playable::Playable;
use crate::queue::Queue;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::listview::ListView;

#[derive(Clone, Deserialize, Serialize)]
pub struct Track {
    pub id: Option<String>,
    pub uri: String,
    pub title: String,
    pub track_number: u32,
    pub disc_number: i32,
    pub duration: u32,
    pub artists: Vec<String>,
    pub artist_ids: Vec<String>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub album_artists: Vec<String>,
    pub cover_url: Option<String>,
    pub url: String,
    pub added_at: Option<DateTime<Utc>>,
    pub list_index: usize,
}

impl Track {
    pub fn from_simplified_track(track: &SimplifiedTrack, album: &FullAlbum) -> Track {
        let artists = track
            .artists
            .iter()
            .map(|artist| artist.name.clone())
            .collect::<Vec<String>>();
        let artist_ids = track
            .artists
            .iter()
            .filter_map(|a| a.id.as_ref().map(|id| id.id().to_string()))
            .collect::<Vec<String>>();
        let album_artists = album
            .artists
            .iter()
            .map(|artist| artist.name.clone())
            .collect::<Vec<String>>();

        Self {
            id: track.id.as_ref().map(|id| id.id().to_string()),
            uri: track.id.as_ref().map(|id| id.uri()).unwrap_or_default(),
            title: track.name.clone(),
            track_number: track.track_number,
            disc_number: track.disc_number,
            duration: track.duration.num_milliseconds() as u32,
            artists,
            artist_ids,
            album: Some(album.name.clone()),
            album_id: Some(album.id.id().to_string()),
            album_artists,
            cover_url: album.images.get(0).map(|img| img.url.clone()),
            url: track.id.as_ref().map(|id| id.url()).unwrap_or_default(),
            added_at: None,
            list_index: 0,
        }
    }

    pub fn duration_str(&self) -> String {
        ms_to_hms(self.duration)
    }
}

impl From<&SimplifiedTrack> for Track {
    fn from(track: &SimplifiedTrack) -> Self {
        let artists = track
            .artists
            .iter()
            .map(|artist| artist.name.clone())
            .collect::<Vec<String>>();
        let artist_ids = track
            .artists
            .iter()
            .filter_map(|a| a.id.as_ref().map(|a| a.id().to_string()))
            .collect::<Vec<String>>();

        Self {
            id: track.id.as_ref().map(|id| id.id().to_string()),
            uri: track.id.as_ref().map(|id| id.uri()).unwrap_or_default(),
            title: track.name.clone(),
            track_number: track.track_number,
            disc_number: track.disc_number,
            duration: track.duration.num_milliseconds() as u32,
            artists,
            artist_ids,
            album: None,
            album_id: None,
            album_artists: Vec::new(),
            cover_url: None,
            url: track.id.as_ref().map(|id| id.url()).unwrap_or_default(),
            added_at: None,
            list_index: 0,
        }
    }
}

impl From<&FullTrack> for Track {
    fn from(track: &FullTrack) -> Self {
        let artists = track
            .artists
            .iter()
            .map(|artist| artist.name.clone())
            .collect::<Vec<String>>();
        let artist_ids = track
            .artists
            .iter()
            .filter_map(|a| a.id.as_ref().map(|a| a.id().to_string()))
            .collect::<Vec<String>>();
        let album_artists = track
            .album
            .artists
            .iter()
            .map(|artist| artist.name.clone())
            .collect::<Vec<String>>();

        Self {
            id: track.id.as_ref().map(|id| id.id().to_string()),
            uri: track.id.as_ref().map(|id| id.uri()).unwrap_or_default(),
            title: track.name.clone(),
            track_number: track.track_number,
            disc_number: track.disc_number,
            duration: track.duration.num_milliseconds() as u32,
            artists,
            artist_ids,
            album: Some(track.album.name.clone()),
            album_id: track.album.id.as_ref().map(|a| a.id().to_string()),
            album_artists,
            cover_url: track.album.images.get(0).map(|img| img.url.clone()),
            url: track.id.as_ref().map(|id| id.url()).unwrap_or_default(),
            added_at: None,
            list_index: 0,
        }
    }
}

impl From<&SavedTrack> for Track {
    fn from(st: &SavedTrack) -> Self {
        let mut track: Self = (&st.track).into();
        track.added_at = Some(st.added_at);
        track
    }
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} - {}", self.artists.join(", "), self.title)
    }
}

impl fmt::Debug for Track {
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

impl ListItem for Track {
    fn is_playing(&self, queue: &Queue) -> bool {
        let current = queue.get_current();
        current.map(|t| t.id() == self.id).unwrap_or(false)
    }

    fn display_left(&self, library: &Library) -> String {
        let formatting = library
            .cfg
            .values()
            .track_format
            .clone()
            .unwrap_or_default();
        let default = config::TrackFormat::default().left.unwrap();
        let left = formatting.left.unwrap_or_else(|| default.clone());
        if left != default {
            Playable::format(&Playable::Track(self.clone()), &left, library)
        } else {
            format!("{self}")
        }
    }

    fn display_center(&self, library: &Library) -> String {
        let formatting = library
            .cfg
            .values()
            .track_format
            .clone()
            .unwrap_or_default();
        let default = config::TrackFormat::default().center.unwrap();
        let center = formatting.center.unwrap_or_else(|| default.clone());
        if center != default {
            Playable::format(&Playable::Track(self.clone()), &center, library)
        } else {
            self.album.clone().unwrap_or_default()
        }
    }

    fn display_right(&self, library: &Library) -> String {
        let formatting = library
            .cfg
            .values()
            .track_format
            .clone()
            .unwrap_or_default();
        let default = config::TrackFormat::default().right.unwrap();
        let right = formatting.right.unwrap_or_else(|| default.clone());
        if right != default {
            Playable::format(&Playable::Track(self.clone()), &right, library)
        } else {
            let saved = if library.is_saved_track(&Playable::Track(self.clone())) {
                if library.cfg.values().use_nerdfont.unwrap_or(false) {
                    "\u{f012c}"
                } else {
                    "✓"
                }
            } else {
                ""
            };
            format!("{} {}", saved, self.duration_str())
        }
    }

    fn play(&mut self, queue: &Queue) {
        let index = queue.append_next(&vec![Playable::Track(self.clone())]);
        queue.play(index, true, false);
    }

    fn play_next(&mut self, queue: &Queue) {
        queue.insert_after_current(Playable::Track(self.clone()));
    }

    fn queue(&mut self, queue: &Queue) {
        queue.append(Playable::Track(self.clone()));
    }

    fn toggle_saved(&mut self, library: &Library) {
        if library.is_saved_track(&Playable::Track(self.clone())) {
            library.unsave_tracks(vec![self], true);
        } else {
            library.save_tracks(vec![self], true);
        }
    }

    fn save(&mut self, library: &Library) {
        library.save_tracks(vec![self], true);
    }

    fn unsave(&mut self, library: &Library) {
        library.unsave_tracks(vec![self], true);
    }

    fn open(&self, _queue: Arc<Queue>, _library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        None
    }

    fn open_recommendations(
        &mut self,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> Option<Box<dyn ViewExt>> {
        let spotify = queue.get_spotify();

        let recommendations: Option<Vec<Track>> = if let Some(id) = &self.id {
            spotify
                .api
                .recommendations(None, None, Some(vec![id]))
                .map(|r| r.tracks)
                .map(|tracks| tracks.iter().map(Track::from).collect())
        } else {
            None
        };

        recommendations.map(|tracks| {
            ListView::new(
                Arc::new(RwLock::new(tracks)),
                queue.clone(),
                library.clone(),
            )
            .with_title(&format!(
                "Similar to \"{} - {}\"",
                self.artists.join(", "),
                self.title
            ))
            .into_boxed_view_ext()
        })
    }

    fn share_url(&self) -> Option<String> {
        self.id
            .clone()
            .map(|id| format!("https://open.spotify.com/track/{id}"))
    }

    fn album(&self, queue: &Queue) -> Option<Album> {
        let spotify = queue.get_spotify();

        match self.album_id {
            Some(ref album_id) => spotify.api.album(album_id).map(|ref fa| fa.into()),
            None => None,
        }
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

    fn track(&self) -> Option<Track> {
        Some(self.clone())
    }

    #[inline]
    fn is_saved(&self, library: &Library) -> Option<bool> {
        Some(library.is_saved_track(&Playable::Track(self.clone())))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
