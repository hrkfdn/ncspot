use std::sync::{Arc, RwLock};

use cursive::view::ViewWrapper;
use cursive::Cursive;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::playlist::Playlist;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::track::Track;
use crate::traits::ViewExt;
use crate::ui::listview::ListView;

pub struct PlaylistView {
    playlist: Playlist,
    list: ListView<Track>,
    spotify: Arc<Spotify>,
    library: Arc<Library>,
}

impl PlaylistView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, playlist: &Playlist) -> Self {
        let mut playlist = playlist.clone();
        playlist.load_tracks(queue.get_spotify());

        let tracks = if let Some(t) = playlist.tracks.as_ref() {
            t.clone()
        } else {
            Vec::new()
        };

        let spotify = queue.get_spotify();
        let list = ListView::new(Arc::new(RwLock::new(tracks)), queue, library.clone());

        Self {
            playlist,
            list,
            spotify,
            library,
        }
    }
}

impl ViewWrapper for PlaylistView {
    wrap_impl!(self.list: ListView<Track>);
}

impl ViewExt for PlaylistView {
    fn title(&self) -> String {
        self.playlist.name.clone()
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        if let Command::Delete = cmd {
            let pos = self.list.get_selected_index();
            let tracks = if let Some(t) = self.playlist.tracks.as_ref() {
                t.clone()
            } else {
                Vec::new()
            };
            let track = tracks.get(pos);
            if let Some(t) = track {
                if self.playlist.delete_tracks(
                    &[(t.clone(), pos)],
                    self.spotify.clone(),
                    self.library.clone(),
                ) {
                    self.list.remove(pos);
                }
            }
            return Ok(CommandResult::Consumed(None));
        }

        if let Command::Sort(key, direction) = cmd {
            self.list.sort(key, direction);
            return Ok(CommandResult::Consumed(None));
        }

        self.list.on_command(s, cmd)
    }
}
