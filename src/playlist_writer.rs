//! Playlist writes off the UI thread. The UI thread applies a job to the stored copy at once
//! ([apply_local]) and queues it; one worker runs the queue in issue order against the copy
//! Spotify last confirmed, so the store always shows the confirmed copy plus the queued jobs.

use std::collections::VecDeque;

use crate::internal_api::playlist_write::{move_target, moved_position};
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::playlist_edit::{move_server_item, remove_server_item, server_item};

/// One playlist write.
// Create, SaveAs, Follow and Pin are built only by later callers; tests do not.
#[cfg_attr(test, allow(dead_code))]
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
/// anything changed, so a cached optimistic copy is reloaded by the next sync. Returns the
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
            playlist.num_tracks = playlist.tracks.as_ref().map_or(0, Vec::len);
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
