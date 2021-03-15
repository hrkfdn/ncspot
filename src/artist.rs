use std::fmt;
use std::sync::Arc;

use rspotify::model::artist::{FullArtist, SimplifiedArtist};

use crate::album::Album;
use crate::library::Library;
use crate::playable::Playable;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::track::Track;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::artist::ArtistView;

#[derive(Clone, Deserialize, Serialize)]
pub struct Artist {
    pub id: Option<String>,
    pub name: String,
    pub url: Option<String>,
    pub albums: Option<Vec<Album>>,
    pub tracks: Option<Vec<Track>>,
    pub is_followed: bool,
}

impl Artist {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id: Some(id),
            name,
            url: None,
            albums: None,
            tracks: None,
            is_followed: false,
        }
    }

    pub fn load_albums(&mut self, spotify: Arc<Spotify>) {
        if let Some(albums) = self.albums.as_mut() {
            for album in albums {
                album.load_tracks(spotify.clone());
            }
            return;
        }

        if let Some(ref artist_id) = self.id {
            let mut collected_ids: Vec<String> = Vec::new();
            let mut offset = 0;
            while let Some(sas) = spotify.artist_albums(artist_id, 50, offset) {
                let items_len = sas.items.len() as u32;
                debug!("got {} albums", items_len);

                for sa in sas.items {
                    if Some("appears_on") == sa.album_group.as_deref() {
                        continue;
                    }
                    if let Some(album_id) = sa.id {
                        collected_ids.push(album_id);
                    }
                }

                match sas.next {
                    Some(_) => offset += items_len,
                    None => break,
                }
            }

            let albums = match spotify.albums(&collected_ids) {
                Some(fas) => fas.iter().map(Album::from).collect(),
                None => Vec::new(),
            };
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
            is_followed: false,
        }
    }
}

impl From<&FullArtist> for Artist {
    fn from(fa: &FullArtist) -> Self {
        Self {
            id: Some(fa.id.clone()),
            name: fa.name.clone(),
            url: Some(fa.uri.clone()),
            albums: None,
            tracks: None,
            is_followed: false,
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
        write!(f, "{} ({:?})", self.name, self.id)
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
                .filter_map(|t| t.id())
                .collect();
            let ids: Vec<String> = tracks.iter().filter_map(|t| t.id.clone()).collect();
            !ids.is_empty() && playing == ids
        } else {
            false
        }
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }

    fn display_left(&self) -> String {
        format!("{}", self)
    }

    fn display_right(&self, library: Arc<Library>) -> String {
        let followed = if library.is_followed_artist(self) {
            if library.cfg.values().use_nerdfont.unwrap_or(false) {
                "\u{f62b} "
            } else {
                "✓ "
            }
        } else {
            ""
        };

        let tracks = if let Some(tracks) = self.tracks.as_ref() {
            format!("{:>3} saved tracks", tracks.len())
        } else {
            "".into()
        };

        format!("{}{}", followed, tracks)
    }

    fn play(&mut self, queue: Arc<Queue>) {
        self.load_albums(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            let tracks: Vec<Playable> = tracks
                .iter()
                .map(|track| Playable::Track(track.clone()))
                .collect();
            let index = queue.append_next(tracks);
            queue.play(index, true, true);
        }
    }

    fn play_next(&mut self, queue: Arc<Queue>) {
        self.load_albums(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for t in tracks.iter().rev() {
                queue.insert_after_current(Playable::Track(t.clone()));
            }
        }
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        self.load_albums(queue.get_spotify());

        if let Some(tracks) = self.tracks() {
            for t in tracks {
                queue.append(Playable::Track(t.clone()));
            }
        }
    }

    fn save(&mut self, library: Arc<Library>) {
        library.follow_artist(self);
    }

    fn unsave(&mut self, library: Arc<Library>) {
        library.unfollow_artist(self);
    }

    fn toggle_saved(&mut self, library: Arc<Library>) {
        if library.is_followed_artist(self) {
            library.unfollow_artist(self);
        } else {
            library.follow_artist(self);
        }
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(ArtistView::new(queue, library, self).as_boxed_view_ext())
    }

    fn share_url(&self) -> Option<String> {
        self.id
            .clone()
            .map(|id| format!("https://open.spotify.com/artist/{}", id))
    }
}
