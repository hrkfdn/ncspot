//! Library saves on their way to Spotify, and the store changes they make. Pure functions over a
//! store, so a save, its undo and its replay over a sync share one implementation.
#![deny(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::{HashMap, HashSet};

use crate::model::artist::Artist;
use crate::model::track::Track;

/// The name the artist list sorts by.
fn sort_name(name: &str) -> &str {
    name.strip_prefix("The ").unwrap_or(name)
}

/// Insert `artist` at its sorted position.
fn insert_sorted(artists: &mut Vec<Artist>, artist: Artist) {
    let at = artists.partition_point(|a| sort_name(&a.name) < sort_name(&artist.name));
    artists.insert(at, artist);
}

/// Neither followed nor holding a saved track: no reason to list it.
fn unwanted(artist: &Artist) -> bool {
    !artist.is_followed && artist.tracks.as_ref().is_none_or(Vec::is_empty)
}

/// Add the saved `track` to the front of each of its artists' lists, adding missing artists.
pub fn add_track(artists: &mut Vec<Artist>, track: &Track) {
    for (id, name) in track.artist_ids.iter().zip(track.artists.iter()) {
        if !artists.iter().any(|a| a.id.as_ref() == Some(id)) {
            let mut artist = Artist::new(id.clone(), name.clone());
            artist.tracks = Some(Vec::new());
            insert_sorted(artists, artist);
        }
        if let Some(artist) = artists.iter_mut().find(|a| a.id.as_ref() == Some(id)) {
            let tracks = artist.tracks.get_or_insert_with(Vec::new);
            if !tracks.iter().any(|t| t.id == track.id) {
                tracks.insert(0, track.clone());
            }
        }
    }
}

/// Remove `track` from its artists' lists, dropping an artist with no reason left to list it.
pub fn remove_track(artists: &mut Vec<Artist>, track: &Track) {
    for id in &track.artist_ids {
        if let Some(at) = artists.iter().position(|a| a.id.as_ref() == Some(id)) {
            if let Some(tracks) = artists[at].tracks.as_mut() {
                tracks.retain(|t| t.id != track.id);
            }
            if unwanted(&artists[at]) {
                artists.remove(at);
            }
        }
    }
}

/// Mark `artist` followed or not. An artist added here gets an empty list: only saved tracks
/// play from the Artists tab, not the tracks a search result carried.
pub fn set_followed(artists: &mut Vec<Artist>, artist: &Artist, followed: bool) {
    match artists.iter().position(|a| a.id == artist.id) {
        Some(at) => {
            artists[at].is_followed = followed;
            if unwanted(&artists[at]) {
                artists.remove(at);
            }
        }
        None if followed => {
            let mut artist = artist.clone();
            artist.is_followed = true;
            artist.tracks = Some(Vec::new());
            insert_sorted(artists, artist);
        }
        None => {}
    }
}

/// The artist list from scratch: the followed artists of `stored` plus the artists of `tracks`,
/// sorted, each with its saved tracks only.
pub fn rebuild_artists(stored: &[Artist], tracks: &[Track]) -> Vec<Artist> {
    let mut artists: Vec<Artist> = stored.iter().filter(|a| a.is_followed).cloned().collect();
    let mut known: HashSet<String> = artists.iter().filter_map(|a| a.id.clone()).collect();
    for track in tracks {
        for (id, name) in track.artist_ids.iter().zip(track.artists.iter()) {
            if known.insert(id.clone()) {
                artists.push(Artist::new(id.clone(), name.clone()));
            }
        }
    }
    // Make sure only saved tracks are played when playing artists
    for artist in &mut artists {
        artist.tracks = Some(Vec::new());
    }
    artists.sort_unstable_by(|a, b| sort_name(&a.name).cmp(sort_name(&b.name)));
    let rows: HashMap<String, usize> = artists
        .iter()
        .enumerate()
        .filter_map(|(row, a)| a.id.clone().map(|id| (id, row)))
        .collect();
    for track in tracks {
        for id in &track.artist_ids {
            if let Some(&row) = rows.get(id)
                && let Some(list) = artists[row].tracks.as_mut()
                && !list.iter().any(|t| t.id == track.id)
            {
                list.push(track.clone());
            }
        }
    }
    artists
}

#[cfg(test)]
pub mod test_items {
    use crate::model::artist::Artist;
    use crate::model::track::Track;

    pub fn track(id: &str, artists: &[(&str, &str)]) -> Track {
        Track {
            id: Some(id.to_string()),
            uri: format!("spotify:track:{id}"),
            title: id.to_string(),
            track_number: 1,
            disc_number: 1,
            duration: 1,
            artists: artists.iter().map(|a| a.1.to_string()).collect(),
            artist_ids: artists.iter().map(|a| a.0.to_string()).collect(),
            album: None,
            album_id: None,
            album_artists: Vec::new(),
            cover_url: None,
            url: String::new(),
            added_at: None,
            list_index: 0,
            is_local: false,
            is_playable: None,
        }
    }

    pub fn artist(id: &str, name: &str, followed: bool) -> Artist {
        let mut artist = Artist::new(id.to_string(), name.to_string());
        artist.is_followed = followed;
        artist.tracks = Some(Vec::new());
        artist
    }
}

#[cfg(test)]
mod tests {
    use super::test_items::{artist, track};
    use super::*;

    /// (id, followed, sorted track ids), sorted: the order of equal names and of an artist's
    /// tracks is not part of the contract.
    fn shape(artists: &[Artist]) -> Vec<(String, bool, Vec<String>)> {
        let mut out: Vec<_> = artists
            .iter()
            .map(|a| {
                let mut ids: Vec<String> = a
                    .tracks
                    .iter()
                    .flatten()
                    .filter_map(|t| t.id.clone())
                    .collect();
                ids.sort();
                (a.id.clone().unwrap_or_default(), a.is_followed, ids)
            })
            .collect();
        out.sort();
        out
    }

    fn names(artists: &[Artist]) -> Vec<&str> {
        artists.iter().map(|a| a.name.as_str()).collect()
    }

    #[test]
    fn a_saved_track_adds_its_artists_in_sorted_place() {
        let mut artists = vec![artist("b", "Beta", true), artist("z", "Zed", false)];
        add_track(&mut artists, &track("t", &[("c", "The Clash")]));
        assert_eq!(
            names(&artists),
            ["Beta", "The Clash", "Zed"],
            "The is ignored"
        );
        assert_eq!(artists[1].tracks.as_ref().unwrap().len(), 1);
        add_track(&mut artists, &track("t", &[("c", "The Clash")]));
        assert_eq!(artists[1].tracks.as_ref().unwrap().len(), 1, "no duplicate");
    }

    #[test]
    fn a_new_track_goes_first_in_its_artists_list() {
        let mut artists = vec![];
        add_track(&mut artists, &track("old", &[("a", "A")]));
        add_track(&mut artists, &track("new", &[("a", "A")]));
        let first = artists[0].tracks.as_ref().unwrap()[0].id.as_deref();
        assert_eq!(first, Some("new"));
    }

    #[test]
    fn removing_the_last_track_drops_an_unfollowed_artist_only() {
        let t = track("t", &[("a", "A"), ("b", "B")]);
        let mut artists = vec![artist("a", "A", true), artist("b", "B", false)];
        add_track(&mut artists, &t);
        remove_track(&mut artists, &t);
        assert_eq!(names(&artists), ["A"]);
        assert!(artists[0].tracks.as_ref().unwrap().is_empty());
    }

    #[test]
    fn follow_and_unfollow_keep_only_artists_with_a_reason() {
        let mut artists = vec![];
        let mut found = artist("a", "A", false);
        found.tracks = Some(vec![track("top", &[("a", "A")])]);
        set_followed(&mut artists, &found, true);
        assert!(artists[0].is_followed);
        assert!(
            artists[0].tracks.as_ref().unwrap().is_empty(),
            "a search result's tracks are not saved tracks"
        );
        set_followed(&mut artists, &found, false);
        assert!(artists.is_empty(), "no tracks and not followed");

        add_track(&mut artists, &track("t", &[("a", "A")]));
        set_followed(&mut artists, &found, true);
        set_followed(&mut artists, &found, false);
        assert_eq!(artists.len(), 1, "saved tracks keep it");
        assert!(!artists[0].is_followed);
    }

    #[test]
    fn incremental_updates_equal_a_full_rebuild() {
        let t1 = track("t1", &[("a", "Abba"), ("b", "The Band")]);
        let t2 = track("t2", &[("b", "The Band")]);
        let t3 = track("t3", &[("c", "Cream")]);
        let followed = artist("f", "Faust", true);
        let mut tracks: Vec<Track> = Vec::new();
        let mut artists: Vec<Artist> = Vec::new();
        for t in [&t1, &t2, &t3] {
            tracks.insert(0, t.clone());
            add_track(&mut artists, t);
        }
        set_followed(&mut artists, &followed, true);
        tracks.retain(|t| t.id != t2.id);
        remove_track(&mut artists, &t2);
        tracks.retain(|t| t.id != t3.id);
        remove_track(&mut artists, &t3);
        assert_eq!(shape(&artists), shape(&rebuild_artists(&artists, &tracks)));
        assert_eq!(
            names(&artists),
            names(&rebuild_artists(&artists, &tracks)),
            "same order"
        );
    }

    #[test]
    fn a_rebuild_drops_unfollowed_artists_without_tracks() {
        let stored = [artist("x", "Gone", false), artist("f", "Kept", true)];
        let rebuilt = rebuild_artists(&stored, &[track("t", &[("a", "New")])]);
        assert_eq!(names(&rebuilt), ["Kept", "New"]);
    }
}
