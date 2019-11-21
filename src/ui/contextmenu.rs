use std::sync::Arc;

use cursive::view::ViewWrapper;
use cursive::views::{Dialog, ScrollView, SelectView};
use cursive::Cursive;

#[cfg(feature = "share_clipboard")]
use clipboard::{ClipboardContext, ClipboardProvider};
use library::Library;
use queue::Queue;
use track::Track;
use traits::ListItem;
use ui::layout::Layout;
use ui::modal::Modal;

pub struct ContextMenu {
    dialog: Modal<Dialog>,
}

enum ContextMenuAction {
    ShowItem(Box<dyn ListItem>),
    ShareUrl(String),
    AddToPlaylist(Box<Track>),
}

impl ContextMenu {
    pub fn add_track_dialog(library: Arc<Library>, track: Track) -> Modal<Dialog> {
        let mut list_select: SelectView<String> = SelectView::new().autojump();

        for list in library.items().iter() {
            list_select.add_item(list.name.clone(), list.id.clone());
        }

        list_select.set_on_submit(move |s, selected| {
            library.playlist_append_tracks(selected, &[track.clone()]);
            s.pop_layer();
        });

        let dialog = Dialog::new()
            .title("Add track to playlist")
            .dismiss_button("Cancel")
            .padding((1, 1, 1, 0))
            .content(ScrollView::new(list_select));
        Modal::new(dialog)
    }

    pub fn new(item: &dyn ListItem, queue: Arc<Queue>, library: Arc<Library>) -> Self {
        let mut content: SelectView<ContextMenuAction> = SelectView::new().autojump();
        if let Some(a) = item.artist() {
            content.add_item("Show artist", ContextMenuAction::ShowItem(Box::new(a)));
        }
        if let Some(a) = item.album(queue.clone()) {
            content.add_item("Show album", ContextMenuAction::ShowItem(Box::new(a)));
        }
        if let Some(url) = item.share_url() {
            #[cfg(feature = "share_clipboard")]
            content.add_item("Share", ContextMenuAction::ShareUrl(url));
        }
        if let Some(t) = item.track() {
            content.add_item(
                "Add to playlist",
                ContextMenuAction::AddToPlaylist(Box::new(t)),
            )
        }

        // open detail view of artist/album
        content.set_on_submit(move |s: &mut Cursive, action: &ContextMenuAction| {
            s.pop_layer();
            let queue = queue.clone();
            let library = library.clone();

            match action {
                ContextMenuAction::ShowItem(item) => {
                    if let Some(view) = item.open(queue, library) {
                        s.call_on_id("main", move |v: &mut Layout| v.push_view(view));
                    }
                }
                ContextMenuAction::ShareUrl(url) => {
                    #[cfg(feature = "share_clipboard")]
                    ClipboardProvider::new()
                        .and_then(|mut ctx: ClipboardContext| ctx.set_contents(url.to_string()))
                        .ok();
                }
                ContextMenuAction::AddToPlaylist(track) => {
                    let dialog = Self::add_track_dialog(library, *track.clone());
                    s.add_layer(dialog);
                }
            }
        });

        let dialog = Dialog::new()
            .title(item.display_left())
            .dismiss_button("Cancel")
            .padding((1, 1, 1, 0))
            .content(content);
        Self {
            dialog: Modal::new(dialog),
        }
    }
}

impl ViewWrapper for ContextMenu {
    wrap_impl!(self.dialog: Modal<Dialog>);
}
