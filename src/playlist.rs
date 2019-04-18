use std::iter::Iterator;
use std::sync::Arc;

use queue::Queue;
use library::Library;
use track::Track;
use traits::ListItem;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
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
            .map(|t| t.id.clone())
            .collect();
        let ids: Vec<String> = self.tracks.iter().map(|t| t.id.clone()).collect();
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

    fn toggle_saved(&mut self, _library: Arc<Library>) {
        // TODO
    }
}
