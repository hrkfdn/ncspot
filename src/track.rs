use std::fmt;
use std::sync::Arc;

use rspotify::spotify::model::album::FullAlbum;
use rspotify::spotify::model::track::{FullTrack, SimplifiedTrack};

use queue::Queue;
use traits::ListItem;

#[derive(Clone, Deserialize, Serialize)]
pub struct Track {
    pub id: Option<String>,
    pub title: String,
    pub track_number: u32,
    pub disc_number: i32,
    pub duration: u32,
    pub artists: Vec<String>,
    pub album: String,
    pub album_artists: Vec<String>,
    pub cover_url: String,
    pub url: String,
}

impl Track {
    pub fn from_simplified_track(track: &SimplifiedTrack, album: &FullAlbum) -> Track {
        let artists = track
            .artists
            .iter()
            .map(|ref artist| artist.name.clone())
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
            album: album.name.clone(),
            album_artists,
            cover_url,
            url: track.uri.clone(),
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
            album: track.album.name.clone(),
            album_artists,
            cover_url,
            url: track.uri.clone(),
        }
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
        current.map(|t| t.id == self.id).unwrap_or(false)
    }

    fn display_left(&self) -> String {
        format!("{}", self)
    }

    fn display_right(&self) -> String {
        self.duration_str()
    }

    fn play(&mut self, queue: Arc<Queue>) {
        let index = queue.append_next(vec![self]);
        queue.play(index, true);
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        queue.append(self);
    }
}
