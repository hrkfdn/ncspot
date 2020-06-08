use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rspotify::model::album::FullAlbum;
use rspotify::model::track::{FullTrack, SavedTrack, SimplifiedTrack};

use crate::album::Album;
use crate::artist::Artist;
use crate::library::Library;
use crate::queue::{Queue, Playable};
use crate::traits::{ListItem, ViewExt};

#[derive(Clone, Deserialize, Serialize)]
pub struct Track {
    pub id: Option<String>,
    pub title: String,
    pub track_number: u32,
    pub disc_number: i32,
    pub duration: u32,
    pub artists: Vec<String>,
    pub artist_ids: Vec<String>,
    pub album: String,
    pub album_id: Option<String>,
    pub album_artists: Vec<String>,
    pub cover_url: String,
    pub url: String,
    pub added_at: Option<DateTime<Utc>>,
}

impl Track {
    pub fn from_simplified_track(track: &SimplifiedTrack, album: &FullAlbum) -> Track {
        let artists = track
            .artists
            .iter()
            .map(|ref artist| artist.name.clone())
            .collect::<Vec<String>>();
        let artist_ids = track
            .artists
            .iter()
            .filter(|a| a.id.is_some())
            .map(|ref artist| artist.id.clone().unwrap())
            .collect::<Vec<String>>();
        let album_artists = album
            .artists
            .iter()
            .map(|ref artist| artist.name.clone())
            .collect::<Vec<String>>();

        let cover_url = match album.images.get(0) {
            Some(image) => image.url.clone(),
            None => "".to_owned(),
        };

        Self {
            id: track.id.clone(),
            title: track.name.clone(),
            track_number: track.track_number,
            disc_number: track.disc_number,
            duration: track.duration_ms,
            artists,
            artist_ids,
            album: album.name.clone(),
            album_id: Some(album.id.clone()),
            album_artists,
            cover_url,
            url: track.uri.clone(),
            added_at: None,
        }
    }

    pub fn duration_str(&self) -> String {
        let minutes = self.duration / 60_000;
        let seconds = (self.duration / 1000) % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }
}

impl From<&FullTrack> for Track {
    fn from(track: &FullTrack) -> Self {
        let artists = track
            .artists
            .iter()
            .map(|ref artist| artist.name.clone())
            .collect::<Vec<String>>();
        let artist_ids = track
            .artists
            .iter()
            .filter(|a| a.id.is_some())
            .map(|ref artist| artist.id.clone().unwrap())
            .collect::<Vec<String>>();
        let album_artists = track
            .album
            .artists
            .iter()
            .map(|ref artist| artist.name.clone())
            .collect::<Vec<String>>();

        let cover_url = match track.album.images.get(0) {
            Some(image) => image.url.clone(),
            None => "".to_owned(),
        };

        Self {
            id: track.id.clone(),
            title: track.name.clone(),
            track_number: track.track_number,
            disc_number: track.disc_number,
            duration: track.duration_ms,
            artists,
            artist_ids,
            album: track.album.name.clone(),
            album_id: track.album.id.clone(),
            album_artists,
            cover_url,
            url: track.uri.clone(),
            added_at: None,
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
    fn is_playing(&self, queue: Arc<Queue>) -> bool {
        let current = queue.get_current();
        current.map(|t| t.id() == self.id).unwrap_or(false)
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }

    fn display_left(&self) -> String {
        format!("{}", self)
    }

    fn display_right(&self, library: Arc<Library>) -> String {
        let saved = if library.is_saved_track(&Playable::Track(self.clone())) {
            if library.use_nerdfont {
                "\u{f62b} "
            } else {
                "✓ "
            }
        } else {
            ""
        };
        format!("{}{}", saved, self.duration_str())
    }

    fn play(&mut self, queue: Arc<Queue>) {
        let index = queue.append_next(vec![Playable::Track(self.clone())]);
        queue.play(index, true, false);
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        queue.append(Playable::Track(self.clone()));
    }

    fn save(&mut self, library: Arc<Library>) {
        library.save_tracks(vec![self], true);
    }

    fn unsave(&mut self, library: Arc<Library>) {
        library.unsave_tracks(vec![self], true);
    }

    fn toggle_saved(&mut self, library: Arc<Library>) {
        if library.is_saved_track(&Playable::Track(self.clone())) {
            library.unsave_tracks(vec![self], true);
        } else {
            library.save_tracks(vec![self], true);
        }
    }

    fn open(&self, _queue: Arc<Queue>, _library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        None
    }

    fn share_url(&self) -> Option<String> {
        self.id
            .clone()
            .map(|id| format!("https://open.spotify.com/track/{}", id))
    }

    fn album(&self, queue: Arc<Queue>) -> Option<Album> {
        let spotify = queue.get_spotify();

        match self.album_id {
            Some(ref album_id) => spotify.album(&album_id).map(|ref fa| fa.into()),
            None => None,
        }
    }

    fn artist(&self) -> Option<Artist> {
        Some(Artist::new(
            self.artist_ids[0].clone(),
            self.artists[0].clone(),
        ))
    }

    fn track(&self) -> Option<Track> {
        Some(self.clone())
    }
}
