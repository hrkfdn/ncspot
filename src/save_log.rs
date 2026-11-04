//! Library saves on their way to Spotify, and the store changes they make. Pure functions over a
//! store, so a save, its undo and its replay over a sync share one implementation.
#![deny(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Duration;

use crate::internal_api::collection::{SET_ARTIST, SET_COLLECTION, SET_LISTEN_LATER, SET_SHOW};
use crate::library_collection::sort_albums;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::episode::Episode;
use crate::model::show::Show;
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

/// One item a save acts on.
#[derive(Clone)]
pub enum SaveItem {
    Track(Box<Track>),
    Album(Box<Album>),
    Artist(Box<Artist>),
    Show(Box<Show>),
    Episode(Box<Episode>),
}

impl SaveItem {
    fn kind(&self) -> &'static str {
        match self {
            Self::Track(_) => "track",
            Self::Album(_) => "album",
            Self::Artist(_) => "artist",
            Self::Show(_) => "show",
            Self::Episode(_) => "episode",
        }
    }

    fn id(&self) -> Option<&str> {
        match self {
            Self::Track(t) => t.id.as_deref(),
            Self::Album(a) => a.id.as_deref(),
            Self::Artist(a) => a.id.as_deref(),
            Self::Show(s) => Some(&s.id),
            Self::Episode(e) => Some(&e.id),
        }
    }

    fn same(&self, other: &Self) -> bool {
        self.kind() == other.kind() && self.id() == other.id()
    }

    /// The collection set this item is written to.
    fn set(&self) -> &'static str {
        match self {
            Self::Track(_) | Self::Album(_) => SET_COLLECTION,
            Self::Artist(_) => SET_ARTIST,
            Self::Show(_) => SET_SHOW,
            Self::Episode(_) => SET_LISTEN_LATER,
        }
    }
}

/// One save (or, with `remove`, one unsave or unfollow).
#[derive(Clone)]
pub struct SaveOp {
    /// The [SaveLog] entry: the job is queued after it exists, so the writer's seq cannot name it.
    pub entry: u64,
    pub item: SaveItem,
    pub remove: bool,
}

// By hand: the items' names must stay out of logs.
impl fmt::Debug for SaveOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SaveOp({}, remove: {})", self.item.kind(), self.remove)
    }
}

/// An item's state in its store before an op: for artists, Present means followed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prior {
    Absent,
    Present(usize),
}

/// A store that saves act on.
pub trait SavedStore: Clone {
    /// This store's item in `item`, if it is of this store's type.
    fn of(item: &SaveItem) -> Option<&Self>;

    /// What makes two entries the same item.
    fn key(&self) -> Option<&str>;

    /// This entry as a [SaveItem].
    fn item(self) -> SaveItem;

    /// The entry the store holds for `item`: an unsave undoes to it, not to the caller's copy,
    /// which may lack the date or the tracks.
    fn stored(store: &[Self], item: &SaveItem) -> Option<Self> {
        let key = Self::of(item)?.key();
        store.iter().find(|e| e.key() == key).cloned()
    }

    /// Add a new `item`.
    fn place(store: &mut Vec<Self>, item: Self) {
        store.insert(0, item);
    }

    /// Restore the store's order after a row was put back.
    fn settle(_store: &mut [Self]) {}

    /// The state of `item` in `store`.
    fn prior(store: &[Self], item: &SaveItem) -> Prior {
        let row = Self::of(item).and_then(|item| store.iter().position(|e| e.key() == item.key()));
        row.map_or(Prior::Absent, Prior::Present)
    }

    /// Apply `op`; repeats change nothing, so replaying is safe.
    fn apply(store: &mut Vec<Self>, op: &SaveOp) {
        let Some(item) = Self::of(&op.item) else {
            return;
        };
        let present = store.iter().any(|e| e.key() == item.key());
        if op.remove {
            store.retain(|e| e.key() != item.key());
        } else if !present {
            Self::place(store, item.clone());
        }
    }

    /// Put the item back to `prior`.
    fn undo(store: &mut Vec<Self>, op: &SaveOp, prior: Prior) {
        let Some(item) = Self::of(&op.item) else {
            return;
        };
        match prior {
            Prior::Absent => store.retain(|e| e.key() != item.key()),
            Prior::Present(row) => {
                if !store.iter().any(|e| e.key() == item.key()) {
                    store.insert(row.min(store.len()), item.clone());
                    Self::settle(store);
                }
            }
        }
    }
}

impl SavedStore for Track {
    fn of(item: &SaveItem) -> Option<&Self> {
        match item {
            SaveItem::Track(t) => Some(t),
            _ => None,
        }
    }

    fn key(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn item(self) -> SaveItem {
        SaveItem::Track(Box::new(self))
    }
}

impl SavedStore for Album {
    fn of(item: &SaveItem) -> Option<&Self> {
        match item {
            SaveItem::Album(a) => Some(a),
            _ => None,
        }
    }

    fn key(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn item(self) -> SaveItem {
        SaveItem::Album(Box::new(self))
    }

    fn place(store: &mut Vec<Self>, item: Self) {
        store.insert(0, item);
        sort_albums(store);
    }

    fn settle(store: &mut [Self]) {
        sort_albums(store);
    }
}

impl SavedStore for Show {
    fn of(item: &SaveItem) -> Option<&Self> {
        match item {
            SaveItem::Show(s) => Some(s),
            _ => None,
        }
    }

    fn key(&self) -> Option<&str> {
        Some(&self.id)
    }

    fn item(self) -> SaveItem {
        SaveItem::Show(Box::new(self))
    }
}

impl SavedStore for Episode {
    fn of(item: &SaveItem) -> Option<&Self> {
        match item {
            SaveItem::Episode(e) => Some(e),
            _ => None,
        }
    }

    fn key(&self) -> Option<&str> {
        Some(&self.id)
    }

    fn item(self) -> SaveItem {
        SaveItem::Episode(Box::new(self))
    }
}

/// Artists take both follows and the artist side of track ops.
impl SavedStore for Artist {
    fn of(item: &SaveItem) -> Option<&Self> {
        match item {
            SaveItem::Artist(a) => Some(a),
            _ => None,
        }
    }

    fn key(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn item(self) -> SaveItem {
        SaveItem::Artist(Box::new(self))
    }

    fn prior(store: &[Self], item: &SaveItem) -> Prior {
        let row = Self::of(item).and_then(|item| {
            store
                .iter()
                .position(|e| e.key() == item.key() && e.is_followed)
        });
        row.map_or(Prior::Absent, Prior::Present)
    }

    fn apply(store: &mut Vec<Self>, op: &SaveOp) {
        match &op.item {
            SaveItem::Artist(a) => set_followed(store, a, !op.remove),
            SaveItem::Track(t) if op.remove => remove_track(store, t),
            SaveItem::Track(t) => add_track(store, t),
            _ => {}
        }
    }

    fn undo(store: &mut Vec<Self>, op: &SaveOp, prior: Prior) {
        let was_there = prior != Prior::Absent;
        match &op.item {
            SaveItem::Artist(a) => set_followed(store, a, was_there),
            SaveItem::Track(t) if was_there => add_track(store, t),
            SaveItem::Track(t) => remove_track(store, t),
            _ => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Pending,
    Landed,
}

struct Entry {
    op: SaveOp,
    state: State,
    prior: Prior,
}

/// Saves not yet settled, and saves that landed while a sync ran.
#[derive(Default)]
pub struct SaveLog {
    next: u64,
    entries: Vec<Entry>,
}

impl SaveLog {
    /// Log a new op on `item`, whose state before it is `prior`.
    pub fn add(&mut self, item: SaveItem, remove: bool, prior: Prior) -> SaveOp {
        self.next += 1;
        let op = SaveOp {
            entry: self.next,
            item,
            remove,
        };
        self.entries.push(Entry {
            op: op.clone(),
            state: State::Pending,
            prior,
        });
        op
    }

    /// Spotify took `entry`.
    pub fn mark_landed(&mut self, entry: u64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.op.entry == entry) {
            e.state = State::Landed;
        }
    }

    /// Forget `entry`.
    pub fn remove(&mut self, entry: u64) {
        self.entries.retain(|e| e.op.entry != entry);
    }

    /// Forget the failed `entry`. Returns what to undo, or None when a later op on the item
    /// decides its state: that op inherits the prior state, so its own failure restores it.
    pub fn fail(&mut self, entry: u64) -> Option<(SaveOp, Prior)> {
        let at = self.entries.iter().position(|e| e.op.entry == entry)?;
        let failed = self.entries.remove(at);
        match self.entries[at..]
            .iter_mut()
            .find(|e| e.op.item.same(&failed.op.item))
        {
            Some(later) => {
                later.prior = failed.prior;
                None
            }
            None => Some((failed.op, failed.prior)),
        }
    }

    /// A sync ended: forget what landed.
    pub fn clear_landed(&mut self) {
        self.entries.retain(|e| e.state != State::Landed);
    }

    /// Whether any op is still unsettled: not yet landed or failed.
    pub fn any_pending(&self) -> bool {
        self.entries.iter().any(|e| e.state == State::Pending)
    }

    /// Whether `set` has an op Spotify has not confirmed.
    pub fn has_pending(&self, set: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.state == State::Pending && e.op.item.set() == set)
    }

    /// Re-apply every op, oldest first, onto a store a sync just changed.
    pub fn replay<T: SavedStore>(&self, store: &mut Vec<T>) {
        for e in &self.entries {
            T::apply(store, &e.op);
        }
    }

    /// Turn a copy of the store into what Spotify confirmed: undo pending ops, newest first.
    pub fn confirmed<T: SavedStore>(&self, store: &mut Vec<T>) {
        for e in self
            .entries
            .iter()
            .rev()
            .filter(|e| e.state == State::Pending)
        {
            T::undo(store, &e.op, e.prior);
        }
    }
}

/// How one try of a save ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WriteOutcome {
    Done,
    RateLimited,
    Transient,
    Failed,
}

/// Tries of one save.
const TRIES: usize = 3;
/// The waits after a transient failure.
const TRANSIENT_WAITS: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(4)];
/// The shortest and longest wait for a rate limit.
const MIN_WAIT: Duration = Duration::from_secs(2);
const MAX_WAIT: Duration = Duration::from_secs(30);
/// A rate-limit window longer than this cannot be waited out in the tries left.
const LONG_WINDOW: Duration = Duration::from_secs(60);

/// Run `write` up to [TRIES] times; true when it landed. `window` is the rate-limit window left,
/// `wait` waits and returns false when the writer stopped.
pub fn run_with_retries(
    mut write: impl FnMut() -> WriteOutcome,
    window: impl Fn() -> Option<Duration>,
    mut wait: impl FnMut(Duration) -> bool,
) -> bool {
    for attempt in 0..TRIES {
        let delay = match write() {
            WriteOutcome::Done => return true,
            WriteOutcome::Failed => return false,
            WriteOutcome::Transient => TRANSIENT_WAITS[attempt.min(TRANSIENT_WAITS.len() - 1)],
            WriteOutcome::RateLimited => {
                let left = window();
                if left.is_some_and(|left| left > LONG_WINDOW) {
                    return false;
                }
                left.unwrap_or(MIN_WAIT).clamp(MIN_WAIT, MAX_WAIT)
            }
        };
        if attempt + 1 == TRIES || !wait(delay) {
            return false;
        }
    }
    false
}

#[cfg(test)]
pub mod test_items {
    use crate::model::artist::Artist;
    use crate::model::episode::Episode;
    use crate::model::show::Show;
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

    pub fn show(id: &str) -> Show {
        Show {
            id: id.to_string(),
            uri: format!("spotify:show:{id}"),
            name: String::new(),
            description: String::new(),
            cover_url: None,
            episodes: None,
            added_at: None,
        }
    }

    pub fn episode(id: &str) -> Episode {
        Episode {
            id: id.to_string(),
            uri: format!("spotify:episode:{id}"),
            duration: 0,
            name: String::new(),
            description: String::new(),
            release_date: String::new(),
            cover_url: None,
            added_at: None,
            list_index: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_items::{artist, track};
    use super::*;
    use WriteOutcome::{Done, Failed, RateLimited, Transient};
    use std::cell::{Cell, RefCell};
    use std::time::Duration;

    /// Runs `outcomes` through the retries; returns the result, the tries and the waits.
    fn retried(
        outcomes: &[WriteOutcome],
        window: Option<Duration>,
        stop_at_wait: Option<usize>,
    ) -> (bool, usize, Vec<u64>) {
        let tries = Cell::new(0);
        let waits = RefCell::new(Vec::new());
        let ok = run_with_retries(
            || {
                tries.set(tries.get() + 1);
                outcomes[tries.get() - 1]
            },
            || window,
            |wait| {
                waits.borrow_mut().push(wait.as_secs());
                stop_at_wait != Some(waits.borrow().len())
            },
        );
        (ok, tries.get(), waits.into_inner())
    }

    #[test]
    fn a_first_try_success_does_not_wait() {
        assert_eq!(retried(&[Done], None, None), (true, 1, vec![]));
    }

    #[test]
    fn a_hard_failure_is_final() {
        assert_eq!(retried(&[Failed], None, None), (false, 1, vec![]));
    }

    #[test]
    fn a_transient_failure_waits_2_then_4_seconds_for_3_tries() {
        let all = [Transient, Transient, Transient];
        assert_eq!(retried(&all, None, None), (false, 3, vec![2, 4]));
        let late = [Transient, Done];
        assert_eq!(retried(&late, None, None), (true, 2, vec![2]));
    }

    #[test]
    fn a_rate_limit_waits_the_window_capped() {
        let all = [RateLimited, RateLimited, RateLimited];
        let secs = |s| Some(Duration::from_secs(s));
        assert_eq!(retried(&all, secs(5), None), (false, 3, vec![5, 5]));
        assert_eq!(retried(&all, secs(45), None), (false, 3, vec![30, 30]));
        assert_eq!(
            retried(&all, None, None),
            (false, 3, vec![2, 2]),
            "no window: the floor"
        );
    }

    #[test]
    fn a_window_too_long_to_wait_out_fails_at_once() {
        let window = Some(Duration::from_secs(61));
        assert_eq!(retried(&[RateLimited], window, None), (false, 1, vec![]));
    }

    #[test]
    fn a_stop_during_a_wait_ends_the_retries() {
        let all = [Transient, Transient, Transient];
        assert_eq!(retried(&all, None, Some(1)), (false, 1, vec![2]));
    }

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

    fn saved(t: &Track) -> SaveItem {
        SaveItem::Track(Box::new(t.clone()))
    }

    /// Apply a save or remove to `store` and log it, as the UI thread does.
    fn logged(log: &mut SaveLog, store: &mut Vec<Track>, t: &Track, remove: bool) -> SaveOp {
        let item = saved(t);
        let prior = Track::prior(store, &item);
        let op = log.add(item, remove, prior);
        Track::apply(store, &op);
        op
    }

    fn ids(store: &[Track]) -> Vec<&str> {
        store.iter().filter_map(|t| t.id.as_deref()).collect()
    }

    #[test]
    fn replay_puts_pending_and_landed_ops_back_on_a_new_list() {
        let (a, b, c) = (track("a", &[]), track("b", &[]), track("c", &[]));
        let mut log = SaveLog::default();
        let mut store = vec![b.clone(), c.clone()];
        let save = logged(&mut log, &mut store, &a, false);
        logged(&mut log, &mut store, &c, true);
        log.mark_landed(save.entry);
        let mut swapped = vec![b.clone(), c.clone()];
        log.replay(&mut swapped);
        assert_eq!(ids(&swapped), ["a", "b"]);
        log.replay(&mut swapped);
        assert_eq!(ids(&swapped), ["a", "b"], "replaying twice changes nothing");
    }

    #[test]
    fn replay_keeps_the_order_of_ops_on_one_item() {
        let a = track("a", &[]);
        let mut log = SaveLog::default();
        let mut store = vec![];
        logged(&mut log, &mut store, &a, false);
        logged(&mut log, &mut store, &a, true);
        let mut swapped: Vec<Track> = vec![];
        log.replay(&mut swapped);
        assert!(swapped.is_empty());
    }

    #[test]
    fn the_confirmed_view_drops_pending_ops_and_keeps_landed_ones() {
        let (a, b, c) = (track("a", &[]), track("b", &[]), track("c", &[]));
        let mut log = SaveLog::default();
        let mut store = vec![b.clone(), c.clone()];
        let landed = logged(&mut log, &mut store, &a, false);
        log.mark_landed(landed.entry);
        logged(&mut log, &mut store, &track("p", &[]), false);
        logged(&mut log, &mut store, &c, true);
        assert_eq!(ids(&store), ["p", "a", "b"]);
        log.confirmed(&mut store);
        assert_eq!(ids(&store), ["a", "b", "c"]);
    }

    #[test]
    fn a_failed_op_hands_its_prior_state_to_the_next_op_on_the_item() {
        let a = track("a", &[]);
        let mut log = SaveLog::default();
        let mut store = vec![];
        let save = logged(&mut log, &mut store, &a, false);
        let unsave = logged(&mut log, &mut store, &a, true);
        assert!(log.fail(save.entry).is_none(), "the later op decides");
        let (op, prior) = log.fail(unsave.entry).unwrap();
        assert_eq!(prior, Prior::Absent, "the original state");
        Track::undo(&mut store, &op, prior);
        assert!(store.is_empty());
    }

    #[test]
    fn a_failed_remove_goes_back_to_its_row() {
        let (a, b, c) = (track("a", &[]), track("b", &[]), track("c", &[]));
        let mut log = SaveLog::default();
        let mut store = vec![a.clone(), b.clone(), c.clone()];
        let op = logged(&mut log, &mut store, &b, true);
        let (op, prior) = log.fail(op.entry).unwrap();
        assert_eq!(prior, Prior::Present(1));
        Track::undo(&mut store, &op, prior);
        assert_eq!(ids(&store), ["a", "b", "c"]);
    }

    #[test]
    fn an_undo_row_past_the_end_is_clamped() {
        let (a, b) = (track("a", &[]), track("b", &[]));
        let mut store = vec![a.clone()];
        let op = SaveOp {
            entry: 1,
            item: saved(&b),
            remove: true,
        };
        Track::undo(&mut store, &op, Prior::Present(9));
        assert_eq!(ids(&store), ["a", "b"]);
    }

    #[test]
    fn a_track_op_reaches_the_artist_store() {
        let t = track("t", &[("a", "A")]);
        let mut log = SaveLog::default();
        let mut tracks = vec![];
        logged(&mut log, &mut tracks, &t, false);
        let mut artists: Vec<Artist> = vec![];
        log.replay(&mut artists);
        assert_eq!(names(&artists), ["A"]);
        log.confirmed(&mut artists);
        assert!(artists.is_empty());
    }

    #[test]
    fn a_follow_undoes_by_its_prior_flag() {
        let a = artist("a", "A", false);
        let item = SaveItem::Artist(Box::new(a.clone()));
        let mut store = vec![];
        let prior = Artist::prior(&store, &item);
        assert_eq!(prior, Prior::Absent);
        let mut log = SaveLog::default();
        let op = log.add(item, false, prior);
        Artist::apply(&mut store, &op);
        assert!(store[0].is_followed);
        Artist::undo(&mut store, &op, prior);
        assert!(store.is_empty());
    }

    #[test]
    fn landed_ops_clear_and_pending_ones_stay() {
        let (a, b) = (track("a", &[]), track("b", &[]));
        let mut log = SaveLog::default();
        let mut store = vec![];
        let landed = logged(&mut log, &mut store, &a, false);
        log.mark_landed(landed.entry);
        logged(&mut log, &mut store, &b, false);
        log.clear_landed();
        let mut swapped = vec![];
        log.replay(&mut swapped);
        assert_eq!(ids(&swapped), ["b"]);
    }

    #[test]
    fn pending_is_per_set() {
        use crate::internal_api::collection::{SET_ARTIST, SET_COLLECTION};
        let a = track("a", &[]);
        let mut log = SaveLog::default();
        let mut store = vec![];
        let op = logged(&mut log, &mut store, &a, false);
        assert!(log.has_pending(SET_COLLECTION));
        assert!(!log.has_pending(SET_ARTIST));
        log.mark_landed(op.entry);
        assert!(!log.has_pending(SET_COLLECTION), "landed is not pending");
    }

    #[test]
    fn each_kind_of_item_acts_on_its_own_store_and_set() {
        use crate::internal_api::collection::{SET_LISTEN_LATER, SET_SHOW};
        let mut album = Album::default_for_test("b");
        album.id = Some("b".to_string());
        let mut first = Album::default_for_test("a");
        first.id = Some("a".to_string());
        let mut third = Album::default_for_test("c");
        third.id = Some("c".to_string());
        let show = Show {
            id: "s".to_string(),
            uri: String::new(),
            name: String::new(),
            description: String::new(),
            cover_url: None,
            episodes: None,
            added_at: None,
        };
        let episode = Episode {
            id: "e".to_string(),
            uri: String::new(),
            duration: 0,
            name: String::new(),
            description: String::new(),
            release_date: String::new(),
            cover_url: None,
            added_at: None,
            list_index: 0,
        };

        let mut log = SaveLog::default();
        let save = log.add(SaveItem::Album(Box::new(album)), false, Prior::Absent);
        assert!(log.has_pending(SET_COLLECTION));
        let mut albums = vec![first, third];
        Album::apply(&mut albums, &save);
        let titles: Vec<_> = albums.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(
            titles,
            ["a", "b", "c"],
            "a new album goes to its sorted place"
        );
        log.mark_landed(save.entry);

        let save = log.add(SaveItem::Show(Box::new(show)), false, Prior::Absent);
        assert!(log.has_pending(SET_SHOW));
        let mut shows = vec![];
        Show::apply(&mut shows, &save);
        assert_eq!(shows.len(), 1);

        let save = log.add(SaveItem::Episode(Box::new(episode)), false, Prior::Absent);
        assert!(log.has_pending(SET_LISTEN_LATER));
        let mut episodes = vec![];
        Episode::apply(&mut episodes, &save);
        assert_eq!(episodes.len(), 1);

        assert!(!log.has_pending(SET_COLLECTION), "the album landed");
        let mut tracks: Vec<Track> = vec![];
        log.replay(&mut tracks);
        assert!(tracks.is_empty(), "other kinds leave the tracks alone");
    }
}
