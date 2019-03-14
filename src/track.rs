use std::fmt;

use rspotify::spotify::model::track::FullTrack;

#[derive(Clone, Deserialize, Serialize)]
pub struct Track {
    pub id: String,
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
    pub fn new(track: &FullTrack) -> Track {
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

        Track {
            id: track.id.clone(),
            title: track.name.clone(),
            track_number: track.track_number,
            disc_number: track.disc_number,
            duration: track.duration_ms / 1000,
            artists: artists,
            album: track.album.name.clone(),
            album_artists: album_artists,
            cover_url: cover_url,
            url: track.uri.clone(),
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
        write!(f, "{} - {}", self.artists.join(", "), self.title)
    }
}

impl fmt::Debug for Track {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "({} - {} ({}))",
            self.artists.join(", "),
            self.title,
            self.id
        )
    }
}
