use std::sync::{Arc, RwLock};

use cursive::view::ViewWrapper;
use cursive::Cursive;

use commands::CommandResult;
use library::Library;
use playlist::Playlist;
use queue::Queue;
use track::Track;
use traits::ViewExt;
use ui::listview::ListView;

pub struct PlaylistView {
    playlist: Playlist,
    list: ListView<Track>,
}

impl PlaylistView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, playlist: &Playlist) -> Self {
        let playlist = playlist.clone();
        let list = ListView::new(
            Arc::new(RwLock::new(playlist.tracks.clone())),
            queue,
            library,
        );

        Self { playlist, list }
    }
}

impl ViewWrapper for PlaylistView {
    wrap_impl!(self.list: ListView<Track>);
}

impl ViewExt for PlaylistView {
    fn title(&self) -> String {
        self.playlist.name.clone()
    }

    fn on_command(
        &mut self,
        s: &mut Cursive,
        cmd: &str,
        args: &[String],
    ) -> Result<CommandResult, String> {
        self.list.on_command(s, cmd, args)
    }
}
