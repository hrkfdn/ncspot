use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::{cmp::Ordering, iter::Iterator};

use rand::{seq::IteratorRandom, thread_rng};

use log::{debug, warn};
use rspotify::model::playlist::{FullPlaylist, SimplifiedPlaylist};
use rspotify::model::Id;

use crate::model::playable::Playable;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::{listview::ListView, playlist::PlaylistView};
use crate::{command::SortDirection, command::SortKey, library::Library};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub owner_name: Option<String>,
    pub snapshot_id: String,
    pub num_tracks: usize,
    pub tracks: Option<Vec<Playable>>,
    pub collaborative: bool,
}

impl Playlist {
    pub fn load_tracks(&mut self, spotify: Spotify) {
        if self.tracks.is_some() {
            return;
        }

        self.tracks = Some(self.get_all_tracks(spotify));
    }

    fn get_all_tracks(&self, spotify: Spotify) -> Vec<Playable> {
        let tracks_result = spotify.api.user_playlist_tracks(&self.id);
        while !tracks_result.at_end() {
            tracks_result.next();
        }

        let tracks = tracks_result.items.read().unwrap();
        tracks.clone()
    }

    pub fn has_track(&self, track_id: &str) -> bool {
        self.tracks.as_ref().map_or(false, |tracks| {
            tracks
                .iter()
                .any(|track| track.id() == Some(track_id.to_string()))
        })
    }

    pub fn delete_track(&mut self, index: usize, spotify: Spotify, library: &Library) -> bool {
        let playable = self.tracks.as_ref().unwrap()[index].clone();
        debug!("deleting track: {} {:?}", index, playable);

        if playable.track().map(|t| t.is_local) == Some(true) {
            warn!("track is a local file, can't delete");
            return false;
        }

        match spotify
            .api
            .delete_tracks(&self.id, &self.snapshot_id, &[playable])
        {
            false => false,
            true => {
                if let Some(tracks) = &mut self.tracks {
                    tracks.remove(index);
                    library.playlist_update(self);
                }

                true
            }
        }
    }

    pub fn append_tracks(&mut self, new_tracks: &[Playable], spotify: &Spotify, library: &Library) {
        let mut has_modified = false;

        if spotify.api.append_tracks(&self.id, new_tracks, None) {
            if let Some(tracks) = &mut self.tracks {
                tracks.append(&mut new_tracks.to_vec());
                has_modified = true;
            }
        }

        if has_modified {
            library.playlist_update(self);
        }
    }

    pub fn sort(&mut self, key: &SortKey, direction: &SortDirection) {
        fn compare_artists(a: &[String], b: &[String]) -> Ordering {
            let sanitize_artists_name = |x: &[String]| -> Vec<String> {
                x.iter()
                    .map(|x| {
                        x.to_lowercase()
                            .split(' ')
                            .skip_while(|x| x == &"the")
                            .collect()
                    })
                    .collect()
            };

            let a = sanitize_artists_name(a);
            let b = sanitize_artists_name(b);

            a.cmp(&b)
        }

        fn compare_album(a: &Track, b: &Track) -> Ordering {
            a.album
                .as_ref()
                .map(|x| x.to_lowercase())
                .cmp(&b.album.as_ref().map(|x| x.to_lowercase()))
                .then_with(|| a.disc_number.cmp(&b.disc_number))
                .then_with(|| a.track_number.cmp(&b.track_number))
        }

        if let Some(c) = self.tracks.as_mut() {
            c.sort_by(|a, b| match (a.track(), b.track()) {
                (Some(a), Some(b)) => {
                    let (a, b) = match *direction {
                        SortDirection::Ascending => (a, b),
                        SortDirection::Descending => (b, a),
                    };
                    match *key {
                        SortKey::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                        SortKey::Duration => a.duration.cmp(&b.duration),
                        SortKey::Album => compare_album(&a, &b),
                        SortKey::Added => a.added_at.cmp(&b.added_at),
                        SortKey::Artist => compare_artists(&a.artists, &b.artists)
                            .then_with(|| compare_album(&a, &b)),
                    }
                }
                _ => std::cmp::Ordering::Equal,
            })
        }
    }
}

impl From<&SimplifiedPlaylist> for Playlist {
    fn from(list: &SimplifiedPlaylist) -> Self {
        Playlist {
            id: list.id.id().to_string(),
            name: list.name.clone(),
            owner_id: list.owner.id.id().to_string(),
            owner_name: list.owner.display_name.clone(),
            snapshot_id: list.snapshot_id.clone(),
            num_tracks: list.tracks.total as usize,
            tracks: None,
            collaborative: list.collaborative,
        }
    }
}

impl From<&FullPlaylist> for Playlist {
    fn from(list: &FullPlaylist) -> Self {
        Playlist {
            id: list.id.id().to_string(),
            name: list.name.clone(),
            owner_id: list.owner.id.id().to_string(),
            owner_name: list.owner.display_name.clone(),
            snapshot_id: list.snapshot_id.clone(),
            num_tracks: list.tracks.total as usize,
            tracks: None,
            collaborative: list.collaborative,
        }
    }
}

impl ListItem for Playlist {
    fn is_playing(&self, queue: &Queue) -> bool {
        if let Some(tracks) = self.tracks.as_ref() {
            let playing: Vec<String> = queue
                .queue
                .read()
                .unwrap()
                .iter()
                .filter_map(|t| t.id())
                .collect();
            let ids: Vec<String> = tracks.iter().filter_map(|t| t.id()).collect();
            !ids.is_empty() && playing == ids
        } else {
            false
        }
    }

    fn display_left(&self, library: &Library) -> String {
        let hide_owners = library.cfg.values().hide_display_names.unwrap_or(false);
        match (self.owner_name.as_ref(), hide_owners) {
            (Some(owner), false) => format!("{} • {}", self.name, owner),
            _ => self.name.clone(),
        }
    }

    fn display_right(&self, library: &Library) -> String {
        let saved = if library.is_saved_playlist(self) {
            if library.cfg.values().use_nerdfont.unwrap_or(false) {
                "\u{f012c} "
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

        format!("{saved}{num_tracks:>4} tracks")
    }

    fn play(&mut self, queue: &Queue) {
        self.load_tracks(queue.get_spotify());

        if let Some(tracks) = &self.tracks {
            let index = queue.append_next(tracks);
            queue.play(index, true, true);
        }
    }

    fn play_next(&mut self, queue: &Queue) {
        self.load_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for track in tracks.iter().rev() {
                queue.insert_after_current(track.clone());
            }
        }
    }

    fn queue(&mut self, queue: &Queue) {
        self.load_tracks(queue.get_spotify());

        if let Some(tracks) = self.tracks.as_ref() {
            for track in tracks.iter() {
                queue.append(track.clone());
            }
        }
    }

    fn toggle_saved(&mut self, library: &Library) {
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

    fn save(&mut self, library: &Library) {
        library.follow_playlist(self);
    }

    fn unsave(&mut self, library: &Library) {
        library.delete_playlist(&self.id);
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(PlaylistView::new(queue, library, self).into_boxed_view_ext())
    }

    fn open_recommendations(
        &mut self,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> Option<Box<dyn ViewExt>> {
        self.load_tracks(queue.get_spotify());
        const MAX_SEEDS: usize = 5;
        let track_ids: Vec<String> = self
            .tracks
            .as_ref()?
            .iter()
            .filter_map(|t| t.id())
            // only select unique tracks
            .collect::<HashSet<_>>()
            .into_iter()
            // spotify allows at max 5 seed items, so choose them at random
            .choose_multiple(&mut thread_rng(), MAX_SEEDS);

        if track_ids.is_empty() {
            return None;
        }

        let spotify = queue.get_spotify();
        let recommendations: Option<Vec<Track>> = spotify
            .api
            .recommendations(
                None,
                None,
                Some(track_ids.iter().map(|t| t.as_ref()).collect()),
            )
            .map(|r| r.tracks)
            .map(|tracks| tracks.iter().map(Track::from).collect());

        recommendations.map(|tracks| {
            ListView::new(
                Arc::new(RwLock::new(tracks)),
                queue.clone(),
                library.clone(),
            )
            .with_title(&format!("Similar to Tracks in \"{}\"", self.name))
            .into_boxed_view_ext()
        })
    }

    fn share_url(&self) -> Option<String> {
        Some(format!(
            "https://open.spotify.com/user/{}/playlist/{}",
            self.owner_id, self.id
        ))
    }

    fn is_saved(&self, library: &Library) -> Option<bool> {
        // save status of personal playlists can't be toggled for safety
        if !library.is_followed_playlist(self) {
            return None;
        }

        Some(library.is_saved_playlist(self))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
