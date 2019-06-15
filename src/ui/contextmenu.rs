use std::sync::Arc;

use cursive::view::ViewWrapper;
use cursive::views::{Dialog, SelectView};
use cursive::Cursive;

use clipboard::{ClipboardContext, ClipboardProvider};
use library::Library;
use queue::Queue;
use traits::ListItem;
use ui::layout::Layout;
use ui::modal::Modal;

pub struct ContextMenu {
    dialog: Modal<Dialog>,
}

enum ContextMenuAction {
    ShowItem(Box<ListItem>),
    ShareUrl(String),
}

impl ContextMenu {
    pub fn new(item: &Box<ListItem>, queue: Arc<Queue>, library: Arc<Library>) -> Self {
        let mut content: SelectView<ContextMenuAction> = SelectView::new().autojump();
        if let Some(a) = item.artist() {
            content.add_item("Show artist", ContextMenuAction::ShowItem(Box::new(a)));
        }
        if let Some(a) = item.album(queue.clone()) {
            content.add_item("Show album", ContextMenuAction::ShowItem(Box::new(a)));
        }
        if let Some(url) = item.share_url() {
            content.add_item("Share", ContextMenuAction::ShareUrl(url));
        }

        // open detail view of artist/album
        content.set_on_submit(move |s: &mut Cursive, action: &ContextMenuAction| {
            s.pop_layer();
            let queue = queue.clone();
            let library = library.clone();
            s.call_on_id("main", move |v: &mut Layout| match action {
                ContextMenuAction::ShowItem(item) => {
                    if let Some(view) = item.open(queue, library) {
                        v.push_view(view)
                    }
                }
                ContextMenuAction::ShareUrl(url) => {
                    ClipboardProvider::new()
                        .and_then(|mut ctx: ClipboardContext| ctx.set_contents(url.to_string()))
                        .ok();
                }
            });
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
