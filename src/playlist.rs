use std::iter::Iterator;
use std::sync::Arc;

use rspotify::spotify::model::playlist::{FullPlaylist, SimplifiedPlaylist};

use library::Library;
use queue::Queue;
use spotify::Spotify;
use track::Track;
use traits::{IntoBoxedViewExt, ListItem, ViewExt};
use ui::playlist::PlaylistView;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub snapshot_id: String,
    pub num_tracks: usize,
    pub tracks: Option<Vec<Track>>,
}

impl Playlist {
    pub fn load_tracks(&mut self, spotify: Arc<Spotify>) {
        if self.tracks.is_some() {
            return;
        }

        let mut collected_tracks = Vec::new();

        let mut tracks_result = spotify.user_playlist_tracks(&self.id, 100, 0);
        while let Some(ref tracks) = tracks_result.clone() {
            for listtrack in &tracks.items {
                collected_tracks.push((&listtrack.track).into());
            }
            debug!("got {} tracks", tracks.items.len());

            // load next batch if necessary
            tracks_result = match tracks.next {
                Some(_) => {
                    debug!("requesting tracks again..");
                    spotify.user_playlist_tracks(
                        &self.id,
                        100,
                        tracks.offset + tracks.items.len() as u32,
                    )
                }
                None => None,
            }
        }

        self.tracks = Some(collected_tracks);
    }
}

impl From<&SimplifiedPlaylist> for Playlist {
    fn from(list: &SimplifiedPlaylist) -> Self {
        let num_tracks = if let Some(number) = list.tracks.get("total") {
            number.as_u64().unwrap() as usize
        } else {
            0
        };

        Playlist {
            id: list.id.clone(),
            name: list.name.clone(),
            owner_id: list.owner.id.clone(),
            snapshot_id: list.snapshot_id.clone(),
            num_tracks,
            tracks: None,
        }
    }
}

impl From<&FullPlaylist> for Playlist {
    fn from(list: &FullPlaylist) -> Self {
        Playlist {
            id: list.id.clone(),
            name: list.name.clone(),
            owner_id: list.owner.id.clone(),
            snapshot_id: list.snapshot_id.clone(),
            num_tracks: list.tracks.total as usize,
            tracks: None,
        }
    }
}

impl ListItem for Playlist {
    fn is_playing(&self, queue: Arc<Queue>) -> bool {
        if let Some(tracks) = self.tracks.as_ref() {
            let playing: Vec<String> = queue
                .queue
                .read()
                .unwrap()
                .iter()
                .filter(|t| t.id.is_some())
                .map(|t| t.id.clone().unwrap())
                .collect();
            let ids: Vec<String> = tracks
                .iter()
                .filter(|t| t.id.is_some())
                .map(|t| t.id.clone().unwrap())
                .collect();
            !ids.is_empty() && playing == ids
        } else {
            false
        }
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }

    fn display_left(&self) -> String {
        self.name.clone()
    }

    fn display_right(&self, library: Arc<Library>) -> String {
        let followed = if library.is_followed_playlist(self) {
            if library.use_nerdfont {
                "\u{f62b} "
            } else {
                "✓ "
            }
        } else {
            ""
        };

        let num_tracks = self
            .tracks
            .as_ref()
            .map(|t| t.len())
            .unwrap_or(self.num_tracks);

        format!("{}{:>4} tracks", followed, num_tracks)
    }

    fn play(&mut self, queue: Arc<Queue>) {
        self.load_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            let index = queue.append_next(tracks.iter().collect());
            queue.play(index, true, true);
        }
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        self.load_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for track in tracks.iter() {
                queue.append(track);
            }
        }
    }

    fn save(&mut self, library: Arc<Library>) {
        library.follow_playlist(self);
    }

    fn unsave(&mut self, library: Arc<Library>) {
        library.delete_playlist(&self.id);
    }

    fn toggle_saved(&mut self, library: Arc<Library>) {
        // Don't allow users to unsave their own playlists with one keypress
        if !library.is_followed_playlist(self) {
            return;
        }

        if library.is_saved_playlist(self) {
            library.delete_playlist(&self.id);
        } else {
            library.follow_playlist(self);
        }
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(PlaylistView::new(queue, library, self).as_boxed_view_ext())
    }

    fn share_url(&self) -> Option<String> {
        Some(format!(
            "https://open.spotify.com/user/{}/playlist/{}",
            self.owner_id, self.id
        ))
    }
}
