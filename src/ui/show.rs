use std::sync::{Arc, RwLock};

use cursive::view::ViewWrapper;
use cursive::Cursive;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::episode::Episode;
use crate::library::Library;
use crate::queue::Queue;
use crate::show::Show;
use crate::traits::ViewExt;
use crate::ui::listview::ListView;

pub struct ShowView {
    list: ListView<Episode>,
    show: Show,
}

impl ShowView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, show: &Show) -> Self {
        let mut show = show.clone();
        show.load_episodes(queue.get_spotify());

        let episodes = show.episodes.clone().unwrap_or_default();

        Self {
            list: ListView::new(Arc::new(RwLock::new(episodes)), queue, library),
            show,
        }
    }
}

impl ViewWrapper for ShowView {
    wrap_impl!(self.list: ListView<Episode>);
}

impl ViewExt for ShowView {
    fn title(&self) -> String {
        self.show.name.clone()
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.list.on_command(s, cmd)
    }
}
