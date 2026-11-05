//! The Playlists tab layout from the rootlist (folders, pinned playlists first), and the
//! spacing between playlist downloads in a sync.

#![deny(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::ext_traits::RwLockExt;
use crate::internal_api::collection::Item;
use crate::internal_api::playlist::{RootEntry, Rootlist};
use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::playlist::Playlist;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::traits::{ListItem, ViewExt};

/// Least time between two playlist content downloads in one sync.
pub const PLAYLIST_FETCH_SPACING: Duration = Duration::from_millis(250);

const PLAYLIST_PREFIX: &str = "spotify:playlist:";

/// One entry of the persisted Playlists tab layout.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayoutEntry {
    /// A pinned playlist id; all pins come first, in pin order.
    Pinned(String),
    /// A folder opens; `depth` is the number of folders it is nested in.
    FolderStart {
        name: String,
        depth: usize,
    },
    Playlist(String),
    FolderEnd,
}

/// The layout for `rootlist`: the `pinned` ids first, then the rootlist in order with the
/// nesting depth of each folder.
pub fn layout_from(rootlist: &Rootlist, pinned: &[String]) -> Vec<LayoutEntry> {
    let mut layout: Vec<LayoutEntry> = pinned
        .iter()
        .map(|id| LayoutEntry::Pinned(id.clone()))
        .collect();
    let mut depth = 0usize;
    for entry in &rootlist.entries {
        layout.push(match entry {
            RootEntry::Playlist(p) => LayoutEntry::Playlist(p.id.clone()),
            RootEntry::FolderStart { name } => {
                depth += 1;
                LayoutEntry::FolderStart {
                    name: name.clone(),
                    depth: depth - 1,
                }
            }
            RootEntry::FolderEnd => {
                depth = depth.saturating_sub(1);
                LayoutEntry::FolderEnd
            }
        });
    }
    layout
}

/// Put playlist `id` at the top of the rootlist part of `layout`, right after the pinned
/// block (where a rootlist add at index 0 lands).
pub fn insert_after_pins(layout: &mut Vec<LayoutEntry>, id: &str) {
    let at = layout
        .iter()
        .position(|entry| !matches!(entry, LayoutEntry::Pinned(_)))
        .unwrap_or(layout.len());
    layout.insert(at, LayoutEntry::Playlist(id.to_string()));
}

/// Replace the pinned block of `layout` with `pinned`. An empty layout (the flat list) stays
/// empty. True when the layout changed.
pub fn set_pins(layout: &mut Vec<LayoutEntry>, pinned: &[String]) -> bool {
    if layout.is_empty() {
        return false;
    }
    let rest: Vec<LayoutEntry> = layout
        .iter()
        .filter(|entry| !matches!(entry, LayoutEntry::Pinned(_)))
        .cloned()
        .collect();
    let mut next: Vec<LayoutEntry> = pinned
        .iter()
        .map(|id| LayoutEntry::Pinned(id.clone()))
        .collect();
    next.extend(rest);
    let changed = next != *layout;
    *layout = next;
    changed
}

/// Drop playlist `id` from the rootlist part of `layout`; its pin, if any, stays (pins are
/// their own set). True when an entry was removed.
pub fn remove_from_layout(layout: &mut Vec<LayoutEntry>, id: &str) -> bool {
    let before = layout.len();
    layout.retain(|entry| !matches!(entry, LayoutEntry::Playlist(p) if p == id));
    layout.len() != before
}

/// The playlist ids among the `ylpin` items, in set order. Other pins (Liked Songs, Your
/// Episodes) are left out.
pub fn pinned_ids(items: &[Item]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| item.uri.strip_prefix(PLAYLIST_PREFIX))
        .map(str::to_string)
        .collect()
}

/// The playlist store the rows look their playlists up in (`Library::playlists`).
pub type PlaylistStore = Arc<RwLock<Vec<Playlist>>>;

/// Replace the entry of `updated` in `store` when its snapshot is still `base_snapshot`, the
/// one `updated` was copied from; a sync that stored another snapshot meanwhile wins. False
/// when nothing was replaced (also for a playlist no longer in the store).
pub fn replace_if_unchanged(
    store: &mut [Playlist],
    updated: &Playlist,
    base_snapshot: &str,
) -> bool {
    match store.iter_mut().find(|p| p.id == updated.id) {
        Some(entry) if entry.snapshot_id == base_snapshot => {
            *entry = updated.clone();
            true
        }
        _ => false,
    }
}

/// One row of the Playlists tab.
#[derive(Clone)]
pub enum PlaylistRow {
    /// A folder name; inert, every item command on it does nothing.
    Header { name: String, depth: usize },
    /// A store playlist by id: `pinned` marks the copies in the pinned block at the top,
    /// `depth` is the number of folders it is in.
    Playlist {
        id: String,
        pinned: bool,
        depth: usize,
        store: PlaylistStore,
    },
}

impl PlaylistRow {
    /// The playlist id of this row, `None` for a header.
    pub fn playlist_id(&self) -> Option<&str> {
        match self {
            Self::Playlist { id, .. } => Some(id),
            Self::Header { .. } => None,
        }
    }

    /// `f` on the current store entry of this row; `None` for a header or a playlist no
    /// longer in the store.
    fn with<R>(&self, f: impl FnOnce(&Playlist) -> R) -> Option<R> {
        let Self::Playlist { id, store, .. } = self else {
            return None;
        };
        let store = store.read_unpoisoned();
        store.iter().find(|p| &p.id == id).map(f)
    }

    /// `f` on a copy of the store entry with no store lock held (it may load the tracks over
    /// the network), then a copy whose tracks or snapshot `f` changed goes back to the store
    /// unless a sync replaced the entry meanwhile (see [replace_if_unchanged]).
    fn with_copy(&self, f: impl FnOnce(&mut Playlist)) {
        let Self::Playlist { store, .. } = self else {
            return;
        };
        let Some(mut copy) = self.snapshot() else {
            return;
        };
        let base_snapshot = copy.snapshot_id.clone();
        let base_loaded = copy.tracks.is_some();
        f(&mut copy);
        if copy.snapshot_id != base_snapshot || copy.tracks.is_some() != base_loaded {
            replace_if_unchanged(&mut store.write_unpoisoned(), &copy, &base_snapshot);
        }
    }

    /// `f` on a copy of the store entry without its tracks (`num_tracks` set to the loaded
    /// count), run after the store guard is dropped. Every call that takes the `Library`
    /// goes through here or [Self::snapshot]: the library takes the store lock itself, and a
    /// second read lock on the writer-preferring store lock can deadlock behind a waiting
    /// writer.
    fn outside_store<R>(&self, f: impl FnOnce(&Playlist) -> R) -> Option<R> {
        let summary = self.with(|p| Playlist {
            id: p.id.clone(),
            name: p.name.clone(),
            owner_id: p.owner_id.clone(),
            owner_name: p.owner_name.clone(),
            snapshot_id: p.snapshot_id.clone(),
            num_tracks: p.tracks.as_ref().map_or(p.num_tracks, Vec::len),
            tracks: None,
            collaborative: p.collaborative,
            cover_url: p.cover_url.clone(),
        })?;
        Some(f(&summary))
    }

    /// A copy of the store entry, for calls into the library that take the store locks
    /// themselves (save, delete, follow).
    fn snapshot(&self) -> Option<Playlist> {
        self.with(Playlist::clone)
    }
}

impl ListItem for PlaylistRow {
    fn is_playing(&self, queue: &Queue) -> bool {
        self.with(|p| p.is_playing(queue)).unwrap_or(false)
    }

    fn display_left(&self, library: &Library) -> String {
        match self {
            Self::Header { name, depth } => format!("{}{name}/", "  ".repeat(*depth)),
            Self::Playlist { pinned, depth, .. } => {
                let marker = if *pinned { "* " } else { "" };
                let name = self
                    .outside_store(|p| p.display_left(library))
                    .unwrap_or_default();
                format!("{}{marker}{name}", "  ".repeat(*depth))
            }
        }
    }

    fn display_right(&self, library: &Library) -> String {
        self.outside_store(|p| p.display_right(library))
            .unwrap_or_default()
    }

    fn play(&mut self, queue: &Queue) {
        self.with_copy(|p| p.play(queue));
    }

    fn play_next(&mut self, queue: &Queue) {
        self.with_copy(|p| p.play_next(queue));
    }

    fn queue(&mut self, queue: &Queue) {
        self.with_copy(|p| p.queue(queue));
    }

    fn toggle_saved(&mut self, library: &Library) {
        if let Some(mut p) = self.snapshot() {
            p.toggle_saved(library);
        }
    }

    fn save(&mut self, library: &Library) {
        if let Some(mut p) = self.snapshot() {
            p.save(library);
        }
    }

    fn unsave(&mut self, library: &Library) {
        if let Some(mut p) = self.snapshot() {
            p.unsave(library);
        }
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        self.snapshot()?.open(queue, library)
    }

    fn open_recommendations(
        &mut self,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> Option<Box<dyn ViewExt>> {
        self.snapshot()?.open_recommendations(queue, library)
    }

    fn share_url(&self) -> Option<String> {
        self.with(ListItem::share_url).flatten()
    }

    fn playlist_id(&self) -> Option<String> {
        Self::playlist_id(self).map(str::to_string)
    }

    fn album(&self, queue: &Queue) -> Option<Album> {
        self.with(|p| p.album(queue)).flatten()
    }

    fn artists(&self) -> Option<Vec<Artist>> {
        self.with(ListItem::artists).flatten()
    }

    fn track(&self) -> Option<Track> {
        self.with(ListItem::track).flatten()
    }

    fn is_saved(&self, library: &Library) -> Option<bool> {
        self.outside_store(|p| p.is_saved(library)).flatten()
    }

    fn is_playable(&self) -> bool {
        self.with(ListItem::is_playable).unwrap_or(false)
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}

/// The Playlists tab rows for the store contents `playlists` and `layout`, looking their
/// playlists up in `store`. An empty layout gives the flat store list. Otherwise the pinned
/// playlists come first, then the layout with a header per folder (a pinned playlist appears
/// again at its place, unmarked), then the store playlists the layout does not list. Ids
/// missing from the store are skipped.
pub fn rows(
    playlists: &[Playlist],
    layout: &[LayoutEntry],
    store: &PlaylistStore,
) -> Vec<PlaylistRow> {
    let row = |id: &str, pinned, depth| PlaylistRow::Playlist {
        id: id.to_string(),
        pinned,
        depth,
        store: store.clone(),
    };
    if layout.is_empty() {
        return playlists.iter().map(|p| row(&p.id, false, 0)).collect();
    }
    let known: HashSet<&str> = playlists.iter().map(|p| p.id.as_str()).collect();
    let mut out = Vec::new();
    let mut listed = HashSet::new();
    for entry in layout {
        if let LayoutEntry::Pinned(id) = entry
            && known.contains(id.as_str())
        {
            listed.insert(id.as_str());
            out.push(row(id, true, 0));
        }
    }
    let mut depth = 0usize;
    for entry in layout {
        match entry {
            LayoutEntry::FolderStart { name, depth: d } => {
                out.push(PlaylistRow::Header {
                    name: name.clone(),
                    depth: *d,
                });
                depth = d + 1;
            }
            LayoutEntry::FolderEnd => depth = depth.saturating_sub(1),
            LayoutEntry::Playlist(id) => {
                listed.insert(id.as_str());
                if known.contains(id.as_str()) {
                    out.push(row(id, false, depth));
                }
            }
            LayoutEntry::Pinned(_) => {}
        }
    }
    out.extend(
        playlists
            .iter()
            .filter(|p| !listed.contains(p.id.as_str()))
            .map(|p| row(&p.id, false, 0)),
    );
    out
}

/// Keeps calls at least `gap` apart.
pub struct Spacer {
    gap: Duration,
    last: Option<Instant>,
}

impl Spacer {
    pub fn new(gap: Duration) -> Self {
        Self { gap, last: None }
    }

    /// How long to wait before the next call at `now`; records `now + wait` as the call time.
    pub fn next_wait(&mut self, now: Instant) -> Duration {
        let wait = self.last.map_or(Duration::ZERO, |last| {
            (last + self.gap).saturating_duration_since(now)
        });
        self.last = Some(now + wait);
        wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal_api::playlist::RootPlaylist;

    fn pl(id: &str) -> RootEntry {
        RootEntry::Playlist(RootPlaylist {
            id: id.into(),
            name: String::new(),
            snapshot_id: String::new(),
            length: 0,
            owner_id: String::new(),
            collaborative: false,
            cover_url: None,
            status: 200,
        })
    }

    fn p(id: &str) -> Playlist {
        Playlist {
            id: id.into(),
            name: String::new(),
            owner_id: String::new(),
            owner_name: None,
            snapshot_id: String::new(),
            num_tracks: 0,
            tracks: None,
            collaborative: false,
            cover_url: None,
        }
    }

    fn with_snapshot(id: &str, snapshot: &str, loaded: bool) -> Playlist {
        Playlist {
            snapshot_id: snapshot.into(),
            tracks: loaded.then(Vec::new),
            ..p(id)
        }
    }

    fn row(id: &str, store: &PlaylistStore) -> PlaylistRow {
        PlaylistRow::Playlist {
            id: id.into(),
            pinned: false,
            depth: 0,
            store: store.clone(),
        }
    }

    #[test]
    fn replace_if_unchanged_replaces_only_the_same_snapshot() {
        let mut store = vec![with_snapshot("a", "s1", false)];
        assert!(replace_if_unchanged(
            &mut store,
            &with_snapshot("a", "s1", true),
            "s1"
        ));
        assert!(store[0].tracks.is_some());

        let mut store = vec![with_snapshot("a", "s2", false)];
        assert!(!replace_if_unchanged(
            &mut store,
            &with_snapshot("a", "s1", true),
            "s1"
        ));
        assert_eq!(
            (store[0].snapshot_id.as_str(), store[0].tracks.is_none()),
            ("s2", true)
        );

        let mut store = vec![with_snapshot("b", "s1", false)];
        assert!(!replace_if_unchanged(
            &mut store,
            &with_snapshot("a", "s1", true),
            "s1"
        ));
    }

    #[test]
    fn a_row_action_runs_outside_the_store_lock_and_stores_the_loaded_copy() {
        let store: PlaylistStore = Arc::new(RwLock::new(vec![with_snapshot("a", "s1", false)]));
        row("a", &store).with_copy(|p| {
            assert!(store.try_write().is_ok(), "the store lock is held");
            p.tracks = Some(Vec::new());
        });
        assert!(store.read_unpoisoned()[0].tracks.is_some());
    }

    #[test]
    fn a_row_action_keeps_a_newer_sync_copy() {
        let store: PlaylistStore = Arc::new(RwLock::new(vec![with_snapshot("a", "", true)]));
        row("a", &store).with_copy(|p| {
            // A sync stores a newer copy while the action reloads.
            store.write_unpoisoned()[0] = with_snapshot("a", "sync", true);
            p.snapshot_id = "reload".into();
        });
        assert_eq!(store.read_unpoisoned()[0].snapshot_id, "sync");
    }

    fn describe(rows: &[PlaylistRow]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                PlaylistRow::Header { name, depth } => format!("header {name} {depth}"),
                PlaylistRow::Playlist {
                    id, pinned: true, ..
                } => format!("pin {id}"),
                PlaylistRow::Playlist { id, .. } => id.clone(),
            })
            .collect()
    }

    fn build(store: &[Playlist], layout: &[LayoutEntry]) -> Vec<PlaylistRow> {
        rows(store, layout, &Arc::new(RwLock::new(store.to_vec())))
    }

    fn folder(name: &str, depth: usize) -> LayoutEntry {
        LayoutEntry::FolderStart {
            name: name.into(),
            depth,
        }
    }

    #[test]
    fn a_new_playlist_goes_right_after_the_pins() {
        let mut layout = vec![
            LayoutEntry::Pinned("a".into()),
            LayoutEntry::Pinned("b".into()),
            folder("F", 0),
            LayoutEntry::Playlist("a".into()),
            LayoutEntry::FolderEnd,
        ];
        insert_after_pins(&mut layout, "n");
        assert_eq!(layout[2], LayoutEntry::Playlist("n".into()));
        assert_eq!(layout[3], folder("F", 0));
        assert_eq!(layout.len(), 6);

        let mut no_pins = vec![LayoutEntry::Playlist("c".into())];
        insert_after_pins(&mut no_pins, "n");
        assert_eq!(
            no_pins,
            vec![
                LayoutEntry::Playlist("n".into()),
                LayoutEntry::Playlist("c".into())
            ]
        );
    }

    #[test]
    fn a_removed_playlist_leaves_the_layout_but_keeps_its_pin() {
        let mut layout = vec![
            LayoutEntry::Pinned("a".into()),
            folder("F", 0),
            LayoutEntry::Playlist("a".into()),
            LayoutEntry::FolderEnd,
            LayoutEntry::Playlist("b".into()),
        ];
        assert!(remove_from_layout(&mut layout, "a"));
        assert_eq!(
            layout,
            vec![
                LayoutEntry::Pinned("a".into()),
                folder("F", 0),
                LayoutEntry::FolderEnd,
                LayoutEntry::Playlist("b".into()),
            ]
        );
        assert!(!remove_from_layout(&mut layout, "x"));
    }

    #[test]
    fn no_layout_gives_the_flat_store_list() {
        let store = vec![p("p2"), p("p1")];
        assert_eq!(describe(&build(&store, &[])), vec!["p2", "p1"]);
    }

    #[test]
    fn pinned_rows_come_first_and_again_in_their_folder() {
        let store = vec![p("p1"), p("p2")];
        let layout = vec![
            LayoutEntry::Pinned("p2".into()),
            LayoutEntry::FolderStart {
                name: "A".into(),
                depth: 0,
            },
            LayoutEntry::Playlist("p2".into()),
            LayoutEntry::FolderEnd,
            LayoutEntry::Playlist("p1".into()),
        ];
        let r = build(&store, &layout);
        assert_eq!(describe(&r), vec!["pin p2", "header A 0", "p2", "p1"]);
    }

    #[test]
    fn nested_folders_keep_their_depth() {
        let store = vec![p("p1"), p("p2")];
        let layout = vec![
            folder("A", 0),
            LayoutEntry::Playlist("p1".into()),
            folder("B", 1),
            LayoutEntry::Playlist("p2".into()),
            LayoutEntry::FolderEnd,
            LayoutEntry::FolderEnd,
        ];
        assert_eq!(
            describe(&build(&store, &layout)),
            vec!["header A 0", "p1", "header B 1", "p2"]
        );
    }

    #[test]
    fn store_playlists_missing_from_the_layout_are_appended() {
        let store = vec![p("p3"), p("p1"), p("p2")];
        let layout = vec![LayoutEntry::Playlist("p1".into())];
        assert_eq!(describe(&build(&store, &layout)), vec!["p1", "p3", "p2"]);
    }

    #[test]
    fn ids_missing_from_the_store_are_skipped() {
        let store = vec![p("p1")];
        let layout = vec![
            LayoutEntry::Pinned("gone".into()),
            LayoutEntry::Playlist("gone".into()),
            LayoutEntry::Playlist("p1".into()),
        ];
        assert_eq!(describe(&build(&store, &layout)), vec!["p1"]);
    }

    #[test]
    fn header_rows_are_inert() {
        let header = PlaylistRow::Header {
            name: "A".into(),
            depth: 1,
        };
        assert!(!header.is_playable());
        assert!(header.share_url().is_none());
        assert!(header.track().is_none());
        assert!(header.artists().is_none());
        assert!(header.playlist_id().is_none());
        assert!(!header.as_listitem().is_playable());
    }

    #[test]
    fn playlist_rows_resolve_to_the_current_store_entry() {
        let store: PlaylistStore = Arc::new(RwLock::new(vec![p("p1")]));
        let r = rows(&store.read_unpoisoned().clone(), &[], &store);
        assert!(r[0].is_playable());
        let url = |r: &PlaylistRow| r.share_url().unwrap();
        assert!(!url(&r[0]).contains("/user/o/"));
        store.write_unpoisoned()[0].owner_id = "o".into();
        assert!(url(&r[0]).contains("/user/o/"));
        store.write_unpoisoned().clear();
        assert!(!r[0].is_playable());
        assert!(r[0].share_url().is_none());
    }

    #[test]
    fn library_calls_run_outside_the_store_guard() {
        let mut loaded = p("p1");
        loaded.num_tracks = 9;
        loaded.tracks = Some(vec![crate::playlist_items::placeholder("spotify:track:a")]);
        let store: PlaylistStore = Arc::new(RwLock::new(vec![loaded]));
        let r = rows(&store.read_unpoisoned().clone(), &[], &store);
        let seen = r[0].outside_store(|p| {
            assert!(store.try_write().is_ok(), "the store guard is dropped");
            (p.id.clone(), p.num_tracks, p.tracks.is_none())
        });
        assert_eq!(seen, Some(("p1".to_string(), 1, true)));
        store.write_unpoisoned().clear();
        assert_eq!(r[0].outside_store(|_| ()), None);
    }

    #[test]
    fn playlist_rows_carry_their_folder_depth() {
        let store = vec![p("p1"), p("p2"), p("p3")];
        let layout = vec![
            folder("A", 0),
            folder("B", 1),
            LayoutEntry::Playlist("p1".into()),
            LayoutEntry::FolderEnd,
            LayoutEntry::Playlist("p2".into()),
            LayoutEntry::FolderEnd,
            LayoutEntry::Playlist("p3".into()),
        ];
        let depths: Vec<usize> = build(&store, &layout)
            .iter()
            .map(|row| match row {
                PlaylistRow::Header { depth, .. } | PlaylistRow::Playlist { depth, .. } => *depth,
            })
            .collect();
        assert_eq!(depths, vec![0, 1, 2, 1, 0]);
    }

    #[test]
    fn a_pinned_playlist_outside_the_layout_is_not_appended_again() {
        let store = vec![p("p1"), p("p2")];
        let layout = vec![
            LayoutEntry::Pinned("p2".into()),
            LayoutEntry::Playlist("p1".into()),
        ];
        assert_eq!(describe(&build(&store, &layout)), vec!["pin p2", "p1"]);
    }

    #[test]
    fn layout_tracks_folder_depth_and_pins_first() {
        let root = Rootlist {
            length: 6,
            entries: vec![
                RootEntry::FolderStart { name: "A".into() },
                pl("p1"),
                RootEntry::FolderStart { name: "B".into() },
                pl("p2"),
                RootEntry::FolderEnd,
                RootEntry::FolderEnd,
            ],
            ..Default::default()
        };
        assert_eq!(
            layout_from(&root, &["p2".into()]),
            vec![
                LayoutEntry::Pinned("p2".into()),
                LayoutEntry::FolderStart {
                    name: "A".into(),
                    depth: 0
                },
                LayoutEntry::Playlist("p1".into()),
                LayoutEntry::FolderStart {
                    name: "B".into(),
                    depth: 1
                },
                LayoutEntry::Playlist("p2".into()),
                LayoutEntry::FolderEnd,
                LayoutEntry::FolderEnd,
            ]
        );
    }

    #[test]
    fn set_pins_replaces_the_pinned_block() {
        let mut layout = vec![
            LayoutEntry::Pinned("a".into()),
            LayoutEntry::Playlist("x".into()),
        ];
        assert!(set_pins(&mut layout, &["b".to_string(), "c".to_string()]));
        assert_eq!(
            layout,
            vec![
                LayoutEntry::Pinned("b".into()),
                LayoutEntry::Pinned("c".into()),
                LayoutEntry::Playlist("x".into()),
            ]
        );
        assert!(!set_pins(&mut layout, &["b".to_string(), "c".to_string()]));
        assert!(set_pins(&mut layout, &[]));
        assert_eq!(layout, vec![LayoutEntry::Playlist("x".into())]);
    }

    #[test]
    fn set_pins_leaves_a_flat_layout_empty() {
        let mut layout = Vec::new();
        assert!(!set_pins(&mut layout, &["a".to_string()]));
        assert!(layout.is_empty());
    }

    #[test]
    fn pinned_ids_keep_only_playlists() {
        use crate::internal_api::collection::Item;
        let it = |uri: &str| Item {
            uri: uri.into(),
            added_at: 0,
        };
        let items = [
            it("spotify:collection:tracks"),
            it("spotify:playlist:p2"),
            it("spotify:collection:your-episodes"),
            it("spotify:playlist:p1"),
        ];
        assert_eq!(pinned_ids(&items), vec!["p2".to_string(), "p1".to_string()]);
    }

    #[test]
    fn spacer_waits_out_the_gap() {
        let t0 = Instant::now();
        let mut s = Spacer::new(Duration::from_millis(250));
        assert_eq!(s.next_wait(t0), Duration::ZERO);
        assert_eq!(
            s.next_wait(t0 + Duration::from_millis(100)),
            Duration::from_millis(150)
        );
        assert_eq!(s.next_wait(t0 + Duration::from_secs(5)), Duration::ZERO);
    }
}
