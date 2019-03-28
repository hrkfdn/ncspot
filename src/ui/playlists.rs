use std::sync::Arc;

use cursive::traits::Identifiable;
use cursive::view::ViewWrapper;
use cursive::views::{Dialog, IdView};
use cursive::Cursive;

use commands::CommandResult;
use playlists::{Playlist, Playlists};
use queue::Queue;
use traits::ViewExt;
use ui::listview::ListView;
use ui::modal::Modal;

pub struct PlaylistView {
    list: IdView<ListView<Playlist>>,
    playlists: Playlists,
}

pub const LIST_ID: &str = "playlist_list";
impl PlaylistView {
    pub fn new(playlists: &Playlists, queue: Arc<Queue>) -> PlaylistView {
        let list = ListView::new(playlists.store.clone(), queue).with_id(LIST_ID);

        PlaylistView {
            list,
            playlists: playlists.clone(),
        }
    }

    pub fn delete_dialog(&mut self) -> Option<Modal<Dialog>> {
        let list = self.list.get_mut();
        let store = self.playlists.items();
        let current = store.get(list.get_selected_index());

        if let Some(playlist) = current {
            let playlists = self.playlists.clone();
            let id = playlist.meta.id.clone();
            let dialog = Dialog::text("Are you sure you want to delete this playlist?")
                .padding((1, 1, 1, 0))
                .title("Delete playlist")
                .dismiss_button("No")
                .button("Yes", move |s: &mut Cursive| {
                    playlists.delete_playlist(&id);
                    s.pop_layer();
                });
            Some(Modal::new(dialog))
        } else {
            None
        }
    }
}

impl ViewWrapper for PlaylistView {
    wrap_impl!(self.list: IdView<ListView<Playlist>>);
}

impl ViewExt for PlaylistView {
    fn on_command(&mut self,
        s: &mut Cursive,
        cmd: &String,
        args: &[String]
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
