use std::fmt;
use std::sync::Arc;

use rspotify::spotify::model::artist::{FullArtist, SimplifiedArtist};

use album::Album;
use queue::Queue;
use spotify::Spotify;
use track::Track;
use traits::ListItem;

#[derive(Clone, Deserialize, Serialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub url: String,
    pub albums: Option<Vec<Album>>,
    pub tracks: Option<Vec<Track>>,
}

impl Artist {
    fn load_albums(&mut self, spotify: Arc<Spotify>) {
        if let Some(albums) = self.albums.as_mut() {
            for album in albums {
                album.load_tracks(spotify.clone());
            }
            return;
        }

        if let Some(sas) = spotify.artist_albums(&self.id, 50, 0) {
            let mut albums: Vec<Album> = Vec::new();

            for sa in sas.items {
                if Some("appears_on".into()) == sa.album_group {
                    continue;
                }

                if let Some(fa) = spotify.full_album(&sa.id).as_ref() {
                    albums.push(fa.into());
                }
            }

            self.albums = Some(albums);
        }
    }

    fn tracks(&self) -> Option<Vec<&Track>> {
        if let Some(tracks) = self.tracks.as_ref() {
            Some(tracks.iter().collect())
        } else if let Some(albums) = self.albums.as_ref() {
            Some(
                albums
                    .iter()
                    .map(|a| a.tracks.as_ref().unwrap())
                    .flatten()
                    .collect(),
            )
        } else {
            None
        }
    }
}

impl From<&SimplifiedArtist> for Artist {
    fn from(sa: &SimplifiedArtist) -> Self {
        Self {
            id: sa.id.clone(),
            name: sa.name.clone(),
            url: sa.uri.clone(),
            albums: None,
            tracks: None,
        }
    }
}

impl From<&FullArtist> for Artist {
    fn from(fa: &FullArtist) -> Self {
        Self {
            id: fa.id.clone(),
            name: fa.name.clone(),
            url: fa.uri.clone(),
            albums: None,
            tracks: None,
        }
    }
}

impl fmt::Display for Artist {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl fmt::Debug for Artist {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.id)
    }
}

impl ListItem for Artist {
    fn is_playing(&self, queue: Arc<Queue>) -> bool {
        if let Some(tracks) = self.tracks() {
            let playing: Vec<String> = queue
                .queue
                .read()
                .unwrap()
                .iter()
                .map(|t| t.id.clone())
                .collect();
            let ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
            !ids.is_empty() && playing == ids
        } else {
            false
        }
    }

    fn display_left(&self) -> String {
        format!("{}", self)
    }

    fn display_right(&self) -> String {
        // TODO: indicate following status
        if let Some(tracks) = self.tracks.as_ref() {
            format!("{} saved tracks", tracks.len())
        } else {
            "".into()
        }
    }

    fn play(&mut self, queue: Arc<Queue>) {
        self.load_albums(queue.get_spotify());

        if let Some(tracks) = self.tracks() {
            let index = queue.append_next(tracks);
            queue.play(index, true);
        }
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        self.load_albums(queue.get_spotify());

        if let Some(tracks) = self.tracks() {
            for t in tracks {
                queue.append(t);
            }
        }
    }
}
