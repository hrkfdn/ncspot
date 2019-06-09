use std::sync::Arc;

use cursive::view::ViewWrapper;
use cursive::views::{Dialog, SelectView};
use cursive::Cursive;

use library::Library;
use queue::Queue;
use traits::ListItem;
use ui::layout::Layout;
use ui::modal::Modal;

pub struct ContextMenu {
    dialog: Modal<Dialog>,
}

impl ContextMenu {
    pub fn new(item: &Box<ListItem>, queue: Arc<Queue>, library: Arc<Library>) -> Self {
        let mut content: SelectView<Box<ListItem>> = SelectView::new().autojump();
        if let Some(a) = item.artist() {
            content.add_item("Artist", Box::new(a))
        }
        if let Some(a) = item.album(queue.clone()) {
            content.add_item("Album", Box::new(a))
        }

        // open detail view of artist/album
        content.set_on_submit(move |s: &mut Cursive, selected: &Box<ListItem>| {
            s.pop_layer();
            let queue = queue.clone();
            let library = library.clone();
            s.call_on_id("main", move |v: &mut Layout| {
                let view = selected.open(queue, library);
                if let Some(view) = view {
                    v.push_view(view)
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
