use std::iter::Iterator;
use std::sync::Arc;

use library::Library;
use queue::Queue;
use track::Track;
use traits::{IntoBoxedViewExt, ListItem, ViewExt};
use ui::playlist::PlaylistView;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub snapshot_id: String,
    pub tracks: Vec<Track>,
}

impl ListItem for Playlist {
    fn is_playing(&self, queue: Arc<Queue>) -> bool {
        let playing: Vec<String> = queue
            .queue
            .read()
            .unwrap()
            .iter()
            .filter(|t| t.id.is_some())
            .map(|t| t.id.clone().unwrap())
            .collect();
        let ids: Vec<String> = self
            .tracks
            .iter()
            .filter(|t| t.id.is_some())
            .map(|t| t.id.clone().unwrap())
            .collect();
        !ids.is_empty() && playing == ids
    }

    fn display_left(&self) -> String {
        self.name.clone()
    }

    fn display_right(&self, library: Arc<Library>) -> String {
        let saved = if library.is_saved_playlist(self) {
            if library.use_nerdfont {
                "\u{f62b} "
            } else {
                "✓ "
            }
        } else {
            ""
        };
        format!("{}{:>3} tracks", saved, self.tracks.len())
    }

    fn play(&mut self, queue: Arc<Queue>) {
        let index = queue.append_next(self.tracks.iter().collect());
        queue.play(index, true);
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        for track in self.tracks.iter() {
            queue.append(track);
        }
    }

    fn toggle_saved(&mut self, library: Arc<Library>) {
        if library.is_saved_playlist(self) {
            library.delete_playlist(&self.id);
        } else {
            library.follow_playlist(self);
        }
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(PlaylistView::new(queue, library, self).as_boxed_view_ext())
    }
}
