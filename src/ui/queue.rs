use cursive::Cursive;
use cursive::traits::{Nameable, Resizable};
use cursive::view::{Margins, ViewWrapper};
use cursive::views::{Dialog, EditView, ScrollView, SelectView};

use std::cmp::min;
use std::sync::Arc;

use crate::command::{Command, MoveMode, ShiftMode};
use crate::commands::CommandResult;
use crate::library::Library;
use crate::model::playable::Playable;
use crate::queue::Queue;
use crate::traits::ViewExt;
use crate::ui::listview::ListView;
use crate::ui::modal::Modal;

use super::listview::MouseHandleResult;

pub struct QueueView {
    list: ListView<Playable>,
    library: Arc<Library>,
    queue: Arc<Queue>,
}

impl QueueView {
    pub fn new(queue: Arc<Queue>, library: Arc<Library>) -> Self {
        let list = ListView::new(queue.queue.clone(), queue.clone(), library.clone());

        Self {
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
                    .with_name("name")
                    .fixed_width(20);
                let dialog = Dialog::new()
                    .title("Enter name")
                    .dismiss_button("Cancel")
                    .padding(Margins::lrtb(1, 1, 1, 0))
                    .content(edit);
                s.add_layer(Modal::new(dialog));
            }
        }
    }

    fn save_dialog(queue: Arc<Queue>, library: Arc<Library>) -> Modal<Dialog> {
        let mut list_select: SelectView<Option<String>> = SelectView::new().autojump();
        list_select.add_item("[Create new]", None);

        for list in library.playlists.read().unwrap().iter() {
            list_select.add_item(list.name.clone(), Some(list.id.clone()));
        }

        list_select.set_on_submit(move |s, selected| {
            Self::save_dialog_cb(s, queue.clone(), library.clone(), selected.clone())
        });

        let dialog = Dialog::new()
            .title("Create new or overwrite existing playlist?")
            .dismiss_button("Cancel")
            .padding(Margins::lrtb(1, 1, 1, 0))
            .content(ScrollView::new(list_select));
        Modal::new(dialog)
    }
}

impl ViewWrapper for QueueView {
    wrap_impl!(self.list: ListView<Playable>);

    fn wrap_on_event(&mut self, ch: cursive::event::Event) -> cursive::event::EventResult {
        let mouse_result = self.with_view_mut(|v| v.handle_mouse_event(ch));
        mouse_result
            .map(|result| match result {
                MouseHandleResult::Handled(event_result) => event_result,
                MouseHandleResult::Unhandled(command) => match command {
                    Command::Play => {
                        self.queue.play(self.list.get_selected_index(), true, false);
                        cursive::event::EventResult::consumed()
                    }
                    _ => cursive::event::EventResult::Ignored,
                },
            })
            .unwrap_or_else(|| cursive::event::EventResult::Ignored)
    }
}

impl ViewExt for QueueView {
    fn title(&self) -> String {
        "Queue".to_string()
    }

    fn title_sub(&self) -> String {
        let track_count = self.queue.len();
        let duration_secs: u64 = self
            .queue
            .queue
            .read()
            .unwrap()
            .iter()
            .map(|p| p.duration() as u64 / 1000)
            .sum();

        if duration_secs > 0 {
            let duration = std::time::Duration::from_secs(duration_secs);
            format!(
                "{} tracks, {}",
                track_count,
                crate::utils::format_duration(&duration)
            )
        } else {
            "".to_string()
        }
    }

    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Play => {
                self.queue.play(self.list.get_selected_index(), true, false);
                return Ok(CommandResult::Consumed(None));
            }
            Command::PlayNext => {
                return Ok(CommandResult::Ignored);
            }
            Command::Queue => {
                return Ok(CommandResult::Ignored);
            }
            Command::Delete => {
                let selected = self.list.get_selected_index();
                let len = self.queue.len();

                self.queue.remove(selected);
                if selected == len.saturating_sub(1) {
                    self.list.move_focus(-1);
                }
                return Ok(CommandResult::Consumed(None));
            }
            Command::Shift(mode, amount) => {
                let amount = match amount {
                    Some(amount) => *amount,
                    _ => 1,
                };

                let selected = self.list.get_selected_index();
                let len = self.queue.len();

                match mode {
                    ShiftMode::Up if selected > 0 => {
                        self.queue
                            .shift(selected, (selected as i32).saturating_sub(amount) as usize);
                        self.list.move_focus(-amount);
                        return Ok(CommandResult::Consumed(None));
                    }
                    ShiftMode::Down if selected < len.saturating_sub(1) => {
                        self.queue
                            .shift(selected, min(selected + amount as usize, len - 1));
                        self.list.move_focus(amount);
                        return Ok(CommandResult::Consumed(None));
                    }
                    _ => {}
                }
            }
            Command::SaveQueue => {
                let dialog = Self::save_dialog(self.queue.clone(), self.library.clone());
                s.add_layer(dialog);
                return Ok(CommandResult::Consumed(None));
            }
            Command::Move(MoveMode::Playing, _) => {
                if let Some(playing) = self.queue.get_current_index() {
                    self.list.move_focus_to(playing);
                }
                return Ok(CommandResult::Consumed(None));
            }
            _ => {}
        }

        self.with_view_mut(move |v| v.on_command(s, cmd)).unwrap()
    }
}
