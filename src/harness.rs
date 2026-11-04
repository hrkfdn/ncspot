#![deny(clippy::disallowed_methods, clippy::disallowed_types)]

//! Drive harness (feature `harness`): IPC commands that open a view by URI and write a JSON
//! snapshot of the current view, so a script can drive ncspot and check what it shows.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use log::error;
use serde::Serialize;

use crate::activity::ACTIVITY;
use crate::command::ShiftMode;
use crate::ext_traits::RwLockExt;
use crate::internal_api::{Area, AreaCounts};
use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::episode::Episode;
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::model::show::Show;
use crate::model::track::Track;
use crate::playlist_edit;
use crate::playlist_items;
use crate::queue::Queue;
use crate::rate_limit::{ClientLabel, Stamp};
use crate::spotify::{PlayerEvent, Spotify, UriType};
use crate::spotify_api::WebCalls;
use crate::spotify_url::SpotifyUrl;
use crate::test_playlists;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::{album::AlbumView, artist::ArtistView, playlist::PlaylistView, show::ShowView};

/// One list in the current view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TabState {
    pub name: String,
    /// Rows loaded so far.
    pub rows: usize,
    /// Rows that can be played.
    pub playable: usize,
    /// Total rows the source reports, `None` when the list has no pagination (fully loaded).
    pub max: Option<usize>,
    /// Whether a page is being fetched.
    pub busy: bool,
}

/// What a view reports to the harness.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HarnessState {
    pub view: String,
    /// Name of the selected tab, for tabbed views.
    pub tab: Option<String>,
    pub tabs: Vec<TabState>,
}

/// Internal API counters per area.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Areas {
    pub metadata: AreaCounts,
    pub library: AreaCounts,
    pub playlists: AreaCounts,
}

/// Which clients are inside a rate-limit window.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Throttled {
    pub shared: bool,
    pub personal: bool,
    pub internal: bool,
}

/// The snapshot `harness dump` writes. Field order is the JSON order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dump {
    /// Increases on every dump, so a reader can tell a fresh file from a stale one.
    pub seq: u64,
    pub view: Option<String>,
    pub title: String,
    pub tab: Option<String>,
    pub tabs: Vec<TabState>,
    pub in_flight: usize,
    /// Playlist writes queued or running on the background worker.
    pub pending_writes: usize,
    /// Milliseconds since the last call ended; `None` before the first one.
    pub quiet_ms: Option<u64>,
    /// The last command's error, as shown in the status bar.
    pub error: Option<String>,
    pub areas: Areas,
    pub web: WebCalls,
    pub throttled: Throttled,
    /// Whether the first library sync finished.
    pub library_done: bool,
    pub playlists: PlaylistCounts,
    pub player: PlayerState,
    pub session: SessionState,
    /// Names of the recorded test playlists, in file order; filled only in the first dump after
    /// `harness test-playlists`, empty otherwise.
    pub test_playlists: Vec<String>,
}

/// How many playlists the library holds and how many of them the Playlists tab layout lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlaylistCounts {
    pub stored: usize,
    /// Number of `LayoutEntry::Playlist` entries.
    pub layout: usize,
    /// Track count of the most recently recorded test playlist in the store; `None` when there
    /// is none (or it is not stored).
    pub last_test_len: Option<usize>,
    /// The most recent test playlist's tracks as letters (`A`, `B`, `C`: the public test
    /// tracks in order; `?`: any other); `None` as for `last_test_len`.
    pub last_test_order: Option<String>,
    /// Whether the most recent test playlist's name ends in `-renamed`; `None` as for
    /// `last_test_len`.
    pub last_test_renamed: Option<bool>,
}

/// What the player is doing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerState {
    /// One of `playing`, `paused`, `stopped`, `finished`.
    pub status: String,
    pub position_ms: u64,
}

/// The playback session's health.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionState {
    pub valid: bool,
    pub reconnects: u64,
}

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Snapshot the view state plus the counters of the web API, the player and the session.
pub fn build_dump(
    title: String,
    state: Option<HarnessState>,
    error: Option<String>,
    spotify: &Spotify,
    library_done: bool,
    playlists: PlaylistCounts,
    pending_writes: usize,
) -> Dump {
    let api = &spotify.api;
    let limited = api.rate_limit_status();
    let throttled = |label: ClientLabel| limited.iter().any(|(l, _)| *l == label);
    let (view, tab, tabs) = match state {
        Some(state) => (Some(state.view), state.tab, state.tabs),
        None => (None, None, Vec::new()),
    };
    Dump {
        seq: SEQ.fetch_add(1, Ordering::Relaxed) + 1,
        view,
        title,
        tab,
        tabs,
        in_flight: ACTIVITY.in_flight(),
        pending_writes,
        quiet_ms: ACTIVITY
            .quiet_for(Stamp::now())
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
        error,
        areas: Areas {
            metadata: api.area_counts(Area::Metadata),
            library: api.area_counts(Area::Library),
            playlists: api.area_counts(Area::Playlists),
        },
        web: api.web_calls(),
        throttled: Throttled {
            shared: throttled(ClientLabel::Shared),
            personal: throttled(ClientLabel::Personal),
            internal: throttled(ClientLabel::Internal),
        },
        library_done,
        playlists,
        player: PlayerState {
            status: match spotify.get_current_status() {
                PlayerEvent::Playing(_) => "playing",
                PlayerEvent::Paused(_) => "paused",
                PlayerEvent::Stopped => "stopped",
                PlayerEvent::FinishedTrack => "finished",
            }
            .into(),
            position_ms: u64::try_from(spotify.get_current_progress().as_millis())
                .unwrap_or(u64::MAX),
        },
        session: SessionState {
            valid: spotify.session_valid(),
            reconnects: spotify.reconnects(),
        },
        test_playlists: Vec::new(),
    }
}

/// Set by `harness test-playlists`, taken by the next dump.
static WANT_TEST_PLAYLISTS: AtomicBool = AtomicBool::new(false);

/// Ask for the recorded test playlists' names in the next dump.
pub fn request_test_playlists() {
    WANT_TEST_PLAYLISTS.store(true, Ordering::Relaxed);
}

/// Whether the next dump should carry the names; clears the request.
pub fn take_test_playlists_request() -> bool {
    WANT_TEST_PLAYLISTS.swap(false, Ordering::Relaxed)
}

/// The playlist id of a recorded URI.
fn recorded_id(uri: &str) -> Option<String> {
    SpotifyUrl::parse_any(uri)
        .filter(|u| matches!(u.uri_type, UriType::Playlist))
        .map(|u| u.id)
}

/// Create the next `ncspot-test-<k>` playlist through the library and record its URI at once.
/// Errors carry no ID or URI.
pub fn new_playlist_test(library: &Library) -> Result<(), String> {
    let path = test_playlists::file_path();
    let recorded = test_playlists::read(&path)
        .map_err(|e| format!("harness newplaylist-test: cannot read the list: {e}"))?;
    let name = format!("ncspot-test-{}", recorded.len() + 1);
    // A create whose rootlist add failed returns None and is not recorded, on purpose: that
    // playlist exists but is not in the library. A failed create has already posted its note,
    // which is also the dump's error, so no second error is returned here.
    let Some(id) = library.create_playlist(&name) else {
        return Ok(());
    };
    test_playlists::append(&path, &format!("spotify:playlist:{id}")).map_err(|e| {
        error!("recording a test playlist failed: {e}");
        format!("harness newplaylist-test: created {name} but could not record it: {e}")
    })
}

/// The most recently recorded test playlist as stored in the library.
fn last_test_playlist(library: &Library, verb: &str) -> Result<Playlist, String> {
    let recorded = test_playlists::read(&test_playlists::file_path())
        .map_err(|e| format!("harness {verb}: cannot read the list: {e}"))?;
    let id = recorded
        .last()
        .and_then(|uri| recorded_id(uri))
        .ok_or_else(|| format!("harness {verb}: no recorded test playlist"))?;
    library
        .playlists
        .read_unpoisoned()
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| format!("harness {verb}: the last test playlist is not in the library"))
}

/// Track count of the most recently recorded test playlist in the store, for the dump.
pub fn last_test_len(library: &Library) -> Option<usize> {
    let playlist = last_test_playlist(library, "dump").ok()?;
    Some(
        playlist
            .tracks
            .as_ref()
            .map_or(playlist.num_tracks, Vec::len),
    )
}

/// The public catalog tracks the playlist-write verbs use.
const PUBLIC_TEST_TRACKS: [&str; 3] = [
    "spotify:track:4uLU6hMCjMI75M1A2tKUQC",
    "spotify:track:7ouMYWpwJ422jRcDASZB7P",
    "spotify:track:0VjIjW4GlUZAMYd2vXMi3b",
];

/// The first `n` public test tracks, repeating from the first when `n` is above three.
fn public_test_tracks(n: usize) -> Vec<Playable> {
    (0..n)
        .map(|i| playlist_items::placeholder(PUBLIC_TEST_TRACKS[i % PUBLIC_TEST_TRACKS.len()]))
        .collect()
}

/// Append the first `n` public test tracks to the most recent test playlist.
pub fn playlist_add_test(library: &Library, n: usize) -> Result<(), String> {
    let playlist = last_test_playlist(library, "playlist-add-test")?;
    library.queue_add(&playlist.id, &public_test_tracks(n));
    Ok(())
}

/// Remove the track at `index` from the most recent test playlist.
pub fn playlist_remove_test(library: &Library, index: usize) -> Result<(), String> {
    let playlist = last_test_playlist(library, "playlist-remove-test")?;
    let len = playlist.tracks.as_ref().map_or(0, Vec::len);
    if index >= len {
        return Err(format!(
            "harness playlist-remove-test: no track {index} ({len} loaded)"
        ));
    }
    let row = playlist.tracks.as_ref().map(|tracks| tracks[index].clone());
    let row = row.ok_or("harness playlist-remove-test: no tracks loaded")?;
    library.queue_remove(&playlist.id, &row)
}

/// Replace the contents of the most recent test playlist with the first `n` public test tracks.
pub fn playlist_overwrite_test(library: &Library, n: usize) -> Result<(), String> {
    let playlist = last_test_playlist(library, "playlist-overwrite-test")?;
    library.queue_overwrite(&playlist.id, &public_test_tracks(n));
    Ok(())
}

/// Name suffix `playlist-rename-test` adds.
/// How long the harness waits for a library save to settle.
const SAVE_SETTLE: Duration = Duration::from_secs(120);
const RENAMED_SUFFIX: &str = "-renamed";

/// Move the track at `index` of the most recent test playlist by `n` rows (negative: up),
/// through the same path as `Shift` in a playlist view.
pub fn playlist_move_test(library: &Library, index: usize, n: i32) -> Result<(), String> {
    let playlist = last_test_playlist(library, "playlist-move-test")?;
    let row = playlist
        .tracks
        .as_ref()
        .and_then(|tracks| tracks.get(index))
        .cloned()
        .ok_or_else(|| format!("harness playlist-move-test: no track {index}"))?;
    let (up, steps) = crate::ui::playlist::shift_direction(&ShiftMode::Down, n);
    library
        .queue_move(&playlist.id, &row, steps, up)
        .map(|_| ())
        .ok_or_else(|| "harness playlist-move-test: the move failed".to_string())
}

/// Rename the most recent test playlist to its name plus [RENAMED_SUFFIX].
pub fn playlist_rename_test(library: &Library) -> Result<(), String> {
    let playlist = last_test_playlist(library, "playlist-rename-test")?;
    if playlist.name.ends_with(RENAMED_SUFFIX) {
        return Err("harness playlist-rename-test: already renamed".into());
    }
    let name = format!("{}{RENAMED_SUFFIX}", playlist.name);
    match library.queue_rename(&playlist.id, &name) {
        Some(note) => Err(format!("harness playlist-rename-test: {note}")),
        None => Ok(()),
    }
}

/// `A`, `B`, `C` for the public test tracks in order, `?` for any other track.
fn track_letter(uri: &str) -> char {
    PUBLIC_TEST_TRACKS
        .iter()
        .position(|track| *track == uri)
        .and_then(|i| u8::try_from(i).ok())
        .map_or('?', |i| char::from(b'A' + i))
}

/// The most recent test playlist's tracks as letters ([track_letter]), for the dump.
pub fn last_test_order(library: &Library) -> Option<String> {
    let playlist = last_test_playlist(library, "dump").ok()?;
    Some(
        playlist
            .tracks
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|playable| track_letter(&playable.uri()))
            .collect(),
    )
}

/// Whether the most recent test playlist's name ends in [RENAMED_SUFFIX], for the dump.
pub fn last_test_renamed(library: &Library) -> Option<bool> {
    let playlist = last_test_playlist(library, "dump").ok()?;
    Some(playlist.name.ends_with(RENAMED_SUFFIX))
}

/// Names of the recorded test playlists, in file order: from the store, else the internal head
/// read, else `(unknown)`.
pub fn test_playlist_names(library: &Library, spotify: &Spotify) -> Result<Vec<String>, String> {
    let recorded = test_playlists::read(&test_playlists::file_path())
        .map_err(|e| format!("harness test-playlists: cannot read the list: {e}"))?;
    Ok(recorded
        .iter()
        .map(|uri| {
            let Some(id) = recorded_id(uri) else {
                return "(unknown)".to_string();
            };
            let stored = library
                .playlists
                .read_unpoisoned()
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.clone());
            stored
                .or_else(|| spotify.api.playlist_head(&id).ok().and_then(|h| h.name))
                .unwrap_or_else(|| "(unknown)".to_string())
        })
        .collect())
}

/// What a server rootlist read says about a recorded playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootlistState {
    Listed,
    /// A complete read that does not list it.
    Absent,
    /// The read failed or was incomplete.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineDecision {
    KeepLine,
    DropLine,
}

/// Whether to drop a recorded line after an unfollow attempt. Only a playlist known to be out of
/// the library (deleted from the store, unfollowed, or verified absent) loses its line.
fn line_decision(
    was_stored: bool,
    still_stored: bool,
    listing: RootlistState,
    unfollowed: bool,
) -> LineDecision {
    let gone = if was_stored {
        !still_stored
    } else {
        match listing {
            RootlistState::Absent => true,
            RootlistState::Listed => unfollowed,
            RootlistState::Unknown => false,
        }
    };
    if gone {
        LineDecision::DropLine
    } else {
        LineDecision::KeepLine
    }
}

/// Unfollow the n-th (1-based) recorded test playlist through the normal delete path (which
/// also unpins), then drop its line. Refuses an `n` out of range, and keeps the line when the
/// playlist is still in the library afterwards. Errors carry no ID or URI.
pub fn unfollow_recorded(library: &Library, spotify: &Spotify, n: usize) -> Result<(), String> {
    let path = test_playlists::file_path();
    let recorded = test_playlists::read(&path)
        .map_err(|e| format!("harness unfollow-recorded: cannot read the list: {e}"))?;
    let uri = n
        .checked_sub(1)
        .and_then(|i| recorded.get(i))
        .ok_or_else(|| {
            format!(
                "harness unfollow-recorded: no recorded playlist {n} ({} recorded)",
                recorded.len()
            )
        })?;
    let id =
        recorded_id(uri).ok_or("harness unfollow-recorded: the recorded line is not a playlist")?;
    if !*library.is_done.read_unpoisoned() {
        return Err("harness unfollow-recorded: the library is still syncing".into());
    }
    let stored = |library: &Library| {
        library
            .playlists
            .read_unpoisoned()
            .iter()
            .any(|p| p.id == id)
    };
    let was_stored = stored(library);
    let mut still_stored = false;
    let mut listing = RootlistState::Unknown;
    let mut unfollowed = false;
    if was_stored {
        library.delete_playlist(&id);
        still_stored = stored(library);
    } else {
        // Absent from the store is not absent from the library (partial sync, stale cache):
        // ask the server before dropping the record. These reads are unspaced; fine for the
        // handful of recorded playlists.
        let api = &spotify.api;
        if let Ok(rootlist) = api.playlist_rootlist() {
            let complete = rootlist.read >= rootlist.length;
            listing = match (playlist_edit::rootlist_index(&rootlist, &id), complete) {
                (Some(_), _) => RootlistState::Listed,
                (None, true) => RootlistState::Absent,
                (None, false) => RootlistState::Unknown,
            };
        }
        if listing == RootlistState::Listed {
            unfollowed = playlist_edit::unfollow(api, &id).is_ok();
            if unfollowed && library.unpin_deleted(&id).is_some() {
                log::warn!("harness unfollow-recorded: unfollowed, but the pin may remain");
            }
        }
    }
    match line_decision(was_stored, still_stored, listing, unfollowed) {
        LineDecision::DropLine => {}
        LineDecision::KeepLine if was_stored => {
            return Err("harness unfollow-recorded: library unchanged".into());
        }
        LineDecision::KeepLine if listing == RootlistState::Unknown => {
            return Err(
                "harness unfollow-recorded: could not confirm; not removed from the list".into(),
            );
        }
        LineDecision::KeepLine => {
            return Err("harness unfollow-recorded: unfollow failed".into());
        }
    }
    test_playlists::remove(&path, n).map_err(|e| {
        format!("harness unfollow-recorded: unfollowed but could not update the list: {e}")
    })?;
    Ok(())
}

/// Write `dump` to `path` atomically: a reader sees the old file or the new one, never half.
pub fn write_dump(path: &Path, dump: &Dump) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    let json = serde_json::to_vec(dump).map_err(std::io::Error::other)?;
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(&json)?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)
}

/// Parse the target of `harness open`.
pub fn parse_target(target: &str) -> Result<SpotifyUrl, String> {
    SpotifyUrl::parse_any(target)
        .ok_or_else(|| format!("harness open: not a Spotify URI or URL: {target}"))
}

/// Build the view for `target` the way `goto` does, on the calling (UI) thread: an album,
/// artist, show or playlist opens its own view, a track its album, an episode its show.
pub fn open_view(
    queue: Arc<Queue>,
    library: Arc<Library>,
    target: &str,
) -> Result<Box<dyn ViewExt>, String> {
    let url = parse_target(target)?;
    let spotify = queue.get_spotify();
    let api = &spotify.api;
    let failed = |what: &str| format!("harness open: could not load {what} {}", url.id);
    let view = match url.uri_type {
        UriType::Album => {
            let album = api.album(&url.id).map_err(|_| failed("album"))?;
            AlbumView::new(queue, library, &Album::from(&album)).into_boxed_view_ext()
        }
        UriType::Track => {
            let track = api.track(&url.id).map_err(|_| failed("track"))?;
            let album = Track::from(&track)
                .album(&queue)
                .ok_or_else(|| failed("album of track"))?;
            AlbumView::new(queue, library, &album).into_boxed_view_ext()
        }
        UriType::Artist => {
            let artist = api.artist(&url.id).map_err(|_| failed("artist"))?;
            ArtistView::new(queue, library, &Artist::from(&artist)).into_boxed_view_ext()
        }
        UriType::Show => {
            let show = api.show(&url.id).map_err(|_| failed("show"))?;
            ShowView::new(queue, library, &Show::from(&show)).into_boxed_view_ext()
        }
        UriType::Episode => {
            let episode = api.episode(&url.id).map_err(|_| failed("episode"))?;
            ShowView::new(queue, library, &Show::from(&episode.show)).into_boxed_view_ext()
        }
        UriType::Playlist => {
            let playlist = api.playlist(&url.id).map_err(|_| failed("playlist"))?;
            PlaylistView::new(queue, library, &Playlist::from(&playlist)).into_boxed_view_ext()
        }
    };
    Ok(view)
}

/// A library membership check and its write, for one item.
type ItemOps<'a> = (Box<dyn Fn() -> bool + 'a>, Box<dyn Fn() + 'a>);

/// Save (or remove, with `removed`) one item through the library, as the UI would. Refuses to
/// save an item already in the library, so a test that saves then removes never removes
/// something the user saved, and refuses to remove an item that is not in it. Fails when the
/// library did not change (the write failed). Errors carry no URI or ID.
pub fn write_item(
    queue: Arc<Queue>,
    library: Arc<Library>,
    target: &str,
    removed: bool,
) -> Result<(), String> {
    let url = parse_target(target)?;
    let spotify = queue.get_spotify();
    let api = &spotify.api;
    let verb = if removed {
        "harness unsave"
    } else {
        "harness save"
    };
    let failed = |what: &str| format!("{verb}: could not load {what}");
    let lib = &library;
    let (is_saved, write): ItemOps = match url.uri_type {
        UriType::Track => {
            let track = Track::from(&api.track(&url.id).map_err(|_| failed("track"))?);
            let playable = Playable::Track(track.clone());
            (
                Box::new(move || lib.is_saved_track(&playable)),
                Box::new(move || {
                    if removed {
                        lib.unsave_tracks(&[&track]);
                    } else {
                        lib.save_tracks(&[&track]);
                    }
                }),
            )
        }
        UriType::Album => {
            let album = Album::from(&api.album(&url.id).map_err(|_| failed("album"))?);
            let check = album.clone();
            (
                Box::new(move || lib.is_saved_album(&check)),
                Box::new(move || {
                    if removed {
                        lib.unsave_album(&album);
                    } else {
                        lib.save_album(&album);
                    }
                }),
            )
        }
        UriType::Artist => {
            let artist = Artist::from(&api.artist(&url.id).map_err(|_| failed("artist"))?);
            let check = artist.clone();
            (
                Box::new(move || lib.is_followed_artist(&check)),
                Box::new(move || {
                    if removed {
                        lib.unfollow_artist(&artist);
                    } else {
                        lib.follow_artist(&artist);
                    }
                }),
            )
        }
        UriType::Show => {
            let show = Show::from(&api.show(&url.id).map_err(|_| failed("show"))?);
            let check = show.clone();
            (
                Box::new(move || lib.is_saved_show(&check)),
                Box::new(move || {
                    if removed {
                        lib.unsave_show(&show);
                    } else {
                        lib.save_show(&show);
                    }
                }),
            )
        }
        UriType::Episode => {
            let episode = Episode::from(&api.episode(&url.id).map_err(|_| failed("episode"))?);
            let check = episode.clone();
            (
                Box::new(move || lib.is_saved_episode(&check) == Some(true)),
                Box::new(move || {
                    if removed {
                        lib.unsave_episode(&episode);
                    } else {
                        lib.save_episode(&episode);
                    }
                }),
            )
        }
        UriType::Playlist => return Err(format!("{verb}: playlists are not a library set")),
    };
    let before = is_saved();
    if !removed && before {
        return Err(format!("{verb}: already in the library"));
    }
    if removed && !before {
        return Err(format!("{verb}: not in the library"));
    }
    write();
    // The store shows a save at once: only the settled write says it landed. The longest a save
    // can take is about 90 s.
    library.writer.wait_idle(SAVE_SETTLE);
    if is_saved() == before {
        return Err(format!("{verb}: library unchanged"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_accepts_every_uri_kind_and_urls() {
        let cases = [
            ("spotify:album:a1", "a1"),
            ("spotify:artist:a2", "a2"),
            ("spotify:show:s1", "s1"),
            ("spotify:playlist:p1", "p1"),
            ("spotify:track:t1", "t1"),
            ("spotify:episode:e1", "e1"),
            ("https://open.spotify.com/album/a3", "a3"),
        ];
        for (input, id) in cases {
            assert_eq!(parse_target(input).unwrap().id, id, "{input}");
        }
        assert!(matches!(
            parse_target("spotify:episode:e1").unwrap().uri_type,
            UriType::Episode
        ));
    }

    #[test]
    fn recorded_id_accepts_only_playlist_uris() {
        assert_eq!(recorded_id("spotify:playlist:p1").as_deref(), Some("p1"));
        assert_eq!(recorded_id("spotify:album:a1"), None);
        assert_eq!(recorded_id("junk"), None);
    }

    #[test]
    fn line_decision_drops_only_a_playlist_known_to_be_gone() {
        use LineDecision::{DropLine, KeepLine};
        use RootlistState::{Absent, Listed, Unknown};
        let table = [
            (true, false, Unknown, false, DropLine),
            (true, true, Unknown, false, KeepLine),
            (false, false, Absent, false, DropLine),
            (false, false, Listed, true, DropLine),
            (false, false, Listed, false, KeepLine),
            (false, false, Unknown, false, KeepLine),
        ];
        for (was, still, listing, done, expect) in table {
            assert_eq!(
                line_decision(was, still, listing, done),
                expect,
                "{was} {still} {listing:?} {done}"
            );
        }
    }

    #[test]
    fn test_playlists_request_is_taken_once() {
        request_test_playlists();
        assert!(take_test_playlists_request());
        assert!(!take_test_playlists_request());
    }

    #[test]
    fn parse_target_names_a_bad_target() {
        let err = parse_target("not-a-uri").unwrap_err();
        assert!(err.contains("not-a-uri"), "{err}");
    }

    fn sample() -> Dump {
        Dump {
            seq: 7,
            view: Some("album".into()),
            title: "T".into(),
            tab: Some("Tracks".into()),
            tabs: vec![TabState {
                name: "Tracks".into(),
                rows: 12,
                playable: 11,
                max: Some(12),
                busy: false,
            }],
            in_flight: 0,
            pending_writes: 0,
            quiet_ms: Some(850),
            error: None,
            areas: Areas {
                metadata: AreaCounts {
                    internal_ok: 14,
                    fallback: 0,
                    latched: false,
                },
                library: AreaCounts {
                    internal_ok: 3,
                    fallback: 0,
                    latched: false,
                },
                playlists: AreaCounts {
                    internal_ok: 5,
                    fallback: 1,
                    latched: false,
                },
            },
            web: WebCalls {
                shared: 0,
                personal: 2,
            },
            throttled: Throttled {
                shared: false,
                personal: false,
                internal: false,
            },
            library_done: true,
            playlists: PlaylistCounts {
                stored: 9,
                layout: 8,
                last_test_len: Some(3),
                last_test_order: Some("ABC".into()),
                last_test_renamed: Some(false),
            },
            player: PlayerState {
                status: "playing".into(),
                position_ms: 1200,
            },
            session: SessionState {
                valid: true,
                reconnects: 1,
            },
            test_playlists: Vec::new(),
        }
    }

    #[test]
    fn dump_serializes_to_the_documented_shape() {
        let json = serde_json::to_string(&sample()).unwrap();
        assert_eq!(
            json,
            r#"{"seq":7,"view":"album","title":"T","tab":"Tracks","tabs":[{"name":"Tracks","rows":12,"playable":11,"max":12,"busy":false}],"in_flight":0,"pending_writes":0,"quiet_ms":850,"error":null,"areas":{"metadata":{"internal_ok":14,"fallback":0,"latched":false},"library":{"internal_ok":3,"fallback":0,"latched":false},"playlists":{"internal_ok":5,"fallback":1,"latched":false}},"web":{"shared":0,"personal":2},"throttled":{"shared":false,"personal":false,"internal":false},"library_done":true,"playlists":{"stored":9,"layout":8,"last_test_len":3,"last_test_order":"ABC","last_test_renamed":false},"player":{"status":"playing","position_ms":1200},"session":{"valid":true,"reconnects":1},"test_playlists":[]}"#
        );
    }

    #[test]
    fn public_test_tracks_are_letters_in_order() {
        assert_eq!(track_letter(PUBLIC_TEST_TRACKS[0]), 'A');
        assert_eq!(track_letter(PUBLIC_TEST_TRACKS[2]), 'C');
        assert_eq!(track_letter("spotify:track:other"), '?');
    }

    #[test]
    fn write_dump_replaces_the_file_atomically() {
        let dir = std::env::temp_dir().join(format!("ncspot-harness-dump-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dump.json");
        write_dump(&path, &sample()).unwrap();
        let mut second = sample();
        second.seq = 8;
        write_dump(&path, &second).unwrap();
        let read: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(read["seq"], 8);
        assert!(
            !dir.join("dump.json.tmp").exists(),
            "temp file renamed away"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_dump_reads_the_counters_and_bumps_seq() {
        let cfg = crate::config::Config::new_for_test();
        let spotify = Spotify::new_for_test(cfg, crate::events::EventManager::new_for_test());
        let counts = PlaylistCounts {
            stored: 3,
            layout: 2,
            last_test_len: None,
            last_test_order: None,
            last_test_renamed: None,
        };
        let first = build_dump(
            "x".into(),
            None,
            Some("boom".into()),
            &spotify,
            false,
            counts.clone(),
            0,
        );
        let second = build_dump("x".into(), None, None, &spotify, false, counts.clone(), 0);
        assert!(!first.session.valid);
        assert_eq!(first.session.reconnects, 0);
        assert_eq!(first.player.status, "stopped");
        assert!(second.seq > first.seq);
        assert_eq!(first.error.as_deref(), Some("boom"));
        assert_eq!(first.playlists, counts);
        assert!(first.test_playlists.is_empty());
        assert_eq!(first.view, None);
        assert!(first.tabs.is_empty());
        assert_eq!(first.web, WebCalls::default());
        assert!(!first.throttled.shared && !first.throttled.personal && !first.throttled.internal);
    }
}
