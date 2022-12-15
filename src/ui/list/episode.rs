//! Representation of an [Episode](crate::model::episode::Episode) in a [List].

use cursive::{View, event::{Event, MouseEvent, MouseButton, Callback, EventResult}};

use crate::{
    command::Command, commands::CommandResult, model::episode::Episode, traits::ViewExt, QUEUE, ui::contextmenu::ContextMenu, LIBRARY,
};

use super::ListItem;

#[derive(Clone)]
pub struct EpisodeListItem(pub Episode);

impl From<Episode> for Box<dyn ListItem> {
    fn from(episode: Episode) -> Self {
        Box::new(EpisodeListItem(episode))
    }
}

impl View for EpisodeListItem {
    fn draw(&self, printer: &cursive::Printer) {
        printer.print((0, 0), &self.0.name);
    }

    fn on_event(&mut self, event: cursive::event::Event) -> cursive::event::EventResult {
        match event {
            Event::Mouse {
                offset: _,
                position: _,
                event: MouseEvent::Press(MouseButton::Right),
            } => {
                let contextmenu = ContextMenu::new(&self.0, QUEUE.get().unwrap().clone(), LIBRARY.get().unwrap().clone());
                return EventResult::Consumed(Some(Callback::from_fn_once(move |s| {
                    s.add_layer(contextmenu)
                })));
            }
            _ => EventResult::Ignored,
        }
    }
}

impl ViewExt for EpisodeListItem {
    fn on_command(
        &mut self,
        _s: &mut cursive::Cursive,
        cmd: &Command,
    ) -> Result<CommandResult, String> {
        match cmd {
            Command::Play => {
                let index = QUEUE.get().unwrap().append_next(&[self.0.clone()]);
                QUEUE.get().unwrap().play(index, true, false);
                Ok(CommandResult::Consumed(None))
            }
            Command::PlayNext => {
                QUEUE.get().unwrap().insert_after_current(self.0.clone());
                Ok(CommandResult::Consumed(None))
            }
            Command::Queue => {
                QUEUE.get().unwrap().append(self.0.clone());
                Ok(CommandResult::Consumed(None))
            }
            _ => Ok(CommandResult::Ignored),
        }
    }
}

impl ListItem for EpisodeListItem {
    fn contains(&self, text: &str) -> bool {
        self.0.name.to_lowercase().contains(&text.to_lowercase())
    }
}
