use std::sync::Arc;

use cursive::view::ViewWrapper;
use cursive::views::Dialog;
use cursive::Cursive;

use commands::CommandResult;
use library::Library;
use playlist::Playlist;
use queue::Queue;
use traits::ViewExt;
use ui::listview::ListView;
use ui::modal::Modal;

pub struct PlaylistsView {
    list: ListView<Playlist>,
    library: Arc<Library>,
}

impl PlaylistsView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>) -> Self {
        Self {
            list: ListView::new(library.playlists.clone(), queue.clone(), library.clone()),
            library,
        }
    }

    pub fn delete_dialog(&mut self) -> Option<Modal<Dialog>> {
        let store = self.library.items();
        let current = store.get(self.list.get_selected_index());

        if let Some(playlist) = current {
            let library = self.library.clone();
            let id = playlist.id.clone();
            let dialog = Dialog::text("Are you sure you want to delete this playlist?")
                .padding((1, 1, 1, 0))
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
    fn on_command(
        &mut self,
        s: &mut Cursive,
        cmd: &str,
        args: &[String],
    ) -> Result<CommandResult, String> {
        if cmd == "delete" {
            if let Some(dialog) = self.delete_dialog() {
                s.add_layer(dialog);
            }
            return Ok(CommandResult::Consumed(None));
        }

        self.list.on_command(s, cmd, args)
    }
}
