use std::sync::Arc;

use cursive::Cursive;
use cursive::view::{Margins, ViewWrapper};
use cursive::views::Dialog;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::model::playlist::Playlist;
use crate::queue::Queue;
use crate::traits::ViewExt;
use crate::ui::listview::ListView;
use crate::ui::modal::Modal;

pub struct PlaylistsView {
    list: ListView<Playlist>,
    library: Arc<Library>,
}

impl PlaylistsView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>) -> Self {
        Self {
            list: ListView::new(library.playlists.clone(), queue, library.clone()),
            library,
        }
    }

    pub fn delete_dialog(&mut self) -> Option<Modal<Dialog>> {
        let playlists = self.library.playlists.read().unwrap();
        let current = playlists.get(self.list.get_selected_index());

        if let Some(playlist) = current {
            let library = self.library.clone();
            let id = playlist.id.clone();
            let dialog = Dialog::text("Are you sure you want to delete this playlist?")
                .padding(Margins::lrtb(1, 1, 1, 0))
                .title("Delete playlist")
                .dismiss_button("No")
                .button("Yes", move |s: &mut Cursive| {
                    library.delete_playlist(&id);
                    s.pop_layer();
                });
            Some(Modal::new(dialog))
        } else {
            None
        }
    }
}

impl ViewWrapper for PlaylistsView {
    wrap_impl!(self.list: ListView<Playlist>);
}

impl ViewExt for PlaylistsView {
    fn title(&self) -> String {
        "Playlists".to_string()
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        if let Command::Delete = cmd {
            if let Some(dialog) = self.delete_dialog() {
                s.add_layer(dialog);
            }
            return Ok(CommandResult::Consumed(None));
        }

        self.list.on_command(s, cmd)
    }
}
