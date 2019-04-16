use std::sync::Arc;

use cursive::view::ViewWrapper;
use cursive::views::Dialog;
use cursive::Cursive;

use commands::CommandResult;
use library::Library;
use queue::Queue;
use traits::ViewExt;
use ui::listview::ListView;
use ui::modal::Modal;
use ui::tabview::TabView;

pub struct LibraryView {
    list: TabView,
    library: Arc<Library>,
}

impl LibraryView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>) -> Self {
        let tabs = TabView::new()
            .tab("tracks", "Tracks", ListView::new(library.tracks.clone(), queue.clone()))
            .tab("albums", "Albums", ListView::new(library.albums.clone(), queue.clone()))
            .tab("artists", "Artists", ListView::new(library.artists.clone(), queue.clone()))
            .tab("playlists", "Playlists", ListView::new(library.playlists.clone(), queue.clone()));

        Self {
            list: tabs,
            library,
        }
    }

    pub fn delete_dialog(&mut self) -> Option<Modal<Dialog>> {
        return None;

        // TODO
        //let store = self.library.items();
        //let current = store.get(self.list.get_selected_index());

        //if let Some(playlist) = current {
        //    let library = self.library.clone();
        //    let id = playlist.id.clone();
        //    let dialog = Dialog::text("Are you sure you want to delete this playlist?")
        //        .padding((1, 1, 1, 0))
        //        .title("Delete playlist")
        //        .dismiss_button("No")
        //        .button("Yes", move |s: &mut Cursive| {
        //            library.delete_playlist(&id);
        //            s.pop_layer();
        //        });
        //    Some(Modal::new(dialog))
        //} else {
        //    None
        //}
    }
}

impl ViewWrapper for LibraryView {
    wrap_impl!(self.list: TabView);
}

impl ViewExt for LibraryView {
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
