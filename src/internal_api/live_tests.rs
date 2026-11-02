use librespot_core::{Session, SessionConfig, cache::Cache};

use rspotify::prelude::Id;

use super::metadata;
use crate::authentication::SPOTIFY_CLIENT_ID;

/// Known ids (public catalog items that have existed for years). The album under test is the
/// track's own album, so no album id needs maintaining.
const TRACK: &str = "4uLU6hMCjMI75M1A2tKUQC";
const ARTIST: &str = "0OdUWJ0sBjDrqHygGUXeCF";
const SHOW: &str = "4rOoJ6Egrf8K2IrywzwOMk";
/// An episode Spotify pulled (status 451 on the batch path).
const REMOVED_EPISODE: &str = "2rKMiQw2A58GL5ZHH8jfHo";

/// `file` in the user's real cache folder: unit tests get a temporary cache folder
/// (`config::cache_path`), but the live tests log in with the cached credentials.
fn user_cache(file: &str) -> std::path::PathBuf {
    crate::config::user_cache_directory()
        .expect("user cache folder")
        .join(file)
}

fn with_session<T>(f: impl FnOnce(&tokio::runtime::Runtime, Session) -> T) -> T {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let cache = Cache::new(Some(user_cache("librespot")), None, None, None).unwrap();
    let credentials = cache.credentials().expect("cached librespot credentials");
    let session = rt.block_on(async {
        let session = Session::new(
            SessionConfig {
                client_id: SPOTIFY_CLIENT_ID.to_string(),
                ..Default::default()
            },
            None,
        );
        session.connect(credentials, false).await.expect("login");
        session
    });
    f(&rt, session)
}

#[test]
#[ignore = "needs cached librespot credentials and network"]
fn track_album_artist_show_load() {
    with_session(|rt, s| {
        let t = rt
            .block_on(metadata::track(s.clone(), TRACK.into()))
            .unwrap();
        assert!(!t.name.is_empty());
        let album_id = t.album.id.as_ref().unwrap().id().to_string();
        let a = rt
            .block_on(metadata::album(s.clone(), album_id.clone()))
            .unwrap();
        assert!(a.tracks.total > 0);
        assert_eq!(a.tracks.items.len(), (a.tracks.total as usize).min(50));
        let album_unplayable = a
            .tracks
            .items
            .iter()
            .filter(|t| t.is_playable == Some(false))
            .count();
        eprintln!(
            "album tracks: {}, is_playable == Some(false): {album_unplayable}",
            a.tracks.items.len()
        );
        assert!(
            a.tracks.items.iter().any(|t| t.is_playable == Some(true)),
            "no album track is playable"
        );
        let page = rt
            .block_on(metadata::album_tracks(s.clone(), album_id, 2, 1))
            .unwrap();
        assert_eq!(page.offset, 1);
        assert!(page.items.len() <= 2);
        let top = rt
            .block_on(metadata::artist_top_tracks(s.clone(), ARTIST.into()))
            .unwrap();
        assert!(!top.is_empty());
        let top_unplayable = top.iter().filter(|t| t.is_playable == Some(false)).count();
        eprintln!(
            "top tracks: {}, is_playable == Some(false): {top_unplayable}",
            top.len()
        );
        let ids = rt
            .block_on(metadata::artist_album_ids(s.clone(), ARTIST.into(), None))
            .unwrap();
        assert!(!ids.is_empty());
        let show = rt.block_on(metadata::show(s.clone(), SHOW.into())).unwrap();
        assert!(!show.episodes.items.is_empty());
        let user = rt.block_on(metadata::current_user(s)).unwrap();
        assert!(!user.id.to_string().is_empty());
    });
}

#[test]
#[ignore = "needs cached librespot credentials and network"]
fn show_episode_order_is_newest_first() {
    with_session(|rt, s| {
        let ids = rt
            .block_on(metadata::show_episode_ids(s.clone(), SHOW.into()))
            .unwrap();
        // Some ids may be pulled episodes (dropped by episodes_by_uri), so look at a window.
        let eps = rt
            .block_on(metadata::episodes_by_uri(
                s,
                ids[..5.min(ids.len())].to_vec(),
                metadata::Missing::Fail,
            ))
            .unwrap();
        assert!(eps.len() >= 2, "fewer than two episodes loaded");
        assert!(
            eps.windows(2)
                .all(|w| w[0].release_date >= w[1].release_date),
            "librespot returned episodes oldest first: reverse in show_episode_ids"
        );
    });
}

#[test]
#[ignore = "needs cached librespot credentials and network"]
fn removed_episode_is_not_found_and_does_not_latch() {
    use super::error::InternalError;
    use super::fallback::{Action, decide_after};
    with_session(|rt, s| {
        let err = rt
            .block_on(metadata::episode(s, REMOVED_EPISODE.into()))
            .unwrap_err();
        assert_eq!(err, InternalError::NotFound);
        assert_eq!(decide_after(&err), Action::WebOnce);
    });
}

#[test]
#[ignore = "needs cached librespot credentials and network"]
fn collection_sets_page_completely_and_accept_a_delta() {
    use super::collection::{
        self, Delta, Listing, SET_ARTIST, SET_COLLECTION, SET_LISTEN_LATER, SET_SHOW,
    };
    with_session(|rt, session| {
        for set in [SET_COLLECTION, SET_ARTIST, SET_SHOW, SET_LISTEN_LATER] {
            let mut listing = Listing::default();
            let mut token = String::new();
            loop {
                let page = rt
                    .block_on(collection::page(session.clone(), set, token.clone()))
                    .expect("page");
                match collection::absorb(&mut listing, page) {
                    Some(next) => token = next,
                    None => break,
                }
            }
            assert!(listing.complete(), "{set}: complete listing");
            println!(
                "{set}: {} items, {} pages",
                listing.items.len(),
                listing.pages
            );
            let sync = listing.sync_token.unwrap();
            let delta = rt
                .block_on(collection::delta(session.clone(), set, sync))
                .expect("delta");
            assert!(
                matches!(delta, Delta::Changes { .. }),
                "{set}: delta accepted"
            );
        }
    });
}

#[test]
#[ignore = "needs cached librespot credentials and network"]
fn playlist_rootlist_and_contents_load() {
    use super::collection;
    use super::playlist::{self, RootEntry};
    with_session(|rt, session| {
        let list = rt
            .block_on(playlist::rootlist(session.clone()))
            .expect("rootlist");
        let playlists: Vec<_> = list
            .entries
            .iter()
            .filter_map(|e| match e {
                RootEntry::Playlist(p) => Some(p),
                _ => None,
            })
            .collect();
        println!(
            "rootlist: {} playlists, {} entries, {} pages",
            playlists.len(),
            list.entries.len(),
            list.pages
        );
        assert!(!playlists.is_empty(), "at least one playlist");
        let username = session.username();
        let owned = playlists.iter().find(|p| p.owner_id == username);
        let spotify = playlists.iter().find(|p| p.owner_id == "spotify");
        for (label, found) in [("owned", owned), ("spotify-owned", spotify)] {
            match found {
                Some(p) => {
                    let (_, items) = rt
                        .block_on(playlist::contents(session.clone(), p.id.clone()))
                        .expect("contents");
                    println!("{label}: {} items", items.len());
                    assert!(!items.is_empty(), "{label}: items");
                }
                None => println!("{label}: none in the rootlist"),
            }
        }
        let pins = rt
            .block_on(collection::page(session.clone(), "ylpin", String::new()))
            .expect("ylpin page");
        println!("ylpin: page ok ({} items)", pins.items.len());
    });
}

/// Save `uri` to `set`, see it in a delta, remove it, see the removal. Skips (and never
/// removes) an item already in the set. Prints only the set name and the outcome.
fn write_round_trip(
    rt: &tokio::runtime::Runtime,
    session: &Session,
    set: &'static str,
    uri: String,
) {
    use super::collection::{self, Delta, Listing};
    let mut listing = Listing::default();
    let mut token = String::new();
    loop {
        let page = rt
            .block_on(collection::page(session.clone(), set, token.clone()))
            .expect("page");
        match collection::absorb(&mut listing, page) {
            Some(next) => token = next,
            None => break,
        }
    }
    if listing.items.iter().any(|i| i.uri == uri) {
        println!("{set}: test item already in the library, skipped");
        return;
    }
    let sync = listing.sync_token.expect("complete listing");
    rt.block_on(collection::write(
        session.clone(),
        set,
        vec![uri.clone()],
        false,
    ))
    .expect("save");
    let after_save = rt
        .block_on(collection::delta(session.clone(), set, sync))
        .expect("delta");
    let Delta::Changes {
        added, sync_token, ..
    } = after_save
    else {
        panic!("{set}: delta refused after save");
    };
    assert!(
        added.iter().any(|i| i.uri == uri),
        "{set}: saved item in the delta"
    );
    rt.block_on(collection::write(
        session.clone(),
        set,
        vec![uri.clone()],
        true,
    ))
    .expect("remove");
    let after_remove = rt
        .block_on(collection::delta(session.clone(), set, sync_token))
        .expect("delta");
    let Delta::Changes { removed, .. } = after_remove else {
        panic!("{set}: delta refused after remove");
    };
    assert!(removed.contains(&uri), "{set}: removal in the delta");
    println!("{set}: save and remove round trip ok");
}

#[test]
#[ignore = "needs cached librespot credentials and network; writes to the account (net no change)"]
fn collection_write_saves_and_removes() {
    with_session(|rt, session| {
        write_round_trip(
            rt,
            &session,
            super::collection::SET_COLLECTION,
            format!("spotify:track:{TRACK}"),
        );
        let ids = rt
            .block_on(super::metadata::show_episode_ids(
                session.clone(),
                SHOW.into(),
            ))
            .expect("episode ids");
        let episode_uri = ids.first().expect("an episode").to_uri();
        write_round_trip(
            rt,
            &session,
            super::collection::SET_LISTEN_LATER,
            episode_uri,
        );
    });
}

/// Create `ncspot-test-<k>`, record it at once (a test never deletes it) and add it to the
/// top of the rootlist, verified by the rootlist length; returns `k` and its id.
fn create_recorded(
    rt: &tokio::runtime::Runtime,
    s: &Session,
    spacing: std::time::Duration,
) -> (usize, String) {
    use super::playlist;
    use super::playlist_write as write;
    use std::thread::sleep;

    // Create and record at once: the playlist is never deleted by a test.
    let path = crate::test_playlists::file_path();
    let k = crate::test_playlists::read(&path)
        .expect("read the test-playlists file")
        .len()
        + 1;
    sleep(spacing);
    let created = rt
        .block_on(write::create(s.clone(), format!("ncspot-test-{k}")))
        .map_err(|e| e.class())
        .expect("create");
    crate::test_playlists::append(&path, &format!("spotify:playlist:{}", created.id))
        .expect("record the test playlist");
    println!("created ncspot-test-{k}, recorded");
    let id = created.id.clone();

    // Rootlist add, with the head read before and after.
    let before = rt
        .block_on(playlist::rootlist_head(s.clone()))
        .map_err(|e| e.class())
        .expect("rootlist head");
    let has_revision = before.revision.is_some();
    let has_length = before.length.is_some();
    println!("rootlist zero-length read: revision {has_revision}, length {has_length}");
    let (base_revision, base_len) = match (before.revision, before.length) {
        (Some(revision), Some(length)) => (revision, length),
        _ => {
            let full = rt
                .block_on(playlist::rootlist(s.clone()))
                .map_err(|e| e.class())
                .expect("rootlist");
            (full.revision, full.length)
        }
    };
    sleep(spacing);
    rt.block_on(write::rootlist_changes(
        s.clone(),
        write::list_changes(
            base_revision,
            vec![write::rootlist_add_op(&format!("spotify:playlist:{id}"))],
        ),
    ))
    .map_err(|e| e.class())
    .expect("rootlist add");
    let after = rt
        .block_on(playlist::rootlist_head(s.clone()))
        .map_err(|e| e.class())
        .expect("rootlist head after");
    let after_len = match after.length {
        Some(len) => len,
        None => {
            rt.block_on(playlist::rootlist(s.clone()))
                .map_err(|e| e.class())
                .expect("rootlist")
                .length
        }
    };
    assert_eq!(after_len, base_len + 1, "rootlist grew by one");
    println!("rootlist add verified");
    (k, id)
}

#[test]
#[ignore = "needs cached librespot credentials and network; creates a test playlist (never deleted)"]
fn playlist_write_round_trip() {
    use super::playlist::{self, Head};
    use super::playlist_write::{self as write, OVERWRITE_ATOMIC_MAX};
    use std::thread::sleep;
    use std::time::Duration;

    const TRACKS: [&str; 3] = [
        "spotify:track:4uLU6hMCjMI75M1A2tKUQC",
        "spotify:track:7ouMYWpwJ422jRcDASZB7P",
        "spotify:track:0VjIjW4GlUZAMYd2vXMi3b",
    ];
    let spacing = Duration::from_millis(300);
    let track = |i: usize| TRACKS[i].to_string();

    with_session(|rt, s| {
        let (_, id) = create_recorded(rt, &s, spacing);

        let head = |rt: &tokio::runtime::Runtime| -> Head {
            rt.block_on(playlist::head(s.clone(), id.clone()))
                .map_err(|e| e.class())
                .expect("head")
        };
        let post = |rt: &tokio::runtime::Runtime, head: &Head, ops| {
            sleep(spacing);
            rt.block_on(write::changes(
                s.clone(),
                id.clone(),
                write::list_changes(head.revision.clone(), ops),
            ))
            .map_err(|e| e.class())
            .expect("changes");
        };

        // Add A, B, A.
        let h = head(rt);
        assert_eq!(h.length, Some(0), "new playlist is empty");
        post(
            rt,
            &h,
            vec![write::add_op(&[track(0), track(1), track(0)], None)],
        );
        let h = head(rt);
        assert_eq!(h.length, Some(3), "three tracks after the add");
        println!("add A, B, A: length 3");

        // Remove index 2 (the second A): one A must be left.
        post(rt, &h, vec![write::remove_at_op(2)]);
        let h = head(rt);
        assert_eq!(h.length, Some(2), "two tracks after the remove");
        let (_, items) = rt
            .block_on(playlist::contents(s.clone(), id.clone()))
            .map_err(|e| e.class())
            .expect("contents");
        let a_left = items.iter().filter(|i| i.uri == TRACKS[0]).count();
        assert_eq!(a_left, 1, "exactly one copy of A is left");
        println!("duplicate remove: length 2, one copy of A left");

        // Overwrite with 3 tracks.
        post(
            rt,
            &h,
            write::overwrite_ops(
                h.length.expect("head length"),
                &[track(0), track(1), track(2)],
            ),
        );
        let h = head(rt);
        assert_eq!(h.length, Some(3), "three tracks after the overwrite");
        println!("overwrite 3: length 3");

        // Overwrite with the atomic maximum in one POST.
        let many: Vec<String> = (0..OVERWRITE_ATOMIC_MAX).map(|i| track(i % 3)).collect();
        post(
            rt,
            &h,
            write::overwrite_ops(h.length.expect("head length"), &many),
        );
        let h = head(rt);
        assert_eq!(
            h.length,
            Some(OVERWRITE_ATOMIC_MAX),
            "atomic overwrite landed"
        );
        println!(
            "overwrite {OVERWRITE_ATOMIC_MAX} in one POST: length {:?}",
            h.length
        );
    });
}

#[test]
#[ignore = "needs cached librespot credentials and network; creates a test playlist (never deleted)"]
fn playlist_move_rename_round_trip() {
    use super::playlist::{self, RootEntry};
    use super::playlist_write::{self as write, move_target, moved_position};
    use std::thread::sleep;
    use std::time::Duration;

    const TRACKS: [&str; 3] = [
        "spotify:track:4uLU6hMCjMI75M1A2tKUQC",
        "spotify:track:7ouMYWpwJ422jRcDASZB7P",
        "spotify:track:0VjIjW4GlUZAMYd2vXMi3b",
    ];
    let spacing = Duration::from_millis(300);
    // A, B, C for TRACKS in order: the test prints letters, never uris.
    let letters = |uris: &[String]| -> String {
        uris.iter()
            .map(|uri| {
                TRACKS
                    .iter()
                    .position(|t| t == uri)
                    .and_then(|i| u8::try_from(i).ok())
                    .map_or('?', |i| char::from(b'A' + i))
            })
            .collect()
    };

    with_session(|rt, s| {
        let (k, id) = create_recorded(rt, &s, spacing);
        let head = || {
            rt.block_on(playlist::head(s.clone(), id.clone()))
                .map_err(|e| e.class())
                .expect("head")
        };
        let post = |base: Vec<u8>, ops| {
            sleep(spacing);
            rt.block_on(write::changes(
                s.clone(),
                id.clone(),
                write::list_changes(base, ops),
            ))
            .map_err(|e| e.class())
            .expect("changes");
        };
        let order = || {
            let (_, items) = rt
                .block_on(playlist::contents(s.clone(), id.clone()))
                .map_err(|e| e.class())
                .expect("contents");
            letters(&items.into_iter().map(|i| i.uri).collect::<Vec<_>>())
        };

        post(
            head().revision,
            vec![write::add_op(&TRACKS.map(String::from), None)],
        );
        assert_eq!(order(), "ABC");

        // index, rows, up, the moved span after it, the whole order after it.
        for (index, n, up, span, all) in [
            (0, 1, false, "BA", "BAC"),
            (2, 1, true, "CA", "BCA"),
            (0, 10, false, "CAB", "CAB"),
        ] {
            let before = head();
            let length = before.length.expect("head length");
            let to = move_target(index, n, up, length).expect("a move");
            post(before.revision.clone(), vec![write::move_op(index, 1, to)]);
            let at = moved_position(index, to);
            let (after, items) = rt
                .block_on(playlist::range(
                    s.clone(),
                    id.clone(),
                    index.min(at),
                    index.abs_diff(at) + 1,
                ))
                .map_err(|e| e.class())
                .expect("range");
            let read = letters(&items.into_iter().map(|i| i.uri).collect::<Vec<_>>());
            println!("move {index} by {n} (up {up}): span {read}");
            assert_ne!(after.revision, before.revision, "a new revision");
            assert_eq!(after.length, Some(3), "the length is unchanged");
            assert_eq!(read, span, "the moved span");
            assert_eq!(order(), all, "the whole order");
        }

        // Rename, verified by the head name and by the rootlist name.
        let renamed = format!("ncspot-test-{k}-renamed");
        post(head().revision, vec![write::rename_op(&renamed)]);
        let head_named = head().name.as_deref() == Some(renamed.as_str());
        println!("rename to ncspot-test-{k}-renamed: head name matches {head_named}");
        assert!(head_named);
        let rootlist = rt
            .block_on(playlist::rootlist(s.clone()))
            .map_err(|e| e.class())
            .expect("rootlist");
        let listed = rootlist
            .entries
            .iter()
            .any(|e| matches!(e, RootEntry::Playlist(p) if p.id == id && p.name == renamed));
        println!("rename: rootlist name matches {listed}");
        assert!(
            listed,
            "the rootlist lists the new name right after the rename"
        );
    });
}
