//! One verified internal write on a playlist: bring the local copy up to date, POST the change
//! against the head revision, then confirm it with a second head read. Rootlist actions
//! (create, follow, unfollow) work the same way on the rootlist head.
//!
//! A write reply never proves the write applied (librespot may also re-send a POST by itself),
//! so the head length after the write is the check, and the local copy takes the snapshot of
//! that head, never the reply's revision.

use std::collections::HashMap;
use std::thread::sleep;
use std::time::Instant;

use librespot_protocol::playlist4_external::{ListChanges, Op};
use log::{debug, warn};

use crate::internal_api::Area;
use crate::internal_api::error::InternalError;
use crate::internal_api::playlist::{Head, RootEntry, RootHead, Rootlist, base64_encode};
use crate::internal_api::playlist_write::{
    ADD_BATCH, Created, OVERWRITE_ATOMIC_MAX, add_op, list_changes, overwrite_ops, remove_at_op,
    rootlist_add_op, rootlist_remove_op,
};
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::playlist_layout::{PLAYLIST_FETCH_SPACING, Spacer};
use crate::spotify_api::WebApi;

/// Builds the track and episode metadata already known locally (`Library::known_playables`).
/// Called only when a stale local copy is reloaded, so a write on a current copy never
/// builds it.
pub type Known<'a> = &'a dyn Fn() -> HashMap<String, Playable>;

/// The playlist reads and writes a verified edit makes: the [WebApi] in the app, a fake in
/// tests.
pub trait PlaylistServer {
    /// The head (revision, length) of a playlist, without its items.
    fn playlist_head(&self, id: &str) -> Result<Head, InternalError>;
    /// The full contents read: the head and every item.
    fn playlist_playables_at(
        &self,
        id: &str,
        known: &HashMap<String, Playable>,
    ) -> Result<(Head, Vec<Playable>), InternalError>;
    /// Items `from..from + length` (their uris) with the head they were read at; past the end
    /// the items are empty.
    #[expect(dead_code)]
    fn playlist_range(
        &self,
        id: &str,
        from: usize,
        length: usize,
    ) -> Result<(Head, Vec<String>), InternalError>;
    /// One write POST, sent at most once.
    fn playlist_changes(&self, id: &str, body: ListChanges) -> Result<(), InternalError>;
    /// The whole rootlist, page by page (up to the page cap).
    fn playlist_rootlist(&self) -> Result<Rootlist, InternalError>;
    /// The rootlist head (revision, length), without entries.
    fn rootlist_head(&self) -> Result<RootHead, InternalError>;
    /// One rootlist write POST, sent at most once.
    fn rootlist_changes(&self, body: ListChanges) -> Result<(), InternalError>;
    /// The area is on and not latched.
    fn internal_usable(&self, area: Area) -> bool;
    fn latch_area(&self, area: Area, err: &InternalError);
}

impl PlaylistServer for WebApi {
    fn playlist_head(&self, id: &str) -> Result<Head, InternalError> {
        Self::playlist_head(self, id)
    }

    fn playlist_playables_at(
        &self,
        id: &str,
        known: &HashMap<String, Playable>,
    ) -> Result<(Head, Vec<Playable>), InternalError> {
        Self::playlist_playables_at(self, id, known)
    }

    fn playlist_range(
        &self,
        id: &str,
        from: usize,
        length: usize,
    ) -> Result<(Head, Vec<String>), InternalError> {
        Self::playlist_range(self, id, from, length)
    }

    fn playlist_changes(&self, id: &str, body: ListChanges) -> Result<(), InternalError> {
        Self::playlist_changes(self, id, body)
    }

    fn playlist_rootlist(&self) -> Result<Rootlist, InternalError> {
        Self::playlist_rootlist(self)
    }

    fn rootlist_head(&self) -> Result<RootHead, InternalError> {
        Self::rootlist_head(self)
    }

    fn rootlist_changes(&self, body: ListChanges) -> Result<(), InternalError> {
        Self::rootlist_changes(self, body)
    }

    fn internal_usable(&self, area: Area) -> bool {
        Self::internal_usable(self, area)
    }

    fn latch_area(&self, area: Area, err: &InternalError) {
        Self::latch_area(self, area, err);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    Current,
    Stale,
}

/// Current when the local copy has loaded tracks, its snapshot is the head's, its track
/// count (placeholders included) is the head length and it is in server order (each item's
/// `list_index` is its position). A head without a length is never current.
pub fn freshness(local: &Playlist, head: &Head) -> Freshness {
    match &local.tracks {
        Some(tracks)
            if local.snapshot_id == head.snapshot_id
                && head.length == Some(tracks.len())
                && in_server_order(tracks) =>
        {
            Freshness::Current
        }
        _ => Freshness::Stale,
    }
}

/// Each item's `list_index` is its position, so positions in the copy are server positions.
fn in_server_order(tracks: &[Playable]) -> bool {
    tracks
        .iter()
        .enumerate()
        .all(|(position, playable)| playable.list_index() == position)
}

/// Where in `tracks` the item at server position `index` (its `list_index`) is, when it is
/// still `uri`. The position in `tracks` is not used as the server index: a copy may be in
/// another order (or, cached by older builds, hold one `list_index` twice; the uri tells them
/// apart).
pub fn server_item(tracks: &[Playable], index: usize, uri: &str) -> Option<usize> {
    tracks
        .iter()
        .position(|playable| playable.list_index() == index && playable.uri() == uri)
}

/// Remove the item at server position `index` (its `list_index`) when it is `uri`, and move
/// every later item one position up, keeping the order of `tracks`. False (nothing removed)
/// when no item at that position is `uri`.
pub fn remove_server_item(tracks: &mut Vec<Playable>, index: usize, uri: &str) -> bool {
    let Some(at) = server_item(tracks, index, uri) else {
        return false;
    };
    tracks.remove(at);
    for playable in tracks.iter_mut() {
        let position = playable.list_index();
        if position > index {
            playable.set_list_index(position - 1);
        }
    }
    true
}

/// What a playlist write is based on: the revision the POST names, the snapshot the local
/// copy takes and the length the expected one is computed from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Base {
    revision: Vec<u8>,
    snapshot_id: String,
    length: usize,
}

/// A head read without a revision is a protocol change: a write never names an empty base.
fn require_revision(head: Head, what: &str) -> Result<Head, InternalError> {
    if head.revision.is_empty() {
        return Err(InternalError::ProtocolChanged(format!(
            "{what} without a revision"
        )));
    }
    Ok(head)
}

/// The base a full read gives: its revision, and the count of the items it read (never the
/// reply's `length` field, which may differ from them).
fn listing_base(head: Head, count: usize) -> Result<Base, InternalError> {
    let head = require_revision(head, "playlist read")?;
    Ok(Base {
        revision: head.revision,
        snapshot_id: head.snapshot_id,
        length: count,
    })
}

/// The base of an overwrite: the head alone, since the contents are replaced whatever they
/// are. Nothing is sent from a head without a revision (a protocol change) or without a length
/// (a transient failure: the old contents cannot be removed by count, and they are not read).
fn overwrite_base(head: Head) -> Result<Base, InternalError> {
    let head = require_revision(head, "playlist head")?;
    let length = head
        .length
        .ok_or_else(|| InternalError::Transient("playlist head without a length".into()))?;
    Ok(Base {
        revision: head.revision,
        snapshot_id: head.snapshot_id,
        length,
    })
}

/// The length the reply of a full read reports when it is not the count of its items.
fn length_drift(head: &Head, count: usize) -> Option<usize> {
    head.length.filter(|&length| length != count)
}

/// The outcome of the head read after a write.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verify {
    /// The head length is the expected one; the head is the base of the next write.
    Applied(Base),
    /// The head length is another one.
    Differs(usize),
    /// The head lacks its revision or its length, so it proves nothing.
    Unreadable,
}

/// A write applied when the head length after it is the expected one. Length only: a
/// concurrent edit that leaves the length unchanged is not detected.
fn verify(expected_len: usize, after: Head) -> Verify {
    let Some(length) = after.length.filter(|_| !after.revision.is_empty()) else {
        return Verify::Unreadable;
    };
    if length != expected_len {
        return Verify::Differs(length);
    }
    Verify::Applied(Base {
        revision: after.revision,
        snapshot_id: after.snapshot_id,
        length,
    })
}

#[derive(Debug)]
pub enum EditError {
    /// Failed before anything was sent (including a first POST that never left: no session,
    /// area off or latched); the class decides the fallback as usual.
    Internal(InternalError),
    /// Failed after a POST may have reached the server (a transient or rate limited first
    /// POST, the verify read, or any error of a later batch): the write may have applied, so it
    /// must never be repeated through the Web API.
    Unverified(InternalError),
    /// The server refused the first POST of an action (not found, denied, a protocol change):
    /// nothing applied, but the action still fails without a Web fallback.
    Refused(InternalError),
    /// The selected item moved; nothing was sent.
    Moved,
    /// The head after the write does not match; the local copy was reloaded (when the reload
    /// failed, its snapshot was cleared instead).
    Mismatch,
    /// The rootlist read stopped at the page cap, so a copy of the playlist may lie past it;
    /// nothing was sent.
    TooLong,
    /// A delete removed at least one rootlist copy, then a later step failed: the playlist
    /// may still be in the library, and the action is never repeated through the Web API.
    PartlyDeleted,
    /// An add stopped after `added` of `total` tracks: the verified batches applied, the local
    /// copy holds them (or the server's copy, when a later batch did not verify). `unconfirmed`
    /// when the failed batch may have applied too.
    Partial {
        added: usize,
        total: usize,
        unconfirmed: bool,
    },
}

impl EditError {
    /// A fixed name of the failure class, for log lines (error texts may carry endpoint paths).
    pub fn class(&self) -> &'static str {
        match self {
            Self::Internal(err) => err.class(),
            Self::Unverified(_) => "unverified",
            Self::Refused(_) => "refused",
            Self::Moved => "moved",
            Self::Mismatch => "mismatch",
            Self::TooLong => "too_long",
            Self::PartlyDeleted => "partly_deleted",
            Self::Partial { .. } => "partial",
        }
    }

    /// The status note for a failure that is not a plain API error.
    pub fn note(&self) -> Option<String> {
        let note = match self {
            Self::Internal(_) => return None,
            Self::Moved => "Playlist changed, try again",
            Self::Mismatch => "Playlist changed elsewhere, reloaded",
            Self::Unverified(_) => "Could not confirm the change; it may have applied",
            Self::Refused(_) => "Spotify refused the change",
            Self::TooLong => "Playlist list too long to edit",
            Self::PartlyDeleted => "Removed some copies; the playlist is still in your library",
            Self::Partial {
                added,
                total,
                unconfirmed,
            } => {
                let at_least = if *unconfirmed { "at least " } else { "" };
                return Some(format!("Added {at_least}{added} of {total} tracks"));
            }
        };
        Some(note.to_string())
    }

    /// The edited local copy must be stored although the action failed, so the store matches
    /// the server: after an unconfirmed write it may hold earlier batches (a failed verify read
    /// cleared its snapshot, so the next write finds it stale and the next open reloads it
    /// with [reload_cleared]), after a mismatch it is the reloaded copy, after a partial add it
    /// holds the verified batches.
    pub fn stores_copy(&self) -> bool {
        matches!(
            self,
            Self::Unverified(_) | Self::Mismatch | Self::Partial { .. }
        )
    }
}

impl From<InternalError> for EditError {
    fn from(err: InternalError) -> Self {
        Self::Internal(err)
    }
}

/// A moved index, a mismatch, a refused POST or an error after a POST fails the action:
/// `Transient` never falls back to the Web API (which could apply the write twice) and never
/// latches the area (a refused POST latches it itself on a protocol change).
impl From<EditError> for InternalError {
    fn from(err: EditError) -> Self {
        match err {
            EditError::Internal(err) => err,
            EditError::Unverified(err) => {
                Self::Transient(format!("the write may have applied: {err}"))
            }
            EditError::Refused(err) => Self::Transient(format!("the write was refused: {err}")),
            EditError::Moved => Self::Transient("the selected item moved".into()),
            EditError::Mismatch => Self::Transient("the playlist changed during the write".into()),
            EditError::TooLong => Self::Transient("the playlist list is too long to edit".into()),
            EditError::PartlyDeleted => {
                Self::Transient("only some copies of the playlist were removed".into())
            }
            EditError::Partial { .. } => Self::Transient("only part of the add applied".into()),
        }
    }
}

/// One change to a playlist, for its expected length afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    /// Append this many tracks.
    Add(usize),
    /// Remove one item.
    Remove,
    /// Replace the contents with this many tracks.
    Overwrite(usize),
}

/// The head length after `change` on a head of `head_len`; None when there is nothing to
/// remove.
fn expected_len(head_len: usize, change: Change) -> Option<usize> {
    match change {
        Change::Add(n) => Some(head_len + n),
        Change::Remove => head_len.checked_sub(1),
        Change::Overwrite(n) => Some(n),
    }
}

/// The part of an overwrite sent in its one atomic POST, and the tail sent as add batches.
fn overwrite_split<T>(tracks: &[T]) -> (&[T], &[T]) {
    tracks.split_at(tracks.len().min(OVERWRITE_ATOMIC_MAX))
}

/// The add batches, one POST each.
fn add_chunks<T>(new: &[T]) -> std::slice::Chunks<'_, T> {
    new.chunks(ADD_BATCH)
}

/// Bring `playlist` up to date with the server when stale (a head without a length counts as
/// stale); returns the base of the write. A head or full read without a revision fails with
/// a protocol change before anything is sent.
fn prepare_head(
    api: &impl PlaylistServer,
    playlist: &mut Playlist,
    known: Known<'_>,
) -> Result<Base, EditError> {
    let head = require_revision(api.playlist_head(&playlist.id)?, "playlist head")?;
    match freshness(playlist, &head) {
        Freshness::Current => Ok(current_base(head, playlist)),
        Freshness::Stale => {
            let (base, drift) = reload(api, playlist, known)?;
            if let Some(reported) = drift {
                warn!(
                    "Playlist read lists {} items but reports {reported}; using the items",
                    base.length
                );
            }
            Ok(base)
        }
    }
}

/// The base of a write on a current local copy: the head read (its length is the copy's).
fn current_base(head: Head, playlist: &Playlist) -> Base {
    Base {
        revision: head.revision,
        snapshot_id: head.snapshot_id,
        length: playlist.tracks.as_ref().map_or(0, Vec::len),
    }
}

/// Replace the local tracks with a full read and take that read's snapshot; returns the base
/// it gives (see [listing_base]: the change is checked against these items, and the playlist
/// may have changed since the head read, so the POST must name the revision of this read) and
/// the reply's length when it differs from the item count. A read without a revision leaves
/// the local copy unchanged.
fn reload(
    api: &impl PlaylistServer,
    playlist: &mut Playlist,
    known: Known<'_>,
) -> Result<(Base, Option<usize>), InternalError> {
    let (head, tracks) = api.playlist_playables_at(&playlist.id, &known())?;
    let drift = length_drift(&head, tracks.len());
    let base = listing_base(head, tracks.len())?;
    set_tracks(playlist, tracks, &base.snapshot_id);
    Ok((base, drift))
}

/// Reload a loaded copy whose snapshot was cleared (a write that could not be confirmed, a
/// mismatch whose reload failed, a Web create, which has no revision) with one full read,
/// when the playlists area is usable; true when it was reloaded. The copy's own items are the
/// known metadata, so only items added elsewhere are looked up. A failed read keeps the copy
/// and its cleared snapshot (the next open or write tries again); a protocol change latches
/// the area.
pub fn reload_cleared(api: &impl PlaylistServer, playlist: &mut Playlist) -> bool {
    let Some(tracks) = &playlist.tracks else {
        return false;
    };
    if !playlist.snapshot_id.is_empty() || !api.internal_usable(Area::Playlists) {
        return false;
    }
    let own: HashMap<String, Playable> = tracks.iter().map(|p| (p.uri(), p.clone())).collect();
    match reload(api, playlist, &|| own.clone()) {
        Ok(_) => true,
        Err(err) => {
            warn!("Could not reload a playlist ({})", err.class());
            if let InternalError::ProtocolChanged(_) = err {
                api.latch_area(Area::Playlists, &err);
            }
            false
        }
    }
}

/// Store `tracks` (renumbered by position) with `snapshot_id`.
fn set_tracks(playlist: &mut Playlist, mut tracks: Vec<Playable>, snapshot_id: &str) {
    for (index, playable) in tracks.iter_mut().enumerate() {
        playable.set_list_index(index);
    }
    playlist.num_tracks = tracks.len();
    playlist.tracks = Some(tracks);
    playlist.snapshot_id = snapshot_id.to_string();
}

/// The position of a POST in its action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Post {
    /// The first POST: nothing of the action applied before it.
    First,
    /// A later add batch (add batches 2..n, the overwrite tail): earlier batches applied.
    Later,
}

/// The class of a failed write POST. A later batch is always [EditError::Unverified]:
/// earlier batches applied, so the Web API must never repeat the action. For the first POST,
/// a missing session never sent it ([EditError::Internal], the usual fallback); a transient
/// error or a rate limit may have reached the server ([EditError::Unverified]); a refusal or
/// a protocol change means nothing applied ([EditError::Refused], no Web fallback).
fn post_error(err: InternalError, post: Post) -> EditError {
    match (post, err) {
        (Post::Later, err) => EditError::Unverified(err),
        (Post::First, err @ InternalError::NoSession) => EditError::Internal(err),
        (Post::First, err @ (InternalError::Transient(_) | InternalError::RateLimited)) => {
            EditError::Unverified(err)
        }
        (
            Post::First,
            err @ (InternalError::NotFound
            | InternalError::Denied
            | InternalError::ProtocolChanged(_)),
        ) => EditError::Refused(err),
    }
}

/// The error of a POST skipped because the playlists area is off or latched: nothing was
/// sent, so a first POST keeps its class (latch, then the Web API) and a later batch is
/// [EditError::Unverified] like any of its errors.
fn unsent_post(post: Post) -> EditError {
    let err = InternalError::ProtocolChanged("area off or latched".into());
    match post {
        Post::First => EditError::Internal(err),
        Post::Later => EditError::Unverified(err),
    }
}

/// Send one write POST at position `post`. The area is checked first, so a POST that
/// `run_internal_once` would skip without sending is told apart from a server protocol
/// change; a server protocol change latches the playlists area, then [post_error] classes
/// the failure.
fn send_post(
    api: &impl PlaylistServer,
    post: Post,
    send: impl FnOnce() -> Result<(), InternalError>,
) -> Result<(), EditError> {
    if !api.internal_usable(Area::Playlists) {
        return Err(unsent_post(post));
    }
    send().map_err(|err| {
        if let InternalError::ProtocolChanged(_) = err {
            api.latch_area(Area::Playlists, &err);
        }
        post_error(err, post)
    })
}

/// The verify read after a POST failed or proved nothing: clear the local snapshot so the
/// next write (stale copy) or open ([reload_cleared]) reloads the copy (the caller stores it,
/// see [EditError::stores_copy]).
fn unconfirmed(playlist: &mut Playlist, err: InternalError) -> EditError {
    playlist.snapshot_id.clear();
    EditError::Unverified(err)
}

/// POST `ops` against `base`, then read the head: its base when its length is `expected_len`,
/// else reload the local copy (or, when the reload fails, clear its snapshot so the next write
/// or open reloads it, see [reload_cleared]) and fail with [EditError::Mismatch]. A failed POST is classed by
/// [post_error] at its position `post`; a failed or unreadable verify read is [unconfirmed].
fn send_verified(
    api: &impl PlaylistServer,
    playlist: &mut Playlist,
    known: Known<'_>,
    post: Post,
    base: Vec<u8>,
    ops: Vec<Op>,
    expected_len: usize,
) -> Result<Base, EditError> {
    send_post(api, post, || {
        api.playlist_changes(&playlist.id, list_changes(base, ops))
    })?;
    let after = match api.playlist_head(&playlist.id) {
        Ok(after) => after,
        Err(err) => return Err(unconfirmed(playlist, err)),
    };
    let length = match verify(expected_len, after) {
        Verify::Applied(base) => return Ok(base),
        Verify::Unreadable => {
            // Not latched: the POST went through and may have applied, so there is no Web
            // fallback either way, and the next head read decides whether the area is usable.
            return Err(unconfirmed(
                playlist,
                InternalError::ProtocolChanged("playlist head without a revision or length".into()),
            ));
        }
        Verify::Differs(length) => length,
    };
    warn!("Playlist write did not verify: length {length} after it, {expected_len} expected");
    if let Err(err) = reload(api, playlist, known) {
        warn!(
            "Could not reload the playlist after a write that did not verify ({})",
            err.class()
        );
        playlist.snapshot_id.clear();
    }
    Err(EditError::Mismatch)
}

fn uris(playables: &[Playable]) -> Vec<String> {
    playables.iter().map(Playable::uri).collect()
}

fn spaced(spacer: &mut Spacer) {
    let wait = spacer.next_wait(Instant::now());
    if !wait.is_zero() {
        sleep(wait);
    }
}

/// Append `new` at the end of the playlist, one POST per [ADD_BATCH] tracks. A failure after
/// the first batch verified is [EditError::Partial].
pub fn add_tracks(
    api: &impl PlaylistServer,
    playlist: &mut Playlist,
    new: &[Playable],
    known: Known<'_>,
) -> Result<(), EditError> {
    if new.is_empty() {
        return Ok(());
    }
    let base = prepare_head(api, playlist, known)?;
    add_batches(
        api,
        playlist,
        known,
        base,
        new,
        Post::First,
        &mut Spacer::new(PLAYLIST_FETCH_SPACING),
    )
    .map_err(|(added, err)| partial(added, new.len(), err))
}

/// The error of an add that failed after `added` of `total` tracks verified: the failure
/// itself when none did, else [EditError::Partial], unconfirmed when the failure is
/// [EditError::Unverified] (the failure's class is logged).
fn partial(added: usize, total: usize, err: EditError) -> EditError {
    if added == 0 {
        return err;
    }
    warn!(
        "Playlist add stopped after {added} of {total} tracks ({})",
        err.class()
    );
    EditError::Partial {
        added,
        total,
        unconfirmed: matches!(err, EditError::Unverified(_)),
    }
}

/// One verified ADD per [ADD_BATCH] tracks, [PLAYLIST_FETCH_SPACING] between POSTs, each based
/// on the head verified after the previous one. Batches before a failure stay applied and in
/// the local copy; the error carries how many tracks they hold.
fn add_batches(
    api: &impl PlaylistServer,
    playlist: &mut Playlist,
    known: Known<'_>,
    mut base: Base,
    new: &[Playable],
    mut post: Post,
    spacer: &mut Spacer,
) -> Result<(), (usize, EditError)> {
    let mut added = 0;
    for batch in add_chunks(new) {
        spaced(spacer);
        let expected = expected_len(base.length, Change::Add(batch.len())).unwrap_or_default();
        let ops = vec![add_op(&uris(batch), None)];
        let after = send_verified(api, playlist, known, post, base.revision, ops, expected)
            .map_err(|err| (added, err))?;
        post = Post::Later;
        added += batch.len();
        let mut tracks = playlist.tracks.take().unwrap_or_default();
        tracks.extend_from_slice(batch);
        set_tracks(playlist, tracks, &after.snapshot_id);
        base = after;
    }
    Ok(())
}

/// The local side of a Web API remove of the row at server position `index` with `uri`. The
/// Web call named that position in the snapshot from before the attempt; when the copy was
/// reloaded since, the item may sit elsewhere. So remove it only where it is still `uri`, else
/// clear the snapshot so the next write or open reloads the copy (see [reload_cleared]). A
/// copy left out of server order (one cached by older builds with a `list_index` twice) is
/// cleared too, so it is reloaded rather than kept.
pub fn remove_after_web(playlist: &mut Playlist, index: usize, uri: &str) {
    let removed = playlist
        .tracks
        .as_mut()
        .is_some_and(|tracks| remove_server_item(tracks, index, uri));
    playlist.num_tracks = playlist.tracks.as_ref().map_or(0, Vec::len);
    let ordered = playlist.tracks.as_deref().is_some_and(in_server_order);
    if !removed || !ordered {
        playlist.snapshot_id.clear();
    }
}

/// Remove the item at server position `index` (its `list_index`), which must still be `uri`
/// once the local copy is current.
pub fn remove_track(
    api: &impl PlaylistServer,
    playlist: &mut Playlist,
    index: usize,
    uri: &str,
    known: Known<'_>,
) -> Result<(), EditError> {
    let base = prepare_head(api, playlist, known)?;
    let tracks = playlist.tracks.as_deref().unwrap_or_default();
    if server_item(tracks, index, uri).is_none() {
        return Err(EditError::Moved);
    }
    let expected = expected_len(base.length, Change::Remove).ok_or(EditError::Moved)?;
    let ops = vec![remove_at_op(index)];
    let after = send_verified(
        api,
        playlist,
        known,
        Post::First,
        base.revision,
        ops,
        expected,
    )?;
    if let Some(tracks) = playlist.tracks.as_mut() {
        remove_server_item(tracks, index, uri);
        playlist.num_tracks = tracks.len();
    }
    playlist.snapshot_id = after.snapshot_id;
    Ok(())
}

/// Replace the contents with `tracks`: one atomic POST up to [OVERWRITE_ATOMIC_MAX] tracks,
/// above that the first POST carries the removal plus the first [OVERWRITE_ATOMIC_MAX] tracks
/// and the rest go as add batches ([EditError::Partial] when one of them fails).
///
/// Based on the head alone ([overwrite_base]): the old contents are never read, and a stale
/// local copy is simply replaced by `tracks`. Only a write that does not verify reloads.
pub fn overwrite(
    api: &impl PlaylistServer,
    playlist: &mut Playlist,
    tracks: &[Playable],
    known: Known<'_>,
) -> Result<(), EditError> {
    let base = overwrite_base(api.playlist_head(&playlist.id)?)?;
    let (first, rest) = overwrite_split(tracks);
    let ops = overwrite_ops(base.length, &uris(first));
    let mut spacer = Spacer::new(PLAYLIST_FETCH_SPACING);
    let after = if ops.is_empty() {
        base
    } else {
        spaced(&mut spacer);
        let expected =
            expected_len(base.length, Change::Overwrite(first.len())).unwrap_or_default();
        send_verified(
            api,
            playlist,
            known,
            Post::First,
            base.revision,
            ops,
            expected,
        )?
    };
    set_tracks(playlist, first.to_vec(), &after.snapshot_id);
    // `rest` is non-empty only when the head POST was sent, so its batches are later ones.
    add_batches(api, playlist, known, after, rest, Post::Later, &mut spacer)
        .map_err(|(added, err)| partial(first.len() + added, tracks.len(), err))
}

const PLAYLIST_URI_PREFIX: &str = "spotify:playlist:";

fn playlist_uri(id: &str) -> String {
    format!("{PLAYLIST_URI_PREFIX}{id}")
}

/// The rootlist index of `spotify:playlist:<id>`, counting folder markers as entries.
/// Items the parse left out of `entries` count too (through `Rootlist::skipped`).
pub fn rootlist_index(rootlist: &Rootlist, id: &str) -> Option<usize> {
    let entry = rootlist
        .entries
        .iter()
        .position(|entry| matches!(entry, RootEntry::Playlist(p) if p.id == id))?;
    // The entry is the `entry`-th raw item that was not skipped.
    let mut index = entry;
    for &skipped in &rootlist.skipped {
        if skipped > index {
            break;
        }
        index += 1;
    }
    Some(index)
}

/// What a rootlist write is based on.
struct RootBase {
    revision: Vec<u8>,
    length: usize,
}

/// The base of a full rootlist read; a read without a revision is a protocol change.
fn root_base(rootlist: &Rootlist) -> Result<RootBase, InternalError> {
    if rootlist.revision.is_empty() {
        return Err(InternalError::ProtocolChanged(
            "rootlist read without a revision".into(),
        ));
    }
    Ok(RootBase {
        revision: rootlist.revision.clone(),
        length: rootlist.length,
    })
}

/// The base a rootlist head gives: only when it carries both a revision and a length.
fn head_base(head: RootHead) -> Option<RootBase> {
    Some(RootBase {
        revision: head.revision?,
        length: head.length?,
    })
}

/// The base from the rootlist head; when the head lacks its revision or its length, from a
/// full rootlist read instead.
fn rootlist_base(api: &WebApi) -> Result<RootBase, InternalError> {
    if let Some(base) = head_base(api.rootlist_head()?) {
        return Ok(base);
    }
    debug!("rootlist head without a revision or length, reading the whole rootlist for the base");
    root_base(&api.playlist_rootlist()?)
}

/// The verify rule for a rootlist write: the head after it carries a length and it is
/// `expected_len`. A head without a length proves nothing ([EditError::Unverified]).
fn rootlist_verify(after: &RootHead, expected_len: usize) -> Result<(), EditError> {
    match after.length {
        Some(length) if length == expected_len => Ok(()),
        Some(length) => {
            warn!(
                "Rootlist write did not verify: length {length} after it, {expected_len} expected"
            );
            Err(EditError::Mismatch)
        }
        None => Err(EditError::Unverified(InternalError::Transient(
            "rootlist head without a length".into(),
        ))),
    }
}

/// POST `op` against the rootlist `base`, then confirm it with [rootlist_verify]. A failed
/// POST is classed by [post_error] at its position `post`; a failed verify read is
/// [EditError::Unverified].
fn send_rootlist_verified(
    api: &impl PlaylistServer,
    post: Post,
    base: RootBase,
    op: Op,
    expected_len: usize,
) -> Result<(), EditError> {
    send_post(api, post, || {
        api.rootlist_changes(list_changes(base.revision, vec![op]))
    })?;
    let after = api.rootlist_head().map_err(EditError::Unverified)?;
    rootlist_verify(&after, expected_len)
}

/// Add `spotify:playlist:<id>` on top of `base`, verified.
fn rootlist_add_on(api: &WebApi, base: RootBase, id: &str) -> Result<(), EditError> {
    let expected = expected_len(base.length, Change::Add(1)).unwrap_or_default();
    send_rootlist_verified(
        api,
        Post::First,
        base,
        rootlist_add_op(&playlist_uri(id)),
        expected,
    )
}

/// Add `spotify:playlist:<id>` at the top of the rootlist, verified.
fn rootlist_add(api: &WebApi, id: &str) -> Result<(), EditError> {
    rootlist_add_on(api, rootlist_base(api)?, id)
}

/// The second rootlist add of a create: a full rootlist read first, so an earlier add that
/// applied without being confirmed is not repeated; the read is also the base.
fn rootlist_add_again(api: &WebApi, id: &str) -> Result<(), EditError> {
    let rootlist = api.playlist_rootlist()?;
    if rootlist_index(&rootlist, id).is_some() {
        debug!("the new playlist is in the rootlist already");
        return Ok(());
    }
    rootlist_add_on(api, root_base(&rootlist)?, id)
}

/// The class of a failed create POST. A protocol change there may be a 2xx reply that does not
/// parse, so the playlist may exist: [EditError::Unverified], never a Web create (the caller
/// latches the area). Other classes keep their meaning: a refusal (not found, denied) may use
/// the Web API once, a transient error or rate limit fails.
fn create_error(err: InternalError) -> EditError {
    match err {
        InternalError::ProtocolChanged(_) => EditError::Unverified(err),
        err => EditError::Internal(err),
    }
}

/// Latch the playlists area on a protocol change found before a rootlist POST.
fn latch_on_protocol_change(api: &WebApi, err: &EditError) {
    if let EditError::Internal(err @ InternalError::ProtocolChanged(_)) = err {
        api.latch_area(Area::Playlists, err);
    }
}

/// The stored copy of a playlist just created: empty, owned by `owner`, at the create
/// reply's revision.
pub fn created_playlist(created: &Created, name: &str, owner: Option<&str>) -> Playlist {
    Playlist {
        id: created.id.clone(),
        name: name.to_string(),
        owner_id: owner.unwrap_or_default().to_string(),
        owner_name: None,
        snapshot_id: base64_encode(&created.revision),
        num_tracks: 0,
        tracks: Some(vec![]),
        collaborative: false,
        cover_url: None,
    }
}

/// A failure once the playlist exists: it must never lead to a second create through the Web
/// API, so even an error from before the rootlist POST counts as [EditError::Unverified].
fn after_create(err: EditError) -> EditError {
    match err {
        EditError::Internal(err) => EditError::Unverified(err),
        other => other,
    }
}

/// Create a playlist and add it to the top of the rootlist; returns the stored copy.
///
/// A failed rootlist add is retried once, then the Web API follow (a second add of the same
/// new playlist is harmless). If all fail the playlist exists but is not in the library.
pub fn create(api: &WebApi, name: &str) -> Result<Playlist, EditError> {
    let mut spacer = Spacer::new(PLAYLIST_FETCH_SPACING);
    spaced(&mut spacer);
    let created = api.playlist_create(name).map_err(|err| {
        if let InternalError::ProtocolChanged(_) = err {
            api.latch_area(Area::Playlists, &err);
        }
        create_error(err)
    })?;
    let playlist = created_playlist(&created, name, api.user_id());
    spaced(&mut spacer);
    let added = rootlist_add(api, &created.id)
        .or_else(|err| {
            latch_on_protocol_change(api, &err);
            warn!(
                "Could not add the new playlist to the rootlist ({}), trying again",
                err.class()
            );
            spaced(&mut spacer);
            rootlist_add_again(api, &created.id)
        })
        .or_else(|err| {
            latch_on_protocol_change(api, &err);
            warn!(
                "Could not add the new playlist to the rootlist ({}), following it over the Web API",
                err.class()
            );
            api.count_fallback(Area::Playlists);
            api.user_playlist_follow_playlist(&created.id)
                .map_err(|()| err)
        });
    match added {
        Ok(()) => Ok(playlist),
        Err(err) => {
            warn!("Playlist created but not added to your library");
            Err(after_create(err))
        }
    }
}

/// Add playlist `id` to the top of the rootlist.
pub fn follow(api: &WebApi, id: &str) -> Result<(), EditError> {
    rootlist_add(api, id)
}

/// What a delete does after one full rootlist read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootlistStep {
    /// Remove the (first) copy of the playlist at this rootlist index.
    Remove(usize),
    /// The complete listing does not hold the playlist: nothing (more) to remove.
    Absent,
    /// The read stopped at the page cap: a copy may lie past it, so nothing may be sent.
    Incomplete,
}

fn rootlist_step(rootlist: &Rootlist, id: &str) -> RootlistStep {
    if rootlist.read < rootlist.length {
        return RootlistStep::Incomplete;
    }
    rootlist_index(rootlist, id).map_or(RootlistStep::Absent, RootlistStep::Remove)
}

fn rootlist_copies(rootlist: &Rootlist, id: &str) -> usize {
    rootlist
        .entries
        .iter()
        .filter(|entry| matches!(entry, RootEntry::Playlist(p) if p.id == id))
        .count()
}

/// Remove every rootlist copy of `id`: read the whole rootlist, remove the first copy at the
/// index found there (verified by the head length), and read again until no copy is left. A
/// rootlist the playlist is not in needs no POST; a read that stopped at the page cap fails
/// before any POST ([EditError::TooLong]).
///
/// Every POST after the first verified remove is a later POST, and any failure after it is
/// [EditError::PartlyDeleted] (never repeated through the Web API; the next sync reconciles
/// the store). At most as many removes are sent as the first read listed copies; one still
/// listed after them fails with [EditError::Mismatch] (the list changed elsewhere).
pub fn unfollow(api: &impl PlaylistServer, id: &str) -> Result<(), EditError> {
    let mut spacer = Spacer::new(PLAYLIST_FETCH_SPACING);
    let mut removed = 0;
    let mut listed = None;
    loop {
        let post = if removed == 0 {
            Post::First
        } else {
            Post::Later
        };
        let fail = |err: EditError| match post {
            Post::First => err,
            Post::Later => {
                warn!(
                    "Removed {removed} rootlist copies, then the delete failed ({})",
                    err.class()
                );
                EditError::PartlyDeleted
            }
        };
        spaced(&mut spacer);
        let rootlist = api.playlist_rootlist().map_err(|err| fail(err.into()))?;
        let index = match rootlist_step(&rootlist, id) {
            RootlistStep::Absent => {
                if removed == 0 {
                    debug!("the playlist is not in the rootlist, nothing to remove");
                }
                return Ok(());
            }
            RootlistStep::Incomplete => return Err(fail(EditError::TooLong)),
            RootlistStep::Remove(index) => index,
        };
        if removed >= *listed.get_or_insert_with(|| rootlist_copies(&rootlist, id)) {
            warn!("Playlist still in the rootlist after removing every copy listed");
            return Err(fail(EditError::Mismatch));
        }
        // The index walks the raw items, so it is the playlist's own; it must also lie inside
        // the listing the service reported.
        if index >= rootlist.length {
            return Err(fail(EditError::Moved));
        }
        let base = root_base(&rootlist).map_err(|err| fail(err.into()))?;
        let expected = expected_len(base.length, Change::Remove)
            .ok_or(EditError::Moved)
            .map_err(fail)?;
        send_rootlist_verified(api, post, base, rootlist_remove_op(index), expected)
            .map_err(fail)?;
        removed += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;
    use crate::internal_api::playlist::{RootEntry, RootPlaylist};
    use crate::playlist_items::placeholder;

    fn track_playable(uri: &str) -> Playable {
        placeholder(uri)
    }

    fn test_playlist(id: &str) -> Playlist {
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

    /// A playable at server position `index`.
    fn at(uri: &str, index: usize) -> Playable {
        let mut playable = track_playable(uri);
        playable.set_list_index(index);
        playable
    }

    fn local(snapshot: &str, n: usize) -> Playlist {
        Playlist {
            snapshot_id: snapshot.into(),
            tracks: Some(
                (0..n)
                    .map(|i| at(&format!("spotify:track:{i}"), i))
                    .collect(),
            ),
            num_tracks: n,
            ..test_playlist("p")
        }
    }

    fn head(snapshot: &str, length: usize) -> Head {
        Head {
            revision: vec![1],
            snapshot_id: snapshot.into(),
            length: Some(length),
            name: None,
        }
    }

    #[test]
    fn freshness_needs_loaded_tracks_same_snapshot_and_count() {
        assert_eq!(freshness(&local("s", 2), &head("s", 2)), Freshness::Current);
        assert_eq!(freshness(&local("s", 2), &head("t", 2)), Freshness::Stale);
        assert_eq!(freshness(&local("s", 2), &head("s", 3)), Freshness::Stale);
        let unloaded = Playlist {
            tracks: None,
            ..local("s", 0)
        };
        assert_eq!(freshness(&unloaded, &head("s", 0)), Freshness::Stale);
    }

    #[test]
    fn a_head_without_a_length_is_stale() {
        let no_length = Head {
            length: None,
            ..head("s", 0)
        };
        assert_eq!(freshness(&local("s", 0), &no_length), Freshness::Stale);
    }

    #[test]
    fn a_read_without_a_revision_is_a_protocol_change() {
        let no_revision = Head {
            revision: Vec::new(),
            ..head("", 2)
        };
        assert!(matches!(
            listing_base(no_revision.clone(), 2),
            Err(InternalError::ProtocolChanged(_))
        ));
        assert_eq!(
            require_revision(head("s", 2), "playlist head"),
            Ok(head("s", 2))
        );
        let err = require_revision(no_revision, "playlist head").unwrap_err();
        assert!(matches!(err, InternalError::ProtocolChanged(_)));
        // Before any POST, so it keeps its class: the area latches, then the Web API.
        use crate::internal_api::fallback::{Action, decide_after};
        let err = InternalError::from(EditError::from(err));
        assert_eq!(decide_after(&err), Action::LatchThenWeb);
    }

    #[test]
    fn a_reload_is_based_on_its_item_count_and_revision() {
        let read = Head {
            revision: b"reload".to_vec(),
            ..head("r", 5)
        };
        let base = listing_base(read, 3).expect("has a revision");
        assert_eq!(
            base,
            Base {
                revision: b"reload".to_vec(),
                snapshot_id: "r".into(),
                length: 3,
            }
        );
    }

    #[test]
    fn a_write_on_a_current_copy_is_based_on_the_head() {
        let read = Head {
            revision: b"head".to_vec(),
            ..head("h", 2)
        };
        assert_eq!(
            current_base(read, &local("h", 2)),
            Base {
                revision: b"head".to_vec(),
                snapshot_id: "h".into(),
                length: 2,
            }
        );
    }

    #[test]
    fn length_drift_compares_a_reported_length_with_the_items() {
        assert_eq!(length_drift(&head("s", 3), 3), None);
        assert_eq!(length_drift(&head("s", 4), 3), Some(4));
        let no_length = Head {
            length: None,
            ..head("s", 0)
        };
        assert_eq!(length_drift(&no_length, 3), None);
    }

    #[test]
    fn the_server_item_is_found_by_its_list_index() {
        let tracks = local("s", 3).tracks.unwrap();
        assert_eq!(server_item(&tracks, 1, "spotify:track:1"), Some(1));
        assert_eq!(server_item(&tracks, 1, "spotify:track:2"), None);
        assert_eq!(server_item(&tracks, 9, "spotify:track:1"), None);
        let reversed: Vec<Playable> = tracks.into_iter().rev().collect();
        assert_eq!(server_item(&reversed, 0, "spotify:track:0"), Some(2));
    }

    #[test]
    fn removing_a_server_item_keeps_the_order_and_renumbers() {
        // Server order a0 b1 c2 d3, held as c2 a0 d3 b1.
        let mut tracks = vec![
            at("spotify:track:c", 2),
            at("spotify:track:a", 0),
            at("spotify:track:d", 3),
            at("spotify:track:b", 1),
        ];
        assert!(
            !remove_server_item(&mut tracks, 1, "spotify:track:a"),
            "another uri"
        );
        assert_eq!(tracks.len(), 4);
        assert!(remove_server_item(&mut tracks, 1, "spotify:track:b"));
        let held: Vec<(String, usize)> = tracks.iter().map(|p| (p.uri(), p.list_index())).collect();
        assert_eq!(
            held,
            [
                ("spotify:track:c".to_string(), 1),
                ("spotify:track:a".to_string(), 0),
                ("spotify:track:d".to_string(), 2),
            ]
        );
        assert!(!remove_server_item(&mut tracks, 7, "spotify:track:c"));
    }

    #[test]
    fn a_copy_out_of_server_order_is_stale() {
        let mut playlist = local("s", 3);
        playlist.tracks.as_mut().unwrap().swap(0, 2);
        assert_eq!(freshness(&playlist, &head("s", 3)), Freshness::Stale);
    }

    #[test]
    fn verify_compares_the_head_length() {
        assert_eq!(
            verify(3, head("x", 3)),
            Verify::Applied(Base {
                revision: vec![1],
                snapshot_id: "x".into(),
                length: 3,
            })
        );
        assert_eq!(verify(3, head("x", 2)), Verify::Differs(2));
    }

    #[test]
    fn a_verify_head_without_revision_or_length_proves_nothing() {
        let no_length = Head {
            length: None,
            ..head("x", 0)
        };
        let no_revision = Head {
            revision: Vec::new(),
            ..head("", 3)
        };
        assert_eq!(verify(0, no_length), Verify::Unreadable);
        assert_eq!(verify(3, no_revision), Verify::Unreadable);
    }

    #[test]
    fn an_unconfirmed_write_clears_the_snapshot_and_stores_the_copy() {
        let mut playlist = local("s", 2);
        let err = unconfirmed(&mut playlist, InternalError::Transient("x".into()));
        assert!(matches!(err, EditError::Unverified(_)));
        assert!(err.stores_copy());
        assert_eq!(
            err.note().as_deref(),
            Some("Could not confirm the change; it may have applied")
        );
        assert_eq!(playlist.snapshot_id, "");
        assert_eq!(playlist.num_tracks, 2, "the tracks stay");
        // The next write sees the copy as stale and reloads.
        assert_eq!(freshness(&playlist, &head("s", 2)), Freshness::Stale);
    }

    #[test]
    fn post_errors_by_position_and_class() {
        use crate::internal_api::fallback::{Action, decide_after};
        // (error, first POST: class and action, later batch: class and action)
        let cases = [
            (
                InternalError::Transient("timeout".into()),
                ("unverified", Action::Fail),
            ),
            (InternalError::RateLimited, ("unverified", Action::Fail)),
            (InternalError::NotFound, ("refused", Action::Fail)),
            (InternalError::Denied, ("refused", Action::Fail)),
            (
                InternalError::ProtocolChanged("412".into()),
                ("refused", Action::Fail),
            ),
            (InternalError::NoSession, ("no session", Action::WebOnce)),
        ];
        let note = |class: &str| match class {
            "refused" => Some("Spotify refused the change"),
            "unverified" => Some("Could not confirm the change; it may have applied"),
            _ => None,
        };
        for (inner, (class, action)) in cases {
            let first = post_error(inner.clone(), Post::First);
            assert_eq!(first.class(), class, "first {inner}");
            assert_eq!(first.note().as_deref(), note(class), "first {inner}");
            assert_eq!(first.stores_copy(), class == "unverified", "first {inner}");
            assert_eq!(
                decide_after(&InternalError::from(first)),
                action,
                "first {inner}"
            );

            let later = post_error(inner.clone(), Post::Later);
            assert_eq!(later.class(), "unverified", "later {inner}");
            assert_eq!(later.note().as_deref(), note("unverified"), "later {inner}");
            assert!(
                later.stores_copy(),
                "later {inner}: the verified batches are stored"
            );
            assert_eq!(
                decide_after(&InternalError::from(later)),
                Action::Fail,
                "later {inner}"
            );
        }
    }

    #[test]
    fn an_unsent_post_keeps_its_class_only_when_first() {
        use crate::internal_api::fallback::{Action, decide_after};
        let first = unsent_post(Post::First);
        assert!(matches!(
            first,
            EditError::Internal(InternalError::ProtocolChanged(_))
        ));
        assert_eq!(
            decide_after(&InternalError::from(first)),
            Action::LatchThenWeb
        );
        let later = unsent_post(Post::Later);
        assert!(later.stores_copy());
        assert_eq!(decide_after(&InternalError::from(later)), Action::Fail);
    }

    #[test]
    fn a_post_on_a_latched_area_is_not_sent() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        api.latch_area(Area::Playlists, &InternalError::ProtocolChanged("x".into()));
        let sent = std::cell::Cell::new(false);
        let result = send_post(&api, Post::First, || {
            sent.set(true);
            Ok(())
        });
        assert!(!sent.get());
        assert!(matches!(
            result,
            Err(EditError::Internal(InternalError::ProtocolChanged(_)))
        ));
    }

    #[test]
    fn edit_errors_fail_without_fallback() {
        use crate::internal_api::fallback::{Action, decide_after};
        for (err, stores) in [
            (EditError::Moved, false),
            (EditError::Refused(InternalError::Denied), false),
            (EditError::Mismatch, true),
            (
                EditError::Partial {
                    added: 100,
                    total: 150,
                    unconfirmed: true,
                },
                true,
            ),
        ] {
            assert_eq!(err.stores_copy(), stores, "{}", err.class());
            assert_eq!(decide_after(&InternalError::from(err)), Action::Fail);
        }
    }

    #[test]
    fn unverified_errors_fail_for_every_class() {
        use crate::internal_api::fallback::{Action, decide_after};
        for inner in [
            InternalError::NoSession,
            InternalError::NotFound,
            InternalError::Denied,
            InternalError::ProtocolChanged("412".into()),
            InternalError::RateLimited,
            InternalError::Transient("x".into()),
        ] {
            let err = InternalError::from(EditError::Unverified(inner.clone()));
            assert!(matches!(err, InternalError::Transient(_)), "{inner}");
            assert_eq!(decide_after(&err), Action::Fail, "{inner}");
        }
    }

    #[test]
    fn errors_before_the_post_keep_their_class() {
        for inner in [
            InternalError::NoSession,
            InternalError::NotFound,
            InternalError::Denied,
            InternalError::ProtocolChanged("412".into()),
        ] {
            assert_eq!(
                InternalError::from(EditError::Internal(inner.clone())),
                inner
            );
        }
    }

    #[test]
    fn expected_length_per_change() {
        assert_eq!(expected_len(5, Change::Add(3)), Some(8));
        assert_eq!(expected_len(5, Change::Remove), Some(4));
        assert_eq!(expected_len(0, Change::Remove), None);
        assert_eq!(expected_len(5, Change::Overwrite(2)), Some(2));
        assert_eq!(expected_len(5, Change::Overwrite(0)), Some(0));
    }

    #[test]
    fn overwrite_sends_up_to_the_atomic_max_then_add_batches() {
        let small: Vec<usize> = (0..250).collect();
        let (first, rest) = overwrite_split(&small);
        assert_eq!((first.len(), rest.len()), (250, 0));

        let big: Vec<usize> = (0..OVERWRITE_ATOMIC_MAX + 250).collect();
        let (first, rest) = overwrite_split(&big);
        assert_eq!(first.len(), OVERWRITE_ATOMIC_MAX);
        assert_eq!(rest[0], OVERWRITE_ATOMIC_MAX, "the tail continues in order");
        let sizes: Vec<usize> = add_chunks(rest).map(<[usize]>::len).collect();
        assert_eq!(sizes, [ADD_BATCH, ADD_BATCH, 50]);
    }

    #[test]
    fn add_batches_are_add_batch_sized() {
        let new: Vec<usize> = (0..ADD_BATCH * 2 + 1).collect();
        let sizes: Vec<usize> = add_chunks(&new).map(<[usize]>::len).collect();
        assert_eq!(sizes, [ADD_BATCH, ADD_BATCH, 1]);
        assert_eq!(add_chunks::<usize>(&[]).count(), 0);
    }

    #[test]
    fn set_tracks_renumbers_and_takes_the_snapshot() {
        let mut playlist = local("s", 0);
        let tracks = vec![
            track_playable("spotify:track:a"),
            track_playable("spotify:track:b"),
        ];
        set_tracks(&mut playlist, tracks.into_iter().rev().collect(), "t");
        assert_eq!(playlist.snapshot_id, "t");
        assert_eq!(playlist.num_tracks, 2);
        let tracks = playlist.tracks.unwrap();
        assert_eq!(tracks[0].uri(), "spotify:track:b");
        for (i, p) in tracks.iter().enumerate() {
            assert_eq!(p.list_index(), i);
        }
    }

    fn root_pl(id: &str) -> RootEntry {
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

    #[test]
    fn rootlist_index_counts_folder_markers() {
        let rootlist = Rootlist {
            entries: vec![
                RootEntry::FolderStart { name: "F".into() },
                root_pl("a"),
                RootEntry::FolderEnd,
                root_pl("b"),
            ],
            length: 4,
            read: 4,
            pages: 1,
            ..Default::default()
        };
        assert_eq!(rootlist_index(&rootlist, "b"), Some(3));
        assert_eq!(rootlist_index(&rootlist, "a"), Some(1));
        assert_eq!(rootlist_index(&rootlist, "missing"), None);
    }

    #[test]
    fn rootlist_index_counts_skipped_items() {
        // Raw items: a, <track>, <track>, b, <track>, c
        let rootlist = Rootlist {
            entries: vec![root_pl("a"), root_pl("b"), root_pl("c")],
            skipped: vec![1, 2, 4],
            length: 6,
            read: 6,
            pages: 1,
            ..Default::default()
        };
        assert_eq!(rootlist_index(&rootlist, "a"), Some(0));
        assert_eq!(rootlist_index(&rootlist, "b"), Some(3));
        assert_eq!(rootlist_index(&rootlist, "c"), Some(5));
    }

    #[test]
    fn create_without_a_session_sends_nothing() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        assert!(matches!(
            create(&api, "ncspot-test-1"),
            Err(EditError::Internal(InternalError::NoSession))
        ));
    }

    #[test]
    fn follow_and_unfollow_without_a_session_send_nothing() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        assert!(matches!(
            follow(&api, "p"),
            Err(EditError::Internal(InternalError::NoSession))
        ));
        assert!(matches!(
            unfollow(&api, "p"),
            Err(EditError::Internal(InternalError::NoSession))
        ));
    }

    #[test]
    fn errors_after_the_create_never_allow_a_second_create() {
        use crate::internal_api::fallback::{Action, decide_after};
        for err in [
            EditError::Internal(InternalError::NoSession),
            EditError::Internal(InternalError::ProtocolChanged("x".into())),
            EditError::Unverified(InternalError::Denied),
            EditError::Refused(InternalError::NotFound),
            EditError::Mismatch,
        ] {
            let after = after_create(err);
            assert!(!matches!(after, EditError::Internal(_)));
            assert_eq!(decide_after(&InternalError::from(after)), Action::Fail);
        }
    }

    #[test]
    fn created_playlist_is_empty_and_owned() {
        let created = Created {
            id: "abc".into(),
            revision: b"rev1".to_vec(),
        };
        let playlist = created_playlist(&created, "ncspot-test-1", Some("me"));
        assert_eq!(playlist.id, "abc");
        assert_eq!(playlist.snapshot_id, "cmV2MQ==");
        assert_eq!(playlist.owner_id, "me");
        assert_eq!(playlist.name, "ncspot-test-1");
        assert_eq!(playlist.num_tracks, 0);
        assert_eq!(playlist.tracks.as_deref().map(<[Playable]>::len), Some(0));
    }

    #[test]
    fn a_protocol_change_on_create_never_creates_again() {
        use crate::internal_api::fallback::{Action, decide_after};
        let class = |err| decide_after(&InternalError::from(create_error(err)));
        assert_eq!(
            class(InternalError::ProtocolChanged("reply".into())),
            Action::Fail
        );
        assert_eq!(class(InternalError::NotFound), Action::WebOnce);
        assert_eq!(class(InternalError::Denied), Action::WebOnce);
        assert_eq!(class(InternalError::NoSession), Action::WebOnce);
        assert_eq!(
            class(InternalError::Transient("timeout".into())),
            Action::Fail
        );
        assert_eq!(class(InternalError::RateLimited), Action::Fail);
    }

    #[test]
    fn a_rootlist_head_is_a_base_only_with_revision_and_length() {
        let head = |revision: Option<&[u8]>, length| RootHead {
            revision: revision.map(<[u8]>::to_vec),
            length,
        };
        let base = head_base(head(Some(b"r"), Some(4))).expect("both fields");
        assert_eq!((base.revision, base.length), (b"r".to_vec(), 4));
        assert!(head_base(head(None, Some(4))).is_none());
        assert!(head_base(head(Some(b"r"), None)).is_none());
    }

    #[test]
    fn a_rootlist_head_without_length_does_not_verify_or_mismatch() {
        let after = |length| RootHead {
            revision: Some(b"r".to_vec()),
            length,
        };
        assert!(rootlist_verify(&after(Some(5)), 5).is_ok());
        assert!(matches!(
            rootlist_verify(&after(Some(4)), 5),
            Err(EditError::Mismatch)
        ));
        assert!(matches!(
            rootlist_verify(&after(None), 0),
            Err(EditError::Unverified(_))
        ));
    }

    #[test]
    fn error_classes_log_fixed_names() {
        assert_eq!(
            EditError::Internal(InternalError::Transient("/user/x".into())).class(),
            "transient"
        );
        assert_eq!(
            EditError::Unverified(InternalError::Transient("/user/x".into())).class(),
            "unverified"
        );
        assert_eq!(
            EditError::Refused(InternalError::ProtocolChanged("/user/x".into())).class(),
            "refused"
        );
    }

    #[test]
    fn add_tracks_without_a_session_leaves_the_playlist_unchanged() {
        let api = WebApi::new(&crate::config::Config::new_for_test());
        let mut playlist = local("s", 2);
        let new = [track_playable("spotify:track:new")];
        let built = std::cell::Cell::new(0);
        let known = || {
            built.set(built.get() + 1);
            HashMap::new()
        };
        let result = add_tracks(&api, &mut playlist, &new, &known);
        assert!(matches!(
            result,
            Err(EditError::Internal(InternalError::NoSession))
        ));
        assert_eq!(
            built.get(),
            0,
            "no reload, so the known metadata is never built"
        );
        assert_eq!(playlist.snapshot_id, "s");
        assert_eq!(playlist.num_tracks, 2);
        let uris: Vec<String> = playlist.tracks.unwrap().iter().map(Playable::uri).collect();
        assert_eq!(uris, ["spotify:track:0", "spotify:track:1"]);
    }

    type ContentsRead = Result<(Head, Vec<Playable>), InternalError>;
    type RangeRead = Result<(Head, Vec<String>), InternalError>;

    /// A scripted server: each call takes the next queued reply (a call with none queued
    /// panics) and is recorded.
    #[derive(Default)]
    struct Fake {
        heads: RefCell<VecDeque<Result<Head, InternalError>>>,
        contents: RefCell<VecDeque<ContentsRead>>,
        #[expect(dead_code)]
        ranges: RefCell<VecDeque<RangeRead>>,
        /// `(from, length)` of each range read, in order.
        #[expect(dead_code)]
        range_reads: RefCell<Vec<(usize, usize)>>,
        posts: RefCell<VecDeque<Result<(), InternalError>>>,
        sent: RefCell<Vec<ListChanges>>,
        contents_reads: std::cell::Cell<usize>,
        rootlists: RefCell<VecDeque<Result<Rootlist, InternalError>>>,
        root_heads: RefCell<VecDeque<Result<RootHead, InternalError>>>,
        /// The playlists area is off or latched.
        unusable: std::cell::Cell<bool>,
        latches: std::cell::Cell<usize>,
    }

    impl Fake {
        fn heads(self, heads: impl IntoIterator<Item = Result<Head, InternalError>>) -> Self {
            self.heads.borrow_mut().extend(heads);
            self
        }

        fn posts(self, posts: impl IntoIterator<Item = Result<(), InternalError>>) -> Self {
            self.posts.borrow_mut().extend(posts);
            self
        }

        #[expect(dead_code)]
        fn ranges(self, reads: impl IntoIterator<Item = RangeRead>) -> Self {
            self.ranges.borrow_mut().extend(reads);
            self
        }

        fn contents(self, read: ContentsRead) -> Self {
            self.contents.borrow_mut().push_back(read);
            self
        }

        fn rootlists(
            self,
            reads: impl IntoIterator<Item = Result<Rootlist, InternalError>>,
        ) -> Self {
            self.rootlists.borrow_mut().extend(reads);
            self
        }

        fn root_heads(
            self,
            heads: impl IntoIterator<Item = Result<RootHead, InternalError>>,
        ) -> Self {
            self.root_heads.borrow_mut().extend(heads);
            self
        }

        /// The `from_index` of each POST's remove op, in order.
        fn removed_at(&self) -> Vec<Option<i32>> {
            self.sent
                .borrow()
                .iter()
                .map(|body| body.deltas[0].ops[0].rem.from_index)
                .collect()
        }
    }

    impl PlaylistServer for Fake {
        fn playlist_head(&self, _id: &str) -> Result<Head, InternalError> {
            self.heads.borrow_mut().pop_front().expect("a queued head")
        }

        fn playlist_playables_at(
            &self,
            _id: &str,
            _known: &HashMap<String, Playable>,
        ) -> ContentsRead {
            self.contents_reads.set(self.contents_reads.get() + 1);
            self.contents
                .borrow_mut()
                .pop_front()
                .expect("a queued contents read")
        }

        fn playlist_range(&self, _id: &str, from: usize, length: usize) -> RangeRead {
            self.range_reads.borrow_mut().push((from, length));
            self.ranges
                .borrow_mut()
                .pop_front()
                .expect("a queued range read")
        }

        fn playlist_changes(&self, _id: &str, body: ListChanges) -> Result<(), InternalError> {
            self.sent.borrow_mut().push(body);
            self.posts
                .borrow_mut()
                .pop_front()
                .expect("a queued POST reply")
        }

        fn playlist_rootlist(&self) -> Result<Rootlist, InternalError> {
            self.rootlists
                .borrow_mut()
                .pop_front()
                .expect("a queued rootlist read")
        }

        fn rootlist_head(&self) -> Result<RootHead, InternalError> {
            self.root_heads
                .borrow_mut()
                .pop_front()
                .expect("a queued rootlist head")
        }

        fn rootlist_changes(&self, body: ListChanges) -> Result<(), InternalError> {
            self.sent.borrow_mut().push(body);
            self.posts
                .borrow_mut()
                .pop_front()
                .expect("a queued POST reply")
        }

        fn internal_usable(&self, _area: Area) -> bool {
            !self.unusable.get()
        }

        fn latch_area(&self, _area: Area, _err: &InternalError) {
            self.latches.set(self.latches.get() + 1);
        }
    }

    fn no_known() -> HashMap<String, Playable> {
        HashMap::new()
    }

    fn new_tracks(n: usize) -> Vec<Playable> {
        (0..n)
            .map(|i| track_playable(&format!("spotify:track:new{i}")))
            .collect()
    }

    fn uri_list(playlist: &Playlist) -> Vec<String> {
        uris(playlist.tracks.as_deref().unwrap_or_default())
    }

    #[test]
    fn a_later_batch_failure_is_partial_and_keeps_the_verified_batches() {
        use crate::internal_api::fallback::{Action, decide_after};
        let new = new_tracks(ADD_BATCH + 50);
        let api = Fake::default()
            .heads([Ok(head("s", 2)), Ok(head("t", ADD_BATCH + 2))])
            .posts([Ok(()), Err(InternalError::Transient("timeout".into()))]);
        let mut playlist = local("s", 2);
        let err = add_tracks(&api, &mut playlist, &new, &no_known).unwrap_err();
        assert!(matches!(
            err,
            EditError::Partial {
                added: ADD_BATCH,
                total: 150,
                unconfirmed: true,
            }
        ));
        // The failed batch may have applied too.
        assert_eq!(
            err.note().as_deref(),
            Some("Added at least 100 of 150 tracks")
        );
        assert!(err.stores_copy());
        assert_eq!(decide_after(&InternalError::from(err)), Action::Fail);
        // The copy holds exactly the verified batch, at the head verified after it.
        assert_eq!(playlist.num_tracks, ADD_BATCH + 2);
        assert_eq!(playlist.snapshot_id, "t");
        assert_eq!(uri_list(&playlist)[2..], uris(&new[..ADD_BATCH]));
        assert_eq!(api.contents_reads.get(), 0);
    }

    #[test]
    fn a_later_batch_mismatch_is_partial_and_stores_the_reloaded_copy() {
        let new = new_tracks(ADD_BATCH + 50);
        let server_copy: Vec<Playable> = new_tracks(5);
        let api = Fake::default()
            .heads([
                Ok(head("s", 2)),
                Ok(head("t", ADD_BATCH + 2)),
                Ok(head("x", 7)),
            ])
            .posts([Ok(()), Ok(())])
            .contents(Ok((head("r", 5), server_copy.clone())));
        let mut playlist = local("s", 2);
        let err = add_tracks(&api, &mut playlist, &new, &no_known).unwrap_err();
        assert_eq!(err.note().as_deref(), Some("Added 100 of 150 tracks"));
        assert!(err.stores_copy());
        assert_eq!(playlist.snapshot_id, "r");
        assert_eq!(uri_list(&playlist), uris(&server_copy));
    }

    #[test]
    fn a_first_batch_failure_is_not_partial() {
        let api = Fake::default()
            .heads([Ok(head("s", 2))])
            .posts([Err(InternalError::Denied)]);
        let mut playlist = local("s", 2);
        let err = add_tracks(&api, &mut playlist, &new_tracks(3), &no_known).unwrap_err();
        assert!(matches!(err, EditError::Refused(InternalError::Denied)));
        assert_eq!(playlist.num_tracks, 2);
        assert_eq!(partial(0, 3, EditError::Mismatch).class(), "mismatch");
    }

    #[test]
    fn a_mismatch_stores_the_reloaded_copy() {
        let server_copy = new_tracks(3);
        let api = Fake::default()
            .heads([Ok(head("s", 2)), Ok(head("x", 2))])
            .posts([Ok(())])
            .contents(Ok((head("r", 3), server_copy.clone())));
        let mut playlist = local("s", 2);
        let err = remove_track(&api, &mut playlist, 0, "spotify:track:0", &no_known).unwrap_err();
        assert!(matches!(err, EditError::Mismatch));
        assert!(err.stores_copy());
        assert_eq!(playlist.snapshot_id, "r");
        assert_eq!(uri_list(&playlist), uris(&server_copy));
    }

    #[test]
    fn a_mismatch_without_a_reload_clears_the_snapshot() {
        let api = Fake::default()
            .heads([Ok(head("s", 2)), Ok(head("x", 2))])
            .posts([Ok(())])
            .contents(Err(InternalError::Transient("timeout".into())));
        let mut playlist = local("s", 2);
        let err = remove_track(&api, &mut playlist, 0, "spotify:track:0", &no_known).unwrap_err();
        assert!(matches!(err, EditError::Mismatch));
        assert!(err.stores_copy());
        assert_eq!(playlist.snapshot_id, "");
        assert_eq!(freshness(&playlist, &head("s", 2)), Freshness::Stale);
    }

    #[test]
    fn an_overwrite_of_a_stale_copy_reads_only_the_head() {
        let new = new_tracks(2);
        let read = Head {
            revision: b"head".to_vec(),
            ..head("new", 3)
        };
        let api = Fake::default()
            .heads([Ok(read), Ok(head("w", 2))])
            .posts([Ok(())]);
        // Stale: another snapshot and another length than the head.
        let mut playlist = local("old", 5);
        overwrite(&api, &mut playlist, &new, &no_known).expect("verified");
        assert_eq!(api.contents_reads.get(), 0, "no contents read");
        let sent = api.sent.borrow();
        assert_eq!(sent[0].base_revision.as_deref(), Some(&b"head"[..]));
        let rem = &sent[0].deltas[0].ops[0].rem;
        assert_eq!(
            (rem.from_index, rem.length),
            (Some(0), Some(3)),
            "removes the head length, not the stale copy's"
        );
        assert_eq!(playlist.snapshot_id, "w");
        assert_eq!(uri_list(&playlist), uris(&new));
    }

    #[test]
    fn an_overwrite_needs_a_head_with_revision_and_length() {
        let no_length = Head {
            length: None,
            ..head("s", 0)
        };
        let api = Fake::default().heads([Ok(no_length)]);
        let mut playlist = local("s", 2);
        let err = overwrite(&api, &mut playlist, &new_tracks(1), &no_known).unwrap_err();
        // A transient failure: nothing latches, and there is no Web fallback.
        assert!(matches!(
            err,
            EditError::Internal(InternalError::Transient(_))
        ));
        {
            use crate::internal_api::fallback::{Action, decide_after};
            assert_eq!(decide_after(&InternalError::from(err)), Action::Fail);
        }
        assert!(api.sent.borrow().is_empty(), "nothing sent");
        assert_eq!(api.contents_reads.get(), 0);
        assert_eq!(playlist.num_tracks, 2);
        let no_revision = Head {
            revision: Vec::new(),
            ..head("", 2)
        };
        assert!(matches!(
            overwrite_base(no_revision),
            Err(InternalError::ProtocolChanged(_))
        ));
    }

    #[test]
    fn an_overwrite_tail_failure_counts_the_head_post() {
        let new = new_tracks(OVERWRITE_ATOMIC_MAX + 50);
        let api = Fake::default()
            .heads([Ok(head("s", 2)), Ok(head("w", OVERWRITE_ATOMIC_MAX))])
            .posts([Ok(()), Err(InternalError::Transient("timeout".into()))]);
        let mut playlist = local("s", 2);
        let err = overwrite(&api, &mut playlist, &new, &no_known).unwrap_err();
        assert_eq!(
            err.note().as_deref(),
            Some("Added at least 1000 of 1050 tracks")
        );
        assert!(err.stores_copy());
        assert_eq!(uri_list(&playlist), uris(&new[..OVERWRITE_ATOMIC_MAX]));
        assert_eq!(playlist.snapshot_id, "w");
    }

    /// A current copy held in another order than the server's: server order a0 b1 c2, held
    /// as c2 a0 b1.
    fn held_sorted() -> Playlist {
        Playlist {
            tracks: Some(vec![
                at("spotify:track:c", 2),
                at("spotify:track:a", 0),
                at("spotify:track:b", 1),
            ]),
            num_tracks: 3,
            ..local("s", 0)
        }
    }

    fn held(playlist: &Playlist) -> Vec<(String, usize)> {
        playlist
            .tracks
            .iter()
            .flatten()
            .map(|p| (p.uri(), p.list_index()))
            .collect()
    }

    #[test]
    fn a_remove_sends_the_server_index_and_keeps_the_order() {
        // In server order (the copy library stores and edits work on), position is list_index.
        let mut playlist = local("s", 3);
        let api = Fake::default()
            .heads([Ok(head("s", 3)), Ok(head("t", 2))])
            .posts([Ok(())]);
        remove_track(&api, &mut playlist, 1, "spotify:track:1", &no_known).expect("verified");
        let rem = &api.sent.borrow()[0].deltas[0].ops[0].rem;
        assert_eq!((rem.from_index, rem.length), (Some(1), Some(1)));
        assert_eq!(
            held(&playlist),
            [
                ("spotify:track:0".to_string(), 0),
                ("spotify:track:2".to_string(), 1)
            ]
        );
        assert_eq!(
            (playlist.num_tracks, playlist.snapshot_id.as_str()),
            (2, "t")
        );
        assert_eq!(api.contents_reads.get(), 0);
    }

    #[test]
    fn a_copy_out_of_server_order_is_reloaded_before_a_remove() {
        // b is at server position 1; the copy holds it at position 2.
        let mut playlist = held_sorted();
        let server: Vec<Playable> = ["a", "b", "c"]
            .iter()
            .enumerate()
            .map(|(i, id)| at(&format!("spotify:track:{id}"), i))
            .collect();
        let api = Fake::default()
            .heads([Ok(head("s", 3)), Ok(head("t", 2))])
            .contents(Ok((head("s", 3), server)))
            .posts([Ok(())]);
        remove_track(&api, &mut playlist, 1, "spotify:track:b", &no_known).expect("verified");
        assert_eq!(api.contents_reads.get(), 1, "not current: reloaded");
        let rem = &api.sent.borrow()[0].deltas[0].ops[0].rem;
        assert_eq!(rem.from_index, Some(1), "b's server index, not its row");
        assert_eq!(
            held(&playlist),
            [
                ("spotify:track:a".to_string(), 0),
                ("spotify:track:c".to_string(), 1)
            ]
        );
    }

    #[test]
    fn a_stale_copy_whose_item_moved_sends_nothing() {
        let mut playlist = local("s", 3);
        // On the server, position 1 now holds another track.
        let server = vec![
            at("spotify:track:0", 0),
            at("spotify:track:x", 1),
            at("spotify:track:2", 2),
        ];
        let api = Fake::default()
            .heads([Ok(head("u", 3))])
            .contents(Ok((head("u", 3), server)));
        let err = remove_track(&api, &mut playlist, 1, "spotify:track:1", &no_known).unwrap_err();
        assert!(matches!(err, EditError::Moved));
        assert!(api.sent.borrow().is_empty());
        assert_eq!(
            playlist.snapshot_id, "u",
            "the reloaded copy, for the caller to store"
        );
    }

    #[test]
    fn a_later_batch_mismatch_note_is_exact() {
        let err = partial(100, 150, EditError::Mismatch);
        assert_eq!(err.note().as_deref(), Some("Added 100 of 150 tracks"));
    }

    /// A copy cached by an older Web append: x kept list_index 0, so a0 b1 x0. On the server
    /// x is at position 2.
    fn held_duplicate() -> Playlist {
        Playlist {
            tracks: Some(vec![
                at("spotify:track:a", 0),
                at("spotify:track:b", 1),
                at("spotify:track:x", 0),
            ]),
            num_tracks: 3,
            ..local("s", 0)
        }
    }

    #[test]
    fn a_duplicate_list_index_is_told_apart_by_the_uri() {
        let tracks = held_duplicate().tracks.unwrap();
        assert_eq!(server_item(&tracks, 0, "spotify:track:x"), Some(2));
        assert_eq!(server_item(&tracks, 0, "spotify:track:a"), Some(0));
    }

    #[test]
    fn a_row_with_a_duplicate_list_index_never_removes_another_track() {
        // Deleting row x (list_index 0): the copy is not in server order, so it is reloaded;
        // server position 0 is a, not x, so nothing is sent.
        let mut playlist = held_duplicate();
        let server = vec![
            at("spotify:track:a", 0),
            at("spotify:track:b", 1),
            at("spotify:track:x", 2),
        ];
        let api = Fake::default()
            .heads([Ok(head("s", 3))])
            .contents(Ok((head("s", 3), server)));
        let err = remove_track(&api, &mut playlist, 0, "spotify:track:x", &no_known).unwrap_err();
        assert!(matches!(err, EditError::Moved));
        assert!(api.sent.borrow().is_empty(), "a is never removed");
        assert_eq!(
            uri_list(&playlist),
            ["spotify:track:a", "spotify:track:b", "spotify:track:x"]
        );
    }

    #[test]
    fn a_web_remove_drops_the_row_only_where_it_still_is() {
        let mut playlist = local("s", 3);
        remove_after_web(&mut playlist, 1, "spotify:track:1");
        assert_eq!(uri_list(&playlist), ["spotify:track:0", "spotify:track:2"]);
        assert_eq!(
            (playlist.num_tracks, playlist.snapshot_id.as_str()),
            (2, "s")
        );

        // Reloaded since: position 1 holds another track, which stays; the copy goes stale.
        let mut reloaded = local("r", 3);
        remove_after_web(&mut reloaded, 1, "spotify:track:gone");
        assert_eq!(reloaded.num_tracks, 3);
        assert_eq!(reloaded.snapshot_id, "");
    }

    #[test]
    fn a_web_remove_on_a_duplicate_index_copy_clears_its_snapshot() {
        // Cached by an older build: position 1 twice.
        let mut cached = Playlist {
            tracks: Some(vec![
                at("spotify:track:a", 0),
                at("spotify:track:b", 1),
                at("spotify:track:c", 1),
                at("spotify:track:d", 2),
            ]),
            num_tracks: 4,
            ..local("s", 0)
        };
        remove_after_web(&mut cached, 1, "spotify:track:b");
        assert_eq!(
            uri_list(&cached),
            ["spotify:track:a", "spotify:track:c", "spotify:track:d"]
        );
        assert_eq!(cached.num_tracks, 3);
        assert_eq!(cached.snapshot_id, "", "reloaded on the next write or open");
    }

    #[test]
    fn an_open_reloads_a_copy_whose_snapshot_was_cleared() {
        // A Web-created empty playlist: one full read, no head read.
        let api = Fake::default().contents(Ok((head("s1", 0), vec![])));
        let mut created = local("", 0);
        assert!(reload_cleared(&api, &mut created));
        assert_eq!(api.contents_reads.get(), 1);
        assert_eq!(
            (created.snapshot_id.as_str(), created.num_tracks),
            ("s1", 0)
        );

        // A copy left by an unconfirmed write takes the server's items and snapshot.
        let server = vec![at("spotify:track:0", 0), at("spotify:track:new", 1)];
        let api = Fake::default().contents(Ok((head("s2", 2), server)));
        let mut stale = local("", 1);
        assert!(reload_cleared(&api, &mut stale));
        assert_eq!(uri_list(&stale), ["spotify:track:0", "spotify:track:new"]);
        assert_eq!((stale.snapshot_id.as_str(), stale.num_tracks), ("s2", 2));
    }

    #[test]
    fn an_open_keeps_a_current_or_unloaded_copy_and_a_failed_reload() {
        // No queued reads: any read would panic.
        let api = Fake::default();
        let mut current = local("s", 2);
        assert!(!reload_cleared(&api, &mut current));
        let mut unloaded = Playlist {
            tracks: None,
            ..local("", 0)
        };
        assert!(!reload_cleared(&api, &mut unloaded));
        api.unusable.set(true);
        let mut cleared = local("", 2);
        assert!(!reload_cleared(&api, &mut cleared));

        let api = Fake::default().contents(Err(InternalError::ProtocolChanged("read".into())));
        let mut cleared = local("", 2);
        assert!(!reload_cleared(&api, &mut cleared));
        assert_eq!(api.latches.get(), 1, "a protocol change latches the area");
        assert_eq!((cleared.snapshot_id.as_str(), cleared.num_tracks), ("", 2));
        let api = Fake::default().contents(Err(InternalError::Transient("read".into())));
        assert!(!reload_cleared(&api, &mut cleared));
        assert_eq!(api.latches.get(), 0);
        assert_eq!(uri_list(&cleared), ["spotify:track:0", "spotify:track:1"]);
    }

    /// A complete rootlist read of `ids` (one raw item each) at revision `rev`.
    fn rootlist_of(rev: &str, ids: &[&str]) -> Rootlist {
        Rootlist {
            entries: ids.iter().map(|id| root_pl(id)).collect(),
            length: ids.len(),
            read: ids.len(),
            pages: 1,
            revision: rev.as_bytes().to_vec(),
            skipped: vec![],
        }
    }

    fn root_head(rev: &str, length: usize) -> RootHead {
        RootHead {
            revision: Some(rev.as_bytes().to_vec()),
            length: Some(length),
        }
    }

    #[test]
    fn a_rootlist_read_decides_the_delete_step() {
        let once = rootlist_of("r", &["a", "p", "b"]);
        assert_eq!(rootlist_step(&once, "p"), RootlistStep::Remove(1));
        let twice = rootlist_of("r", &["p", "a", "p"]);
        assert_eq!(rootlist_step(&twice, "p"), RootlistStep::Remove(0));
        assert_eq!(rootlist_copies(&twice, "p"), 2);
        let absent = rootlist_of("r", &["a", "b"]);
        assert_eq!(rootlist_step(&absent, "p"), RootlistStep::Absent);
        let incomplete = Rootlist {
            length: 7000,
            ..rootlist_of("r", &["p", "a"])
        };
        assert_eq!(rootlist_step(&incomplete, "p"), RootlistStep::Incomplete);
        let incomplete_absent = Rootlist {
            length: 7000,
            ..absent
        };
        assert_eq!(
            rootlist_step(&incomplete_absent, "p"),
            RootlistStep::Incomplete
        );
    }

    #[test]
    fn a_delete_removes_every_copy_one_read_at_a_time() {
        let api = Fake::default()
            .rootlists([
                Ok(rootlist_of("r1", &["a", "p", "b", "p"])),
                Ok(rootlist_of("r2", &["a", "b", "p"])),
                Ok(rootlist_of("r3", &["a", "b"])),
            ])
            .posts([Ok(()), Ok(())])
            .root_heads([Ok(root_head("r2", 3)), Ok(root_head("r3", 2))]);
        unfollow(&api, "p").expect("every copy removed");
        assert_eq!(api.removed_at(), [Some(1), Some(2)]);
        // Each remove is based on the read that found its index.
        let bases: Vec<Option<Vec<u8>>> = api
            .sent
            .borrow()
            .iter()
            .map(|body| body.base_revision.clone())
            .collect();
        assert_eq!(bases, [Some(b"r1".to_vec()), Some(b"r2".to_vec())]);
    }

    #[test]
    fn a_delete_of_a_playlist_not_in_the_rootlist_sends_nothing() {
        let api = Fake::default().rootlists([Ok(rootlist_of("r", &["a", "b"]))]);
        unfollow(&api, "p").expect("nothing to remove is a success");
        assert!(api.sent.borrow().is_empty());
    }

    #[test]
    fn a_delete_on_an_incomplete_rootlist_fails_before_any_post() {
        use crate::internal_api::fallback::{Action, decide_after};
        let api = Fake::default().rootlists([Ok(Rootlist {
            length: 7000,
            ..rootlist_of("r", &["p", "a"])
        })]);
        let err = unfollow(&api, "p").expect_err("too long");
        assert!(matches!(err, EditError::TooLong));
        assert_eq!(
            err.note().as_deref(),
            Some("Playlist list too long to edit")
        );
        assert_eq!(decide_after(&InternalError::from(err)), Action::Fail);
        assert!(api.sent.borrow().is_empty());
    }

    #[test]
    fn a_failure_after_the_first_remove_is_a_partial_delete() {
        use crate::internal_api::fallback::{Action, decide_after};
        // After one verified remove, the re-read, the later POST, its verify read, or a re-read
        // that stopped at the page cap fails in turn.
        let first = || Ok(rootlist_of("r1", &["p", "p"]));
        let verified = || Ok(root_head("r2", 1));
        let reread = Fake::default()
            .rootlists([first(), Err(InternalError::NoSession)])
            .posts([Ok(())])
            .root_heads([verified()]);
        let later_post = Fake::default()
            .rootlists([first(), Ok(rootlist_of("r2", &["p"]))])
            .posts([Ok(()), Err(InternalError::NotFound)])
            .root_heads([verified()]);
        let later_verify = Fake::default()
            .rootlists([first(), Ok(rootlist_of("r2", &["p"]))])
            .posts([Ok(()), Ok(())])
            .root_heads([verified(), Err(InternalError::NoSession)]);
        let too_long = Fake::default()
            .rootlists([
                first(),
                Ok(Rootlist {
                    length: 7000,
                    ..rootlist_of("r2", &["p"])
                }),
            ])
            .posts([Ok(())])
            .root_heads([verified()]);
        for api in [reread, later_post, later_verify, too_long] {
            let err = unfollow(&api, "p").expect_err("failed after a remove");
            assert!(matches!(err, EditError::PartlyDeleted), "{err:?}");
            assert_eq!(
                err.note().as_deref(),
                Some("Removed some copies; the playlist is still in your library")
            );
            assert!(!err.stores_copy());
            assert_eq!(decide_after(&InternalError::from(err)), Action::Fail);
        }
    }

    #[test]
    fn a_first_remove_failure_keeps_its_class() {
        let api = Fake::default()
            .rootlists([Ok(rootlist_of("r1", &["p", "p"]))])
            .posts([Err(InternalError::NoSession)]);
        assert!(matches!(
            unfollow(&api, "p"),
            Err(EditError::Internal(InternalError::NoSession))
        ));
    }

    #[test]
    fn a_copy_still_listed_after_every_listed_remove_fails() {
        // One copy listed, removed and verified (r2, length 1); before the re-read another
        // client adds it again at the top (r3, length 2).
        let api = Fake::default()
            .rootlists([
                Ok(rootlist_of("r1", &["a", "p"])),
                Ok(rootlist_of("r3", &["p", "a"])),
            ])
            .posts([Ok(())])
            .root_heads([Ok(root_head("r2", 1))]);
        // Past the first remove, the cap's mismatch is a partial delete.
        assert!(matches!(unfollow(&api, "p"), Err(EditError::PartlyDeleted)));
        assert_eq!(api.removed_at(), [Some(1)]);
    }
}
