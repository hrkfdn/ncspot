#![allow(clippy::use_self)]
#![deny(clippy::disallowed_methods, clippy::disallowed_types)]

use std::sync::Arc;

use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use crate::ext_traits::RwLockExt;
use crate::library::Library;
use crate::queue::Queue;
use crate::spotify::Spotify;

/// Build the D-Bus object path for a playlist with the given Spotify ID.
pub(super) fn playlist_path_for_id(id: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("/org/ncspot/playlist/{id}"))
        .unwrap_or_else(|_| OwnedObjectPath::try_from("/org/ncspot/playlist/_").unwrap())
}

/// Parse `/org/ncspot/playlist/<id>` → `id`.
pub(crate) fn parse_playlist_path(path: &ObjectPath<'_>) -> Option<String> {
    path.as_str()
        .strip_prefix("/org/ncspot/playlist/")
        .map(str::to_string)
}

/// Return a page of playlists: skip `index` items, take up to `max_count`, optionally reversed.
pub(crate) fn page_playlists(
    playlists: &[(OwnedObjectPath, String, String)],
    index: u32,
    max_count: u32,
    reverse_order: bool,
) -> Vec<(OwnedObjectPath, String, String)> {
    let start = index as usize;
    if start >= playlists.len() {
        return Vec::new();
    }
    let end = (start + max_count as usize).min(playlists.len());
    let slice = &playlists[start..end];
    if reverse_order {
        slice.iter().rev().cloned().collect()
    } else {
        slice.to_vec()
    }
}

pub struct MprisPlaylists {
    pub queue: Arc<Queue>,
    pub library: Arc<Library>,
    pub spotify: Spotify,
}

fn all_playlist_tuples(library: &Library) -> Vec<(OwnedObjectPath, String, String)> {
    library
        .playlists
        .read_unpoisoned()
        .iter()
        .map(|p| {
            (
                playlist_path_for_id(&p.id),
                p.name.clone(),
                p.cover_url.clone().unwrap_or_default(),
            )
        })
        .collect()
}

#[interface(name = "org.mpris.MediaPlayer2.Playlists")]
impl MprisPlaylists {
    /// Total number of playlists available.
    #[zbus(property)]
    fn playlist_count(&self) -> u32 {
        self.library.playlists.read_unpoisoned().len() as u32
    }

    /// Supported playlist orderings – ncspot exposes only UserDefined.
    #[zbus(property)]
    fn orderings(&self) -> Vec<String> {
        vec!["UserDefined".to_string()]
    }

    /// The playlist that is currently active (if any).
    /// Returns `(true, playlist_tuple)` when active, `(false, sentinel)` otherwise.
    #[zbus(property)]
    fn active_playlist(&self) -> (bool, (OwnedObjectPath, String, String)) {
        if let Some(id) = self.queue.active_playlist() {
            let playlists = self.library.playlists.read_unpoisoned();
            if let Some(p) = playlists.iter().find(|p| p.id == id) {
                return (
                    true,
                    (
                        playlist_path_for_id(&id),
                        p.name.clone(),
                        p.cover_url.clone().unwrap_or_default(),
                    ),
                );
            }
        }
        (
            false,
            (
                OwnedObjectPath::try_from("/").unwrap(),
                String::new(),
                String::new(),
            ),
        )
    }

    /// Load the playlist identified by `playlist_id` into the queue and start playback.
    ///
    /// This loads a *copy* of the playlist's tracks — it never mutates the saved playlist.
    async fn activate_playlist(&self, playlist_id: ObjectPath<'_>) {
        let Some(id) = parse_playlist_path(&playlist_id) else {
            return;
        };
        let playlist_copy = {
            let playlists = self.library.playlists.read_unpoisoned();
            playlists.iter().find(|p| p.id == id).cloned()
        };
        let Some(mut playlist) = playlist_copy else {
            return;
        };
        let spotify = self.spotify.clone();
        let tracks = tokio::task::spawn_blocking(move || {
            playlist.load_tracks(&spotify);
            playlist.tracks.clone()
        })
        .await
        .ok()
        .flatten();
        let Some(tracks) = tracks else {
            return;
        };
        let should_shuffle = self.queue.get_shuffle();
        self.queue.clear();
        let index = self.queue.append_next(&tracks);
        self.queue.play(index, should_shuffle, should_shuffle);
        self.queue.set_active_playlist(Some(id));
    }

    /// Return a page of playlists starting at `index`, up to `max_count`, in the given order.
    fn get_playlists(
        &self,
        index: u32,
        max_count: u32,
        _order: String,
        reverse_order: bool,
    ) -> Vec<(OwnedObjectPath, String, String)> {
        let all = all_playlist_tuples(&self.library);
        page_playlists(&all, index, max_count, reverse_order)
    }

    /// Emitted when the metadata of a playlist changes (name or icon).
    #[zbus(signal)]
    pub(super) async fn playlist_changed(
        ctx: &SignalEmitter<'_>,
        playlist: (OwnedObjectPath, String, String),
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_path(id: &str) -> OwnedObjectPath {
        playlist_path_for_id(id)
    }

    fn make_list(names: &[&str]) -> Vec<(OwnedObjectPath, String, String)> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| (make_path(&i.to_string()), name.to_string(), String::new()))
            .collect()
    }

    #[test]
    fn test_parse_playlist_path_ok() {
        let p = ObjectPath::from_static_str_unchecked("/org/ncspot/playlist/abc123");
        assert_eq!(parse_playlist_path(&p), Some("abc123".to_string()));
    }

    #[test]
    fn test_parse_playlist_path_rejects_other() {
        let p = ObjectPath::from_static_str_unchecked("/org/ncspot/queue/42");
        assert_eq!(parse_playlist_path(&p), None);
    }

    #[test]
    fn test_page_playlists_basic_skip_take() {
        let list = make_list(&["a", "b", "c", "d", "e"]);
        let result = page_playlists(&list, 1, 2, false);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].1, "b");
        assert_eq!(result[1].1, "c");
    }

    #[test]
    fn test_page_playlists_reversed() {
        let list = make_list(&["a", "b", "c", "d", "e"]);
        let result = page_playlists(&list, 0, 3, true);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].1, "c");
        assert_eq!(result[1].1, "b");
        assert_eq!(result[2].1, "a");
    }

    #[test]
    fn test_page_playlists_index_out_of_bounds() {
        let list = make_list(&["a", "b"]);
        let result = page_playlists(&list, 5, 10, false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_page_playlists_max_count_clamps_at_end() {
        let list = make_list(&["a", "b", "c"]);
        let result = page_playlists(&list, 1, 100, false);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].1, "b");
        assert_eq!(result[1].1, "c");
    }

    #[test]
    fn test_page_playlists_empty_input() {
        let result = page_playlists(&[], 0, 10, false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_page_playlists_reversed_with_offset() {
        let list = make_list(&["a", "b", "c", "d", "e"]);
        // skip 1, take 3 reversed → ["d", "c", "b"]
        let result = page_playlists(&list, 1, 3, true);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].1, "d");
        assert_eq!(result[1].1, "c");
        assert_eq!(result[2].1, "b");
    }
}
