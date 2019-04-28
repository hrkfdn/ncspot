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
    pub tracks: Vec<Track>,
}

impl Playlist {
    pub fn from_simplified_playlist(list: &SimplifiedPlaylist, spotify: &Spotify) -> Playlist {
        Self::_process_playlist(
            list.id.clone(),
            list.name.clone(),
            list.owner.id.clone(),
            list.snapshot_id.clone(),
            spotify,
        )
    }

    pub fn from_full_playlist(list: &FullPlaylist, spotify: &Spotify) -> Playlist {
        Self::_process_playlist(
            list.id.clone(),
            list.name.clone(),
            list.owner.id.clone(),
            list.snapshot_id.clone(),
            spotify,
        )
    }

    fn _process_playlist(
        id: String,
        name: String,
        owner_id: String,
        snapshot_id: String,
        spotify: &Spotify,
    ) -> Playlist {
        let mut collected_tracks = Vec::new();

        let mut tracks_result = spotify.user_playlist_tracks(&id, 100, 0);
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
                        &id,
                        100,
                        tracks.offset + tracks.items.len() as u32,
                    )
                }
                None => None,
            }
        }

        Playlist {
            id,
            name,
            owner_id,
            snapshot_id,
            tracks: collected_tracks,
        }
    }
}

impl ListItem for Playlist {
    fn is_playing(&self, queue: Arc<Queue>) -> bool {
        let playing: Vec<String> = queue
            .queue
            .read()
            .unwrap()
            .iter()
            .filter(|t| t.id.is_some())
            .map(|t| t.id.clone().unwrap())
            .collect();
        let ids: Vec<String> = self
            .tracks
            .iter()
            .filter(|t| t.id.is_some())
            .map(|t| t.id.clone().unwrap())
            .collect();
        !ids.is_empty() && playing == ids
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
        format!("{}{:>3} tracks", followed, self.tracks.len())
    }

    fn play(&mut self, queue: Arc<Queue>) {
        let index = queue.append_next(self.tracks.iter().collect());
        queue.play(index, true);
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        for track in self.tracks.iter() {
            queue.append(track);
        }
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
}
