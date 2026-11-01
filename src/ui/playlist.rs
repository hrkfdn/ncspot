use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

use cursive::view::{View, ViewWrapper};
use cursive::{Cursive, Vec2};

use crate::command::{Command, ShiftMode, SortDirection, SortKey};
use crate::commands::CommandResult;
use crate::config::SortingOrder;
use crate::library::{Library, is_owned};
use crate::model::playable::Playable;
use crate::model::playlist::Playlist;
use crate::queue::Queue;

use crate::traits::ViewExt;
use crate::ui::listview::ListView;

const SORTED_NOTE: &str = "Sort is on: use 'sort custom' to reorder";
const NOT_OWNED_NOTE: &str = "Only your own or collaborative playlists can be reordered";

/// Whether the library's playlists revision moved on since the view last took the store copy.
fn needs_refresh(seen: u64, current: u64) -> bool {
    seen != current
}

/// The store's copy of playlist `id`, when it has its tracks loaded (a copy without them would
/// empty the view).
fn store_copy(library: &Library, id: &str) -> Option<Playlist> {
    library
        .playlists
        .read()
        .unwrap()
        .iter()
        .find(|p| p.id == id && p.tracks.is_some())
        .cloned()
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

/// A playlist's tracks. `playlist` is the store's copy in server order (edits are queued on
/// the store and the view follows it); only the rows of `list` follow the saved sort order.
pub struct PlaylistView {
    playlist: Playlist,
    rows: Arc<RwLock<Vec<Playable>>>,
    list: ListView<Playable>,
    library: Arc<Library>,
    queue: Arc<Queue>,
    /// The `Library::playlists_rev` the view last took the store copy at.
    seen_rev: u64,
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

        let seen_rev = library.playlists_rev.load(Ordering::Relaxed);
        let rows = Arc::new(RwLock::new(sorted_rows(&library, &playlist)));
        let list = ListView::new(rows.clone(), queue.clone(), library.clone());

        Self {
            playlist,
            rows,
            list,
            library,
            queue,
            seen_rev,
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

    /// Take the store's copy (queued edits are already applied to it) and show it, keeping the
    /// selected index.
    fn refresh_from_store(&mut self) {
        // Read first: an edit bumping the revision after this read is seen on the next poll.
        self.seen_rev = self.library.playlists_rev.load(Ordering::Relaxed);
        let Some(copy) = store_copy(&self.library, &self.playlist.id) else {
            return;
        };
        self.playlist = copy;
        let selected = self.list.get_selected_index();
        self.show_tracks();
        self.list.move_focus_to(selected);
    }

    /// Move the selected track `amount` rows (`shift up|down [amount]`); the note to show, if
    /// any. Refused in a sorted view or a playlist the user may not reorder. The move is
    /// queued and shown at once; the focus follows the track.
    fn shift(&mut self, mode: &ShiftMode, amount: i32) -> Option<String> {
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
        if let Some(at) = self.library.queue_move(&self.playlist.id, &row, n, up) {
            self.refresh_from_store();
            self.list.move_focus_to(at);
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

    /// Follow the store after every event or redraw trigger (cursive lays the view out on each
    /// refresh), so a worker settle shows without a key press.
    fn wrap_layout(&mut self, size: Vec2) {
        let current = self.library.playlists_rev.load(Ordering::Relaxed);
        if needs_refresh(self.seen_rev, current) {
            self.refresh_from_store();
        }
        self.list.layout(size);
    }
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

    fn playlist_id(&self) -> Option<String> {
        Some(self.playlist.id.clone())
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

            return match self.library.queue_remove(&self.playlist.id, &row) {
                Ok(()) => {
                    // The later rows moved one server position up.
                    self.refresh_from_store();
                    self.list.move_focus_to(pos);
                    Ok(CommandResult::Consumed(None))
                }
                Err(note) => Err(note),
            };
        }

        if let Command::Shift(mode, amount) = cmd {
            return Ok(CommandResult::Consumed(
                self.shift(mode, amount.unwrap_or(1)),
            ));
        }

        if let Command::Rename(name) = cmd {
            let note = self.library.queue_rename(&self.playlist.id, name);
            return Ok(CommandResult::Consumed(note));
        }

        if let Command::Pin | Command::Unpin = cmd {
            // The worker posts the failure note, if any.
            self.library
                .queue_pin(&self.playlist.id, matches!(cmd, Command::Pin));
            return Ok(CommandResult::Consumed(None));
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
    use crate::config::{CACHE_VERSION, Config};
    use crate::events::EventManager;
    use crate::spotify::Spotify;

    /// A view of playlist "p" (owned by the user) holding the placeholder tracks `uris`, over
    /// a library with no worker running.
    fn test_view(uris: &[&str]) -> (PlaylistView, Arc<Library>) {
        let cfg = Config::new_for_test();
        cfg.with_state_mut(|s| s.cache_version = CACHE_VERSION);
        let ev = EventManager::new_for_test();
        let spotify = Spotify::new_for_test(cfg.clone(), ev.clone());
        let mut library = Library::new_for_test(ev, spotify.clone(), cfg.clone());
        Arc::get_mut(&mut library).unwrap().user_id = Some("me".into());
        let tracks: Vec<Playable> = uris
            .iter()
            .enumerate()
            .map(|(i, uri)| {
                let mut playable = crate::playlist_items::placeholder(uri);
                playable.set_list_index(i);
                playable
            })
            .collect();
        let mut playlist = owned_by("me", false);
        playlist.snapshot_id = "s0".into();
        playlist.num_tracks = tracks.len();
        playlist.tracks = Some(tracks);
        *library.playlists.write().unwrap() = vec![playlist.clone()];
        let queue = Arc::new(Queue::new(spotify, cfg, library.clone()));
        (
            PlaylistView::new(queue, library.clone(), &playlist),
            library,
        )
    }

    fn view_uris(view: &PlaylistView) -> Vec<String> {
        view.rows
            .read()
            .unwrap()
            .iter()
            .map(Playable::uri)
            .collect()
    }

    #[test]
    fn shift_queues_and_moves_the_focus_without_waiting() {
        let (mut view, library) = test_view(&["a", "b", "c"]);
        assert_eq!(view.shift(&ShiftMode::Down, 1), None);
        assert_eq!(view.list.get_selected_index(), 1);
        assert_eq!(view_uris(&view), ["b", "a", "c"]);
        let stored: Vec<String> = library.playlists.read().unwrap()[0]
            .tracks
            .as_ref()
            .unwrap()
            .iter()
            .map(Playable::uri)
            .collect();
        assert_eq!(stored, ["b", "a", "c"], "the store holds the shown order");
    }

    #[test]
    fn the_view_takes_the_store_copy_when_the_revision_changes() {
        let (mut view, library) = test_view(&["a", "b", "c"]);
        let row = view.rows.read().unwrap()[2].clone();
        library.queue_remove("p", &row).unwrap();
        assert_eq!(view_uris(&view), ["a", "b", "c"], "not yet polled");
        assert!(needs_refresh(
            view.seen_rev,
            library.playlists_rev.load(Ordering::Relaxed)
        ));
        view.refresh_from_store();
        assert_eq!(view_uris(&view), ["a", "b"]);
        assert!(!needs_refresh(
            view.seen_rev,
            library.playlists_rev.load(Ordering::Relaxed)
        ));
    }

    #[test]
    fn a_refresh_keeps_the_selected_index() {
        let (mut view, library) = test_view(&["a", "b", "c"]);
        view.list.move_focus_to(1);
        library.queue_rename("p", "new");
        view.refresh_from_store();
        assert_eq!(view.list.get_selected_index(), 1);
        view.list.move_focus_to(2);
        let row = view.rows.read().unwrap()[2].clone();
        library.queue_remove("p", &row).unwrap();
        view.refresh_from_store();
        assert_eq!(view.list.get_selected_index(), 1, "clamped");
    }

    #[test]
    fn a_store_copy_without_tracks_is_not_taken() {
        let (_view, library) = test_view(&["a"]);
        assert!(store_copy(&library, "p").is_some());
        library.playlists.write().unwrap()[0].tracks = None;
        assert!(store_copy(&library, "p").is_none());
        assert!(store_copy(&library, "other").is_none());
    }

    #[test]
    fn the_revision_check_is_a_change_check() {
        assert!(!needs_refresh(3, 3));
        assert!(needs_refresh(3, 4));
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
