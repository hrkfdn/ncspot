//! Playlist writes off the UI thread. The UI thread applies a job to the stored copy at once
//! ([apply_local]) and queues it; one worker runs the queue in issue order against the copy
//! Spotify last confirmed, so the store always shows the confirmed copy plus the queued jobs.
#![deny(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::error;
use parking_lot::{Condvar, Mutex};

use crate::internal_api::playlist_write::{move_target, moved_position};
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::playlist_edit::{move_server_item, remove_server_item, server_item};
use crate::save_log::SaveOp;

/// One playlist write.
#[derive(Clone, Debug)]
pub enum JobKind {
    /// Move the item at server position `index` (still `uri`) by `n` rows, clamped.
    Move {
        index: usize,
        uri: String,
        n: usize,
        up: bool,
    },
    Remove {
        index: usize,
        uri: String,
    },
    Add {
        tracks: Vec<Playable>,
    },
    Rename {
        name: String,
    },
    Overwrite {
        tracks: Vec<Playable>,
    },
    Create {
        name: String,
    },
    SaveAs {
        name: String,
        tracks: Vec<Playable>,
    },
    Follow {
        playlist: Box<Playlist>,
    },
    Unfollow,
    Pin {
        pinned: bool,
    },
    /// A library save, with no playlist: the save log holds its optimistic state.
    LibrarySave(SaveOp),
}

impl JobKind {
    /// Shown in the store before Spotify confirms it.
    pub fn optimistic(&self) -> bool {
        matches!(
            self,
            Self::Move { .. } | Self::Remove { .. } | Self::Add { .. } | Self::Rename { .. }
        )
    }

    /// Runs against the playlist's confirmed copy.
    pub fn on_copy(&self) -> bool {
        self.optimistic() || matches!(self, Self::Overwrite { .. })
    }
}

#[derive(Clone, Debug)]
pub struct WriteJob {
    /// Issue order; a folded job keeps the last folded seq.
    pub seq: u64,
    /// `None` for a create or save-as (no id yet).
    pub playlist_id: Option<String>,
    pub kind: JobKind,
}

/// Where the item at `index` ends after moving `n` rows (clamped); `index` when it cannot move.
pub fn final_position(index: usize, n: usize, up: bool, length: usize) -> usize {
    move_target(index, n, up, length).map_or(index, |to| moved_position(index, to))
}

/// Apply an optimistic job to `playlist` (a copy in server order). Clears the snapshot when
/// anything changed, so an optimistic copy never passes for a Spotify version. Returns the
/// moved item's new position for a move; `None` for other jobs or when nothing changed.
pub fn apply_local(playlist: &mut Playlist, kind: &JobKind) -> Option<usize> {
    let changed = match kind {
        JobKind::Move { index, uri, n, up } => {
            let tracks = playlist.tracks.as_mut()?;
            server_item(tracks, *index, uri)?;
            let at = final_position(*index, *n, *up, tracks.len());
            if at != *index {
                move_server_item(tracks, *index, at);
            }
            playlist.snapshot_id.clear();
            return Some(at);
        }
        JobKind::Remove { index, uri } => {
            let removed = playlist
                .tracks
                .as_mut()
                .is_some_and(|tracks| remove_server_item(tracks, *index, uri));
            if removed {
                playlist.num_tracks = playlist.tracks.as_ref().map_or(0, Vec::len);
            }
            removed
        }
        JobKind::Add { tracks: new } => match &mut playlist.tracks {
            Some(tracks) => {
                let start = tracks.len();
                tracks.extend(new.iter().cloned().enumerate().map(|(i, mut playable)| {
                    playable.set_list_index(start + i);
                    playable
                }));
                playlist.num_tracks = tracks.len();
                true
            }
            None => false,
        },
        JobKind::Rename { name } => {
            playlist.name.clone_from(name);
            true
        }
        _ => false,
    };
    if changed {
        playlist.snapshot_id.clear();
    }
    None
}

/// Fold the queued moves that directly follow `job` and move the same row of the same
/// playlist again (a held Shift key) into one move. `length` is the playlist's track count.
pub fn fold_moves(mut job: WriteJob, queue: &mut VecDeque<WriteJob>, length: usize) -> WriteJob {
    let JobKind::Move {
        index: from,
        uri,
        n,
        up,
    } = &job.kind
    else {
        return job;
    };
    let (from, uri) = (*from, uri.clone());
    let mut at = final_position(from, *n, *up, length);
    while let Some(WriteJob {
        playlist_id,
        kind:
            JobKind::Move {
                index,
                uri: next,
                n,
                up,
            },
        ..
    }) = queue.front()
    {
        if *playlist_id != job.playlist_id || *next != uri || *index != at {
            break;
        }
        at = final_position(at, *n, *up, length);
        job.seq = queue.pop_front().map_or(job.seq, |next| next.seq);
    }
    job.kind = JobKind::Move {
        index: from,
        uri,
        n: at.abs_diff(from),
        up: at < from,
    };
    job
}

#[derive(Default)]
struct State {
    queue: VecDeque<WriteJob>,
    /// The id of the job being run, if it has one (`Some(None)`: a job with no id).
    in_flight: Option<Option<String>>,
    /// The copy Spotify last confirmed, for each id with a queued or running job.
    confirmed: HashMap<String, Playlist>,
    next_seq: u64,
    stopping: bool,
    /// A worker loop runs (between [PlaylistWriter::mark_running] and [PlaylistWriter::mark_exited]).
    worker_running: bool,
}

impl State {
    fn has_jobs(&self, id: &str) -> bool {
        self.in_flight
            .as_ref()
            .is_some_and(|f| f.as_deref() == Some(id))
            || self
                .queue
                .iter()
                .any(|j| j.playlist_id.as_deref() == Some(id))
    }
}

/// The outcome of one [PlaylistWriter::step].
#[derive(Debug)]
pub struct Settled {
    pub seq: u64,
    pub playlist_id: Option<String>,
    pub ok: bool,
    /// Later queued jobs of the same playlist dropped after a failure.
    pub dropped: usize,
    /// No job of this playlist is left queued.
    pub drained: bool,
    /// The confirmed copy after the job (for jobs that run on a copy).
    pub confirmed: Option<Playlist>,
    /// The save this job was, for the library to settle.
    pub save: Option<SaveOp>,
}

/// The playlist write queue, shared by the UI thread (queueing) and the worker (running).
#[derive(Clone, Default)]
pub struct PlaylistWriter {
    shared: Arc<(Mutex<State>, Condvar)>,
}

impl PlaylistWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a job and return its seq. For a job on a copy, `confirmed` supplies the stored
    /// copy (taken before the job is applied locally) when the id has no confirmed copy yet.
    pub fn queue(
        &self,
        playlist_id: Option<&str>,
        kind: JobKind,
        confirmed: impl FnOnce() -> Option<Playlist>,
    ) -> u64 {
        let (lock, cvar) = &*self.shared;
        let mut state = lock.lock();
        state.next_seq += 1;
        let seq = state.next_seq;
        if let Some(id) = playlist_id
            && kind.on_copy()
            && !state.confirmed.contains_key(id)
            && let Some(copy) = confirmed()
        {
            state.confirmed.insert(id.to_string(), copy);
        }
        state.queue.push_back(WriteJob {
            seq,
            playlist_id: playlist_id.map(str::to_string),
            kind,
        });
        cvar.notify_all();
        seq
    }

    /// `updated` with the queued optimistic jobs of its id applied.
    pub fn overlay(&self, updated: &Playlist) -> Playlist {
        let state = self.shared.0.lock();
        let mut shown = updated.clone();
        for job in state
            .queue
            .iter()
            .filter(|j| j.playlist_id.as_deref() == Some(&updated.id))
        {
            if job.kind.optimistic() {
                apply_local(&mut shown, &job.kind);
            }
        }
        shown
    }

    pub fn is_pending(&self, id: &str) -> bool {
        self.shared.0.lock().has_jobs(id)
    }

    pub fn pending(&self) -> usize {
        let state = self.shared.0.lock();
        state.queue.len() + usize::from(state.in_flight.is_some())
    }

    /// Forget the confirmed copy of `id` once no job of it is left. Called after the store
    /// holds the confirmed copy, so a new job never adopts an optimistic store copy as base.
    pub fn release(&self, id: &str) {
        let mut state = self.shared.0.lock();
        if !state.has_jobs(id) {
            state.confirmed.remove(id);
        }
    }

    pub fn confirmed(&self, id: &str) -> Option<Playlist> {
        self.shared.0.lock().confirmed.get(id).cloned()
    }

    /// Run the next job (folded with following held-key moves) through `run`, which gets the
    /// confirmed copy for a job on a copy and returns whether the write landed. A panic in
    /// `run` settles the job as failed and keeps the confirmed copy it had. `None` when
    /// nothing is queued or the writer is stopping.
    pub fn step(
        &self,
        run: impl FnOnce(&WriteJob, Option<&mut Playlist>) -> bool,
    ) -> Option<Settled> {
        let (job, mut copy) = {
            let mut state = self.shared.0.lock();
            if state.stopping {
                return None;
            }
            let job = state.queue.pop_front()?;
            let copy = job
                .playlist_id
                .as_ref()
                .filter(|_| job.kind.on_copy())
                .and_then(|id| state.confirmed.get(id).cloned());
            let length = copy
                .as_ref()
                .and_then(|c| c.tracks.as_ref())
                .map_or(0, Vec::len);
            let job = fold_moves(job, &mut state.queue, length);
            state.in_flight = Some(job.playlist_id.clone());
            (job, copy)
        };
        let ok = match catch_unwind(AssertUnwindSafe(|| run(&job, copy.as_mut()))) {
            Ok(ok) => ok,
            Err(_) => {
                error!("a playlist write panicked");
                // Half a write: not a copy Spotify confirmed.
                copy = None;
                false
            }
        };
        let (lock, cvar) = &*self.shared;
        let mut state = lock.lock();
        state.in_flight = None;
        let id = job.playlist_id.clone();
        let mut dropped = 0;
        if let Some(id) = &id {
            if let Some(copy) = copy.take() {
                state.confirmed.insert(id.clone(), copy);
            }
            if !ok {
                let before = state.queue.len();
                state.queue.retain(|j| j.playlist_id.as_deref() != Some(id));
                dropped = before - state.queue.len();
            }
        }
        let drained = id.as_deref().is_none_or(|id| !state.has_jobs(id));
        let confirmed = match &id {
            Some(id) => state.confirmed.get(id).cloned(),
            None => None,
        };
        cvar.notify_all();
        let save = match &job.kind {
            JobKind::LibrarySave(op) => Some(op.clone()),
            _ => None,
        };
        Some(Settled {
            seq: job.seq,
            playlist_id: id,
            ok,
            dropped,
            drained,
            confirmed,
            save,
        })
    }

    /// Block until a job is queued (true) or the writer stops (false).
    pub fn wait_job(&self) -> bool {
        let (lock, cvar) = &*self.shared;
        let mut state = lock.lock();
        while state.queue.is_empty() && !state.stopping {
            cvar.wait(&mut state);
        }
        !state.stopping
    }

    /// Wait up to `timeout`; false as soon as the writer stops.
    pub fn wait_unless_stopped(&self, timeout: Duration) -> bool {
        let (lock, cvar) = &*self.shared;
        let deadline = Instant::now() + timeout;
        let mut state = lock.lock();
        while !state.stopping {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return true;
            }
            cvar.wait_for(&mut state, left);
        }
        false
    }

    /// Stop taking jobs: drop the queued ones. Returns the ids with a confirmed copy, whose
    /// stored copies must be reset to it (the job in flight, if any, still finishes).
    pub fn stop(&self) -> Vec<String> {
        let (lock, cvar) = &*self.shared;
        let mut state = lock.lock();
        state.stopping = true;
        state.queue.clear();
        cvar.notify_all();
        let mut ids: Vec<String> = state.confirmed.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// The worker loop starts (before it is spawned).
    pub fn mark_running(&self) {
        self.shared.0.lock().worker_running = true;
    }

    /// A guard that marks the worker loop ended when dropped, also when its thread unwinds.
    pub fn exit_guard(&self) -> ExitGuard {
        ExitGuard(self.clone())
    }

    /// The worker loop ended.
    pub fn mark_exited(&self) {
        let (lock, cvar) = &*self.shared;
        lock.lock().worker_running = false;
        cvar.notify_all();
    }

    /// Wait until no worker loop runs (at once when none was started), at most `timeout`; true
    /// when it is gone. After [PlaylistWriter::stop] the worker exits once its job is done.
    pub fn wait_exited(&self, timeout: Duration) -> bool {
        let (lock, cvar) = &*self.shared;
        let deadline = Instant::now() + timeout;
        let mut state = lock.lock();
        while state.worker_running {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            cvar.wait_for(&mut state, left);
        }
        true
    }

    /// Wait until no job runs, at most `timeout`; true when idle.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let (lock, cvar) = &*self.shared;
        let deadline = Instant::now() + timeout;
        let mut state = lock.lock();
        while state.in_flight.is_some() || (!state.stopping && !state.queue.is_empty()) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            cvar.wait_for(&mut state, left);
        }
        true
    }
}

/// Calls [PlaylistWriter::mark_exited] on drop (see [PlaylistWriter::exit_guard]).
pub struct ExitGuard(PlaylistWriter);

impl Drop for ExitGuard {
    fn drop(&mut self) {
        self.0.mark_exited();
    }
}

/// How long quit waits for the playlist write in flight.
pub const QUIT_GRACE: Duration = Duration::from_secs(3);

pub const SAVED_NOTE: &str = "Playlist changes saved";

/// The note for a settled write, if any. Success: only once the playlist's queue drained and
/// it is not the open view, and never for a `silent` one. Failure: its note was posted already; repeat it with the dropped
/// count when later writes were dropped.
pub fn settled_note(
    ok: bool,
    drained: bool,
    dropped: usize,
    note: Option<String>,
    open: bool,
    silent: bool,
) -> Option<String> {
    if ok {
        return (drained && !open && !silent).then(|| SAVED_NOTE.to_string());
    }
    if dropped == 0 {
        return None;
    }
    let plural = if dropped == 1 { "change" } else { "changes" };
    let base = note.unwrap_or_else(|| "Playlist change failed".to_string());
    Some(format!("{base} ({dropped} later {plural} dropped)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_exited_follows_the_worker() {
        let writer = PlaylistWriter::new();
        assert!(writer.wait_exited(Duration::ZERO), "no worker started");
        writer.mark_running();
        assert!(
            !writer.wait_exited(Duration::from_millis(10)),
            "still running"
        );
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let w = writer.clone();
        let worker = std::thread::spawn(move || {
            rx.recv().unwrap();
            w.mark_exited();
        });
        assert!(
            !writer.wait_exited(Duration::from_millis(10)),
            "blocked on a job"
        );
        tx.send(()).unwrap();
        assert!(writer.wait_exited(Duration::from_secs(30)));
        worker.join().unwrap();
    }

    #[test]
    fn a_wait_ends_with_a_stop_and_otherwise_runs_out() {
        let w = PlaylistWriter::new();
        assert!(w.wait_unless_stopped(Duration::from_millis(1)));
        let waiter = {
            let w = w.clone();
            std::thread::spawn(move || w.wait_unless_stopped(Duration::from_secs(30)))
        };
        w.stop();
        assert!(!waiter.join().unwrap());
    }

    #[test]
    fn settled_notes() {
        assert_eq!(
            settled_note(true, true, 0, None, false, false).as_deref(),
            Some(SAVED_NOTE)
        );
        assert_eq!(
            settled_note(true, true, 0, None, true, false),
            None,
            "open view: silent"
        );
        assert_eq!(
            settled_note(true, false, 0, None, false, false),
            None,
            "more queued: wait"
        );
        assert_eq!(
            settled_note(false, true, 0, Some("x".into()), true, false),
            None,
            "already posted"
        );
        assert_eq!(
            settled_note(true, true, 0, None, false, true),
            None,
            "a save is silent"
        );
        assert_eq!(
            settled_note(false, true, 2, Some("x".into()), true, false).as_deref(),
            Some("x (2 later changes dropped)")
        );
        assert_eq!(
            settled_note(false, true, 1, None, false, false).as_deref(),
            Some("Playlist change failed (1 later change dropped)")
        );
    }
    use crate::playlist_items::placeholder;

    fn playlist_of(ids: &[&str]) -> Playlist {
        let tracks: Vec<Playable> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let mut playable = placeholder(id);
                playable.set_list_index(i);
                playable
            })
            .collect();
        Playlist {
            id: "p".into(),
            name: String::new(),
            owner_id: String::new(),
            owner_name: None,
            snapshot_id: "snap".into(),
            num_tracks: tracks.len(),
            tracks: Some(tracks),
            collaborative: false,
            cover_url: None,
        }
    }

    fn uris(p: &Playlist) -> Vec<String> {
        p.tracks
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|t| t.uri())
            .collect()
    }

    fn mv(seq: u64, index: usize, uri: &str, n: usize, up: bool) -> WriteJob {
        WriteJob {
            seq,
            playlist_id: Some("p".into()),
            kind: JobKind::Move {
                index,
                uri: uri.into(),
                n,
                up,
            },
        }
    }

    #[test]
    fn local_move_reindexes_and_clears_the_snapshot() {
        let mut p = playlist_of(&["a", "b", "c"]);
        let at = apply_local(
            &mut p,
            &JobKind::Move {
                index: 0,
                uri: "a".into(),
                n: 1,
                up: false,
            },
        );
        assert_eq!(at, Some(1));
        assert_eq!(uris(&p), ["b", "a", "c"]);
        let indexes: Vec<usize> = p
            .tracks
            .as_deref()
            .unwrap()
            .iter()
            .map(|t| t.list_index())
            .collect();
        assert_eq!(indexes, [0, 1, 2]);
        assert!(p.snapshot_id.is_empty());
    }

    #[test]
    fn local_move_clamps_and_a_wrong_uri_changes_nothing() {
        let mut p = playlist_of(&["a", "b", "c"]);
        let far = JobKind::Move {
            index: 0,
            uri: "a".into(),
            n: 10,
            up: false,
        };
        assert_eq!(apply_local(&mut p, &far), Some(2));
        assert_eq!(uris(&p), ["b", "c", "a"]);
        let before = uris(&p);
        let wrong = JobKind::Move {
            index: 0,
            uri: "zz".into(),
            n: 1,
            up: false,
        };
        assert_eq!(apply_local(&mut p, &wrong), None);
        assert_eq!(uris(&p), before);
    }

    #[test]
    fn local_remove_add_and_rename() {
        let mut p = playlist_of(&["a", "b", "c"]);
        apply_local(
            &mut p,
            &JobKind::Remove {
                index: 1,
                uri: "b".into(),
            },
        );
        assert_eq!(uris(&p), ["a", "c"]);
        assert_eq!(p.num_tracks, 2);
        let extra = playlist_of(&["d"]).tracks.unwrap();
        apply_local(&mut p, &JobKind::Add { tracks: extra });
        assert_eq!(uris(&p), ["a", "c", "d"]);
        assert_eq!(p.tracks.as_deref().unwrap()[2].list_index(), 2);
        apply_local(&mut p, &JobKind::Rename { name: "new".into() });
        assert_eq!(p.name, "new");
    }

    #[test]
    fn remove_on_an_unloaded_playlist_keeps_the_count_and_snapshot() {
        let mut p = playlist_of(&["a", "b"]);
        p.tracks = None;
        let remove = JobKind::Remove {
            index: 0,
            uri: "a".into(),
        };
        assert_eq!(apply_local(&mut p, &remove), None);
        assert_eq!(p.num_tracks, 2);
        assert_eq!(p.snapshot_id, "snap");
    }

    #[test]
    fn remove_with_a_wrong_uri_changes_nothing() {
        let mut p = playlist_of(&["a", "b", "c"]);
        p.num_tracks = 7;
        let remove = JobKind::Remove {
            index: 1,
            uri: "zz".into(),
        };
        assert_eq!(apply_local(&mut p, &remove), None);
        assert_eq!(uris(&p), ["a", "b", "c"]);
        assert_eq!(p.num_tracks, 7);
        assert_eq!(p.snapshot_id, "snap");
    }

    #[test]
    fn rootlist_jobs_are_not_applied_locally() {
        let mut p = playlist_of(&["a"]);
        p.snapshot_id = "s".into();
        assert_eq!(apply_local(&mut p, &JobKind::Unfollow), None);
        assert_eq!(p.snapshot_id, "s");
        assert!(!JobKind::Unfollow.optimistic());
        assert!(JobKind::Overwrite { tracks: vec![] }.on_copy());
        assert!(!JobKind::Overwrite { tracks: vec![] }.optimistic());
    }

    #[test]
    fn held_key_moves_of_one_row_fold_into_one() {
        // a b c d: a down, then a (now at 1) down, then a (now at 2) down.
        let mut queue: VecDeque<WriteJob> =
            [mv(2, 1, "a", 1, false), mv(3, 2, "a", 1, false)].into();
        let folded = fold_moves(mv(1, 0, "a", 1, false), &mut queue, 4);
        assert!(queue.is_empty());
        match folded.kind {
            JobKind::Move { index, n, up, .. } => assert_eq!((index, n, up), (0, 3, false)),
            _ => panic!("not a move"),
        }
        assert_eq!(folded.seq, 3, "the folded job settles as the last one");
    }

    #[test]
    fn moves_of_another_row_or_playlist_do_not_fold() {
        let mut other = mv(3, 1, "a", 1, false);
        other.playlist_id = Some("q".into());
        let mut queue: VecDeque<WriteJob> = [mv(2, 2, "c", 1, true), other].into();
        let folded = fold_moves(mv(1, 0, "a", 1, false), &mut queue, 4);
        assert_eq!(queue.len(), 2);
        assert!(matches!(
            folded.kind,
            JobKind::Move {
                index: 0,
                n: 1,
                up: false,
                ..
            }
        ));
    }

    #[test]
    fn a_down_up_pair_folds_to_no_move() {
        let mut queue: VecDeque<WriteJob> = [mv(2, 1, "a", 1, true)].into();
        let folded = fold_moves(mv(1, 0, "a", 1, false), &mut queue, 3);
        assert!(matches!(folded.kind, JobKind::Move { n: 0, .. }));
    }

    fn writer_with(p: &Playlist) -> PlaylistWriter {
        let w = PlaylistWriter::new();
        let _ = p; // the first queue call supplies it
        w
    }

    fn queue_move(w: &PlaylistWriter, p: &Playlist, index: usize, uri: &str) -> u64 {
        w.queue(
            Some("p"),
            JobKind::Move {
                index,
                uri: uri.into(),
                n: 1,
                up: false,
            },
            || Some(p.clone()),
        )
    }

    #[test]
    fn jobs_run_in_issue_order_on_the_confirmed_copy() {
        let mut p = playlist_of(&["a", "b", "c"]);
        p.id = "p".into();
        p.snapshot_id = "s0".into();
        let w = writer_with(&p);
        queue_move(&w, &p, 0, "a");
        w.queue(Some("p"), JobKind::Rename { name: "n".into() }, || {
            unreachable!("already confirmed")
        });
        assert_eq!(w.pending(), 2);
        let mut seen = Vec::new();
        let s1 = w
            .step(|job, copy| {
                let copy = copy.unwrap();
                assert_eq!(copy.snapshot_id, "s0", "runs on the confirmed copy");
                apply_local(copy, &job.kind);
                copy.snapshot_id = "s1".into();
                seen.push(job.seq);
                true
            })
            .unwrap();
        assert!(s1.ok && !s1.drained);
        assert_eq!(s1.playlist_id.as_deref(), Some("p"));
        let s2 = w
            .step(|job, copy| {
                assert_eq!(copy.unwrap().snapshot_id, "s1");
                seen.push(job.seq);
                true
            })
            .unwrap();
        assert!(s2.drained);
        assert_eq!(s2.confirmed.unwrap().snapshot_id, "s1");
        assert_eq!(seen, [1, 2]);
        assert_eq!(w.pending(), 0);
        assert!(!w.is_pending("p"));
    }

    #[test]
    fn overlay_shows_confirmed_plus_queued_jobs() {
        let mut p = playlist_of(&["a", "b", "c"]);
        p.id = "p".into();
        let w = writer_with(&p);
        queue_move(&w, &p, 0, "a");
        let shown = w.overlay(&p);
        assert_eq!(uris(&shown), ["b", "a", "c"]);
        assert!(shown.snapshot_id.is_empty());
    }

    #[test]
    fn a_failure_drops_later_jobs_of_that_playlist_only() {
        let mut p = playlist_of(&["a", "b", "c"]);
        p.id = "p".into();
        let mut q = playlist_of(&["x"]);
        q.id = "q".into();
        let w = writer_with(&p);
        queue_move(&w, &p, 0, "a");
        w.queue(Some("q"), JobKind::Rename { name: "r".into() }, || {
            Some(q.clone())
        });
        w.queue(
            Some("p"),
            JobKind::Remove {
                index: 2,
                uri: "c".into(),
            },
            || unreachable!(),
        );
        let s = w.step(|_, _| false).unwrap();
        assert!(!s.ok);
        assert_eq!(s.dropped, 1);
        assert!(s.drained);
        assert_eq!(w.pending(), 1, "the other playlist's job stays");
    }

    #[test]
    fn a_panicking_job_settles_as_failed() {
        let mut p = playlist_of(&["a", "b", "c"]);
        p.id = "p".into();
        let w = PlaylistWriter::new();
        queue_move(&w, &p, 0, "a");
        w.queue(
            Some("p"),
            JobKind::Rename { name: "n".into() },
            || unreachable!(),
        );
        let s = w
            .step(|_, copy| {
                copy.unwrap().snapshot_id = "half".into();
                panic!("a write failed hard");
            })
            .unwrap();
        assert!(!s.ok);
        assert_eq!((s.dropped, s.drained), (1, true));
        assert_eq!(w.pending(), 0);
        assert_eq!(
            s.confirmed.unwrap().snapshot_id,
            "snap",
            "the copy before the job"
        );
    }

    #[test]
    fn the_exit_guard_marks_the_worker_exited_on_a_panic() {
        let w = PlaylistWriter::new();
        w.mark_running();
        let guarded = w.clone();
        let worker = std::thread::spawn(move || {
            let _exited = guarded.exit_guard();
            panic!("the worker loop failed");
        });
        assert!(worker.join().is_err());
        assert!(w.wait_exited(Duration::ZERO));
    }

    #[test]
    fn step_folds_held_key_moves() {
        let mut p = playlist_of(&["a", "b", "c", "d"]);
        p.id = "p".into();
        let w = writer_with(&p);
        queue_move(&w, &p, 0, "a");
        queue_move(&w, &p, 1, "a");
        let mut runs = 0;
        let s = w
            .step(|job, _| {
                runs += 1;
                assert!(matches!(
                    job.kind,
                    JobKind::Move {
                        index: 0,
                        n: 2,
                        up: false,
                        ..
                    }
                ));
                true
            })
            .unwrap();
        assert_eq!((runs, s.seq, s.drained), (1, 2, true));
    }

    #[test]
    fn stop_drops_queued_jobs_and_names_copies_to_reset() {
        let mut p = playlist_of(&["a", "b"]);
        p.id = "p".into();
        let w = writer_with(&p);
        queue_move(&w, &p, 0, "a");
        assert_eq!(w.stop(), vec!["p".to_string()]);
        assert_eq!(w.pending(), 0);
        assert!(w.step(|_, _| true).is_none());
        assert!(!w.wait_job(), "a stopped writer wakes its worker to exit");
        assert_eq!(uris(&w.confirmed("p").unwrap()), ["a", "b"]);
        assert!(w.wait_idle(Duration::from_millis(10)));
    }

    #[test]
    fn the_confirmed_copy_outlives_the_drain_until_released() {
        let mut p = playlist_of(&["a", "b", "c"]);
        p.id = "p".into();
        p.snapshot_id = "s0".into();
        let w = PlaylistWriter::new();
        queue_move(&w, &p, 0, "a");
        let s = w
            .step(|_, copy| {
                copy.unwrap().snapshot_id = "s1".into();
                true
            })
            .unwrap();
        assert!(s.drained);
        assert_eq!(s.confirmed.unwrap().snapshot_id, "s1");
        assert_eq!(w.confirmed("p").unwrap().snapshot_id, "s1");
        // The store still shows the optimistic copy: a new job must not adopt it.
        let mut cleared = p.clone();
        cleared.snapshot_id.clear();
        w.queue(Some("p"), JobKind::Rename { name: "n".into() }, || {
            Some(cleared)
        });
        w.release("p");
        assert!(w.confirmed("p").is_some(), "a queued job keeps the entry");
        w.step(|_, copy| {
            assert_eq!(copy.unwrap().snapshot_id, "s1");
            true
        })
        .unwrap();
        w.release("p");
        assert!(w.confirmed("p").is_none());
    }

    #[test]
    fn stop_wakes_a_running_step_to_idle() {
        let mut p = playlist_of(&["a", "b"]);
        p.id = "p".into();
        let w = PlaylistWriter::new();
        queue_move(&w, &p, 0, "a");
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let (started_tx, started) = std::sync::mpsc::channel::<()>();
        let worker = {
            let w = w.clone();
            std::thread::spawn(move || {
                w.step(|_, _| {
                    started_tx.send(()).unwrap();
                    rx.recv().unwrap();
                    true
                })
            })
        };
        started.recv().unwrap();
        assert!(!w.wait_idle(Duration::from_millis(10)));
        w.stop();
        tx.send(()).unwrap();
        assert!(w.wait_idle(Duration::from_secs(1)));
        assert!(worker.join().unwrap().unwrap().drained);
    }

    #[test]
    fn wait_job_wakes_when_another_thread_queues() {
        let w = PlaylistWriter::new();
        let waiter = {
            let w = w.clone();
            std::thread::spawn(move || w.wait_job())
        };
        w.queue(None, JobKind::Create { name: "n".into() }, || None);
        assert!(waiter.join().unwrap());
    }
}
