use cursive::traits::{Boxable, Identifiable};
use cursive::view::ViewWrapper;
use cursive::views::{Dialog, EditView, ScrollView, SelectView};
use cursive::Cursive;

use std::cmp::min;
use std::sync::Arc;

use commands::CommandResult;
use library::Library;
use queue::Queue;
use track::Track;
use traits::ViewExt;
use ui::listview::ListView;
use ui::modal::Modal;

pub struct QueueView {
    list: ListView<Track>,
    library: Arc<Library>,
    queue: Arc<Queue>,
}

impl QueueView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>) -> QueueView {
        let list = ListView::new(queue.queue.clone(), queue.clone(), library.clone());

        QueueView {
            list,
            library,
            queue,
        }
    }

    fn save_dialog_cb(
        s: &mut Cursive,
        queue: Arc<Queue>,
        library: Arc<Library>,
        id: Option<String>,
    ) {
        let tracks = queue.queue.read().unwrap().clone();
        match id {
            Some(id) => {
                library.overwrite_playlist(&id, &tracks);
                s.pop_layer();
            }
            None => {
                s.pop_layer();
                let edit = EditView::new()
                    .on_submit(move |s: &mut Cursive, name| {
                        library.save_playlist(name, &tracks);
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

    fn save_dialog(queue: Arc<Queue>, library: Arc<Library>) -> Modal<Dialog> {
        let mut list_select: SelectView<Option<String>> = SelectView::new().autojump();
        list_select.add_item("[Create new]", None);

        for list in library.items().iter() {
            list_select.add_item(list.name.clone(), Some(list.id.clone()));
        }

        list_select.set_on_submit(move |s, selected| {
            Self::save_dialog_cb(s, queue.clone(), library.clone(), selected.clone())
        });

        let dialog = Dialog::new()
            .title("Create new or overwrite existing playlist?")
            .dismiss_button("Cancel")
            .padding((1, 1, 1, 0))
            .content(ScrollView::new(list_select));
        Modal::new(dialog)
    }
}

impl ViewWrapper for QueueView {
    wrap_impl!(self.list: ListView<Track>);
}

impl ViewExt for QueueView {
    fn on_command(
        &mut self,
        s: &mut Cursive,
        cmd: &str,
        args: &[String],
    ) -> Result<CommandResult, String> {
        if cmd == "play" {
            self.queue.play(self.list.get_selected_index(), true);
            return Ok(CommandResult::Consumed(None));
        }

        if cmd == "queue" {
            return Ok(CommandResult::Ignored);
        }

        if cmd == "delete" {
            self.queue.remove(self.list.get_selected_index());
            return Ok(CommandResult::Consumed(None));
        }

        if cmd == "shift" {
            if let Some(dir) = args.get(0) {
                let amount: usize = args
                    .get(1)
                    .unwrap_or(&"1".to_string())
                    .parse()
                    .map_err(|e| format!("{:?}", e))?;
                let selected = self.list.get_selected_index();
                let len = self.queue.len();
                if dir == "up" && selected > 0 {
                    self.queue.shift(selected, selected.saturating_sub(amount));
                    self.list.move_focus(-(amount as i32));
                    return Ok(CommandResult::Consumed(None));
                } else if dir == "down" && selected < len.saturating_sub(1) {
                    self.queue
                        .shift(selected, min(selected + amount as usize, len - 1));
                    self.list.move_focus(amount as i32);
                    return Ok(CommandResult::Consumed(None));
                }
            }
        }

        if cmd == "save" && args.get(0).unwrap_or(&"".to_string()) == "queue" {
            let dialog = Self::save_dialog(self.queue.clone(), self.library.clone());
            s.add_layer(dialog);
            return Ok(CommandResult::Consumed(None));
        }

        self.with_view_mut(move |v| v.on_command(s, cmd, args))
            .unwrap()
    }
}
