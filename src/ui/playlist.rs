use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use cursive::Cursive;
use cursive::view::ViewWrapper;

use crate::command::{Command, ShiftMode, SortDirection, SortKey};
use crate::commands::CommandResult;
use crate::config::SortingOrder;
use crate::library::{Library, is_owned};
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::queue::Queue;
use crate::spotify::Spotify;

use crate::traits::ViewExt;
use crate::ui::listview::ListView;

/// A Shift handled this soon after the previous move finished is a key repeat that queued up
/// during the move (the network calls run on the UI thread, and the queued repeats are handled
/// in one burst right after it): it is dropped, so a held key moves one row per finished move
/// and stops within one move of its release.
const SHIFT_BACKLOG: Duration = Duration::from_millis(150);
const SORTED_NOTE: &str = "Sort is on: use 'sort custom' to reorder";
const NOT_OWNED_NOTE: &str = "Only your own or collaborative playlists can be reordered";

/// Whether a Shift handled at `now` is a queued repeat of a move that finished at
/// `previous_end`.
fn backlog(previous_end: Option<Instant>, now: Instant) -> bool {
    previous_end.is_some_and(|end| now.saturating_duration_since(end) < SHIFT_BACKLOG)
}

/// Up (true) or down, and the row count, of `shift <mode> <amount>`: a negative amount flips
/// the direction, as in the queue.
pub fn shift_direction(mode: &ShiftMode, amount: i32) -> (bool, usize) {
    let up = matches!(mode, ShiftMode::Up) != amount.is_negative();
    (up, amount.unsigned_abs() as usize)
}

/// The user's own playlists and collaborative ones can be reordered (an unknown user owns
/// nothing).
fn may_reorder(playlist: &Playlist, user_id: Option<&str>) -> bool {
    playlist.collaborative || is_owned(&playlist.owner_id, user_id)
}

/// Save `key` and `direction` as the sort order of playlist `id`; `custom` removes the saved
/// order, so the rows are in server order again.
fn apply_sort(
    orders: &mut HashMap<String, SortingOrder>,
    id: &str,
    key: &SortKey,
    direction: &SortDirection,
) {
    match key {
        SortKey::Custom => {
            orders.remove(id);
        }
        _ => {
            orders.insert(
                id.to_string(),
                SortingOrder {
                    key: key.clone(),
                    direction: direction.clone(),
                },
            );
        }
    }
}

/// A playlist's tracks. `playlist` stays in server order (it is what edits work on and what
/// the library stores); only the rows of `list` follow the saved sort order.
pub struct PlaylistView {
    playlist: Playlist,
    rows: Arc<RwLock<Vec<Playable>>>,
    list: ListView<Playable>,
    spotify: Spotify,
    library: Arc<Library>,
    queue: Arc<Queue>,
    /// When the last move in this view finished (any outcome), for [backlog].
    last_move_end: Option<Instant>,
}

impl PlaylistView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, playlist: &Playlist) -> Self {
        let mut playlist = playlist.clone();
        let cleared = playlist.tracks.is_some() && playlist.snapshot_id.is_empty();
        playlist.load_tracks(&queue.get_spotify());
        if cleared && !playlist.snapshot_id.is_empty() {
            // Reloaded: store the current copy so the library matches the view, unless a
            // sync stored a newer one meanwhile.
            library.playlist_update_if_unchanged(&playlist, "");
        }

        let rows = Arc::new(RwLock::new(sorted_rows(&library, &playlist)));
        let spotify = queue.get_spotify();
        let list = ListView::new(rows.clone(), queue.clone(), library.clone());

        Self {
            playlist,
            rows,
            list,
            spotify,
            library,
            queue,
            last_move_end: None,
        }
    }

    /// Rebuild the list from the playlist's tracks in the saved sort order.
    fn show_tracks(&mut self) {
        self.rows = Arc::new(RwLock::new(sorted_rows(&self.library, &self.playlist)));
        self.list = ListView::new(self.rows.clone(), self.queue.clone(), self.library.clone());
    }

    /// The playlist's name in the store: a rename (here or in the Playlists tab) or a sync
    /// updates the store, not this copy.
    fn stored_name(&self) -> Option<String> {
        self.library
            .playlists
            .read()
            .unwrap()
            .iter()
            .find(|p| p.id == self.playlist.id)
            .map(|p| p.name.clone())
    }

    /// Take the stored name before a write stores this copy, so the write never puts an old
    /// name back.
    fn refresh_name(&mut self) {
        if let Some(name) = self.stored_name() {
            self.playlist.name = name;
        }
    }

    /// Move the selected track `amount` rows (`shift up|down [amount]`); the note to show, if
    /// any. Refused before any request in a sorted view or a playlist the user may not
    /// reorder; a queued key repeat is dropped.
    fn shift(&mut self, mode: &ShiftMode, amount: i32) -> Option<String> {
        if backlog(self.last_move_end, Instant::now()) {
            return None;
        }
        if self
            .library
            .cfg
            .state()
            .playlist_orders
            .contains_key(&self.playlist.id)
        {
            return Some(SORTED_NOTE.to_string());
        }
        if !may_reorder(&self.playlist, self.library.user_id.as_deref()) {
            return Some(NOT_OWNED_NOTE.to_string());
        }
        let (up, n) = shift_direction(mode, amount);
        let pos = self.list.get_selected_index();
        let row = self.rows.read().unwrap().get(pos).cloned()?;
        self.refresh_name();
        let snapshot_id = self.playlist.snapshot_id.clone();
        let moved = self
            .playlist
            .move_track(&row, n, up, &self.spotify, &self.library);
        self.last_move_end = Some(Instant::now());
        match moved {
            Some(at) => {
                // Unsorted: a row's place is its server position.
                self.show_tracks();
                self.list.move_focus_to(at);
            }
            // A failed move may have reloaded the copy (or cleared its snapshot).
            None if self.playlist.snapshot_id != snapshot_id => self.show_tracks(),
            None => {}
        }
        None
    }
}

/// The tracks of `playlist` in its saved sort order (server order when none is saved).
fn sorted_rows(library: &Library, playlist: &Playlist) -> Vec<Playable> {
    let order = library
        .cfg
        .state()
        .playlist_orders
        .get(&playlist.id)
        .cloned();
    let mut sorted = playlist.clone();
    if let Some(order) = order {
        sorted.sort(&order.key, &order.direction);
    }
    sorted.tracks.unwrap_or_default()
}

impl ViewWrapper for PlaylistView {
    wrap_impl!(self.list: ListView<Playable>);
}

impl ViewExt for PlaylistView {
    #[cfg(feature = "harness")]
    fn harness_state(&self) -> Option<crate::harness::HarnessState> {
        let mut state = self.list.harness_state()?;
        state.view = "playlist".into();
        Some(state)
    }

    fn title(&self) -> String {
        self.stored_name()
            .unwrap_or_else(|| self.playlist.name.clone())
    }

    fn title_sub(&self) -> String {
        if let Some(tracks) = self.playlist.tracks.as_ref() {
            let duration_secs = tracks.iter().map(|p| p.duration() as u64 / 1000).sum();
            let duration = std::time::Duration::from_secs(duration_secs);
            format!(
                "{} tracks, {}",
                tracks.len(),
                crate::utils::format_duration(&duration)
            )
        } else {
            "".to_string()
        }
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        if let Command::Delete = cmd {
            let pos = self.list.get_selected_index();
            // The row itself: its server position and uri (the rows may be sorted, `playlist`
            // is not).
            let Some(row) = self.rows.read().unwrap().get(pos).cloned() else {
                return Ok(CommandResult::Consumed(None));
            };
            self.refresh_name();
            let snapshot_id = self.playlist.snapshot_id.clone();

            return match self
                .playlist
                .delete_track(&row, self.spotify.clone(), &self.library)
            {
                Ok(()) => {
                    // Rebuilt from the copy: a reload before the delete may have changed more
                    // than the removed row, and the later rows moved one server position up.
                    self.show_tracks();
                    self.list.move_focus_to(pos);
                    Ok(CommandResult::Consumed(None))
                }
                Err(note) => {
                    // A failed delete may have reloaded the copy (or cleared its snapshot).
                    if self.playlist.snapshot_id != snapshot_id {
                        self.show_tracks();
                    }
                    Err(note)
                }
            };
        }

        if let Command::Shift(mode, amount) = cmd {
            return Ok(CommandResult::Consumed(
                self.shift(mode, amount.unwrap_or(1)),
            ));
        }

        if let Command::Rename(name) = cmd {
            let note = self.library.rename_playlist(&self.playlist.id, name);
            self.refresh_name();
            return Ok(CommandResult::Consumed(note));
        }

        if let Command::Pin | Command::Unpin = cmd {
            let note = self
                .library
                .pin_command(&self.playlist.id, matches!(cmd, Command::Pin));
            return Ok(CommandResult::Consumed(note));
        }

        if let Command::Sort(key, direction) = cmd {
            self.library.cfg.with_state_mut(|state| {
                apply_sort(
                    &mut state.playlist_orders,
                    &self.playlist.id,
                    key,
                    direction,
                );
            });

            self.show_tracks();
            return Ok(CommandResult::Consumed(None));
        }

        self.list.on_command(s, cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shift_soon_after_a_move_is_a_queued_repeat() {
        let end = Instant::now();
        assert!(!backlog(None, end), "no move yet");
        assert!(backlog(Some(end), end + Duration::from_millis(100)));
        assert!(
            !backlog(Some(end), end + SHIFT_BACKLOG),
            "at the limit: a new press"
        );
        assert!(!backlog(Some(end), end + Duration::from_millis(400)));
    }

    #[test]
    fn a_negative_amount_flips_the_direction() {
        assert_eq!(shift_direction(&ShiftMode::Up, 1), (true, 1));
        assert_eq!(shift_direction(&ShiftMode::Down, 3), (false, 3));
        assert_eq!(shift_direction(&ShiftMode::Down, -2), (true, 2));
        assert_eq!(shift_direction(&ShiftMode::Up, -1), (false, 1));
        assert_eq!(shift_direction(&ShiftMode::Up, 0), (true, 0));
    }

    fn owned_by(owner: &str, collaborative: bool) -> Playlist {
        Playlist {
            id: "p".into(),
            name: String::new(),
            owner_id: owner.into(),
            owner_name: None,
            snapshot_id: String::new(),
            num_tracks: 0,
            tracks: None,
            collaborative,
            cover_url: None,
        }
    }

    #[test]
    fn only_own_or_collaborative_playlists_reorder() {
        assert!(may_reorder(&owned_by("me", false), Some("me")));
        assert!(may_reorder(&owned_by("other", true), Some("me")));
        assert!(!may_reorder(&owned_by("other", false), Some("me")));
        assert!(!may_reorder(&owned_by("me", false), None), "unknown user");
    }

    #[test]
    fn sort_custom_removes_the_saved_order() {
        let mut orders = HashMap::new();
        apply_sort(
            &mut orders,
            "p",
            &SortKey::Title,
            &SortDirection::Descending,
        );
        assert!(orders.contains_key("p"));
        apply_sort(
            &mut orders,
            "p",
            &SortKey::Custom,
            &SortDirection::Ascending,
        );
        assert!(!orders.contains_key("p"));
    }
}
