use std::sync::Arc;

use cursive::view::ViewWrapper;
use cursive::Cursive;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::model::episode::Episode;
use crate::model::show::Show;
use crate::queue::Queue;
use crate::traits::ViewExt;
use crate::ui::listview::ListView;

pub struct ShowView {
    list: ListView<Episode>,
    show: Show,
}

impl ShowView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>, show: &Show) -> Self {
        let spotify = queue.get_spotify();
        let show = show.clone();

        let list = {
            let results = spotify.api.show_episodes(&show.id);
            let view = ListView::new(results.items.clone(), queue, library);
            results.apply_pagination(view.get_pagination());

            view
        };

        Self { list, show }
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
