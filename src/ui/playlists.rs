use std::sync::Arc;

use cursive::view::{Margins, ViewWrapper};
use cursive::views::{Dialog, ScrollView};
use cursive::Cursive;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::library::Library;
use crate::model::playlist::Playlist;
use crate::traits::ViewExt;
use crate::ui::modal::Modal;

use super::list::List;

pub struct PlaylistsView {
    list: ScrollView<List<Playlist>>,
    library: Arc<Library>,
}

impl PlaylistsView {
    pub fn new(library: Arc<Library>) -> Self {
        Self {
            list: ScrollView::new(List::new(library.playlists.clone())),
            library,
        }
    }

    pub fn delete_dialog(&mut self) -> Option<Modal<Dialog>> {
        let playlists = self.library.playlists();
        let current = playlists.get(self.list.get_inner().selected_index());

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
    wrap_impl!(self.list: ScrollView<List<Playlist>>);
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
