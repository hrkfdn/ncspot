use std::fmt;

use librespot::core::spotify_id::SpotifyId;
use rspotify::spotify::model::track::FullTrack;

#[derive(Clone)]
pub struct Track {
    pub id: SpotifyId,
    pub duration: u32,
    pub artists: String,
    pub title: String,
}

impl Track {
    pub fn new(track: &FullTrack) -> Track {
        let artists_joined = track
            .artists
            .iter()
            .map(|ref artist| artist.name.clone())
            .collect::<Vec<String>>()
            .join(", ");

        Track {
            id: SpotifyId::from_base62(&track.id).expect("could not load track"),
            duration: track.duration_ms / 1000,
            artists: artists_joined,
            title: track.name.clone(),
        }
    }

    pub fn duration_str(&self) -> String {
        let minutes = self.duration / 60;
        let seconds = self.duration % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} - {}", self.artists, self.title)
    }
}

impl fmt::Debug for Track {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "({} - {} ({})",
            self.artists,
            self.title,
            self.id.to_base62()
        )
    }
}
