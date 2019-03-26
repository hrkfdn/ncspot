use cursive::event::{Callback, Event, EventResult};
use cursive::traits::{Boxable, Identifiable, View};
use cursive::view::ViewWrapper;
use cursive::views::{Dialog, EditView, IdView, ScrollView, SelectView};
use cursive::Cursive;

use std::sync::Arc;

use playlists::Playlists;
use queue::Queue;
use track::Track;
use ui::listview::ListView;
use ui::modal::Modal;

pub struct QueueView {
    list: IdView<ListView<Track>>,
    playlists: Arc<Playlists>,
}

impl QueueView {
    pub fn new(queue: Arc<Queue>, playlists: Arc<Playlists>) -> QueueView {
        let list = ListView::new(queue.queue.clone(), queue.clone()).with_id("queue_list");

        QueueView { list, playlists }
    }

    fn save_dialog_cb(s: &mut Cursive, playlists: Arc<Playlists>, id: Option<String>) {
        let tracks = s
            .call_on_id("queue_list", |view: &mut ListView<_>| {
                view.content().clone()
            })
            .unwrap();
        match id {
            Some(id) => {
                playlists.overwrite_playlist(&id, &tracks);
                s.pop_layer();
            }
            None => {
                s.pop_layer();
                let edit = EditView::new()
                    .on_submit(move |s: &mut Cursive, name| {
                        playlists.save_playlist(name, &tracks);
                        s.pop_layer();
                    })
                    .with_id("name")
                    .fixed_width(20);
                let dialog = Dialog::new()
                    .title("Enter name")
                    .dismiss_button("Cancel")
                    .padding((1, 1, 1, 0))
                    .content(edit);
                s.add_layer(Modal::new(dialog));
            }
        }
    }

    fn save_dialog(playlists: Arc<Playlists>) -> Modal<Dialog> {
        let mut list_select: SelectView<Option<String>> = SelectView::new().autojump();
        list_select.add_item("[Create new]", None);

        for list in playlists.items().iter() {
            list_select.add_item(list.meta.name.clone(), Some(list.meta.id.clone()));
        }

        list_select.set_on_submit(move |s, selected| {
            Self::save_dialog_cb(s, playlists.clone(), selected.clone())
        });

        let dialog = Dialog::new()
            .title("Save to existing or new playlist?")
            .dismiss_button("Cancel")
            .padding((1, 1, 1, 0))
            .content(ScrollView::new(list_select));
        Modal::new(dialog)
    }
}

impl ViewWrapper for QueueView {
    wrap_impl!(self.list: IdView<ListView<Track>>);

    fn wrap_on_event(&mut self, ch: Event) -> EventResult {
        match ch {
            Event::Char('s') => {
                debug!("save list");
                let playlists = self.playlists.clone();
                let cb = move |s: &mut Cursive| {
                    let dialog = Self::save_dialog(playlists.clone());
                    s.add_layer(dialog)
                };
                EventResult::Consumed(Some(Callback::from_fn(cb)))
            }
            _ => self.list.on_event(ch),
        }
    }
}
