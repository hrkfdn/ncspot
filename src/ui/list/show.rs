//! Representation of a [Show](crate::model::show::Show) in a [List].

use std::sync::{Arc, RwLock};

use cursive::View;

use crate::{
    command::{Command, TargetMode},
    commands::CommandResult,
    model::show::Show,
    traits::ViewExt,
    QUEUE,
};

use super::{List, ListItem};

#[derive(Clone)]
pub struct ShowListItem(pub Show);

impl From<Show> for Box<dyn ListItem> {
    fn from(show: Show) -> Self {
        Box::new(ShowListItem(show))
    }
}

impl View for ShowListItem {
    fn draw(&self, printer: &cursive::Printer) {
        printer.print((0, 0), &self.0.name);
    }
}

impl ViewExt for ShowListItem {
    fn on_command(
        &mut self,
        _s: &mut cursive::Cursive,
        cmd: &crate::command::Command,
    ) -> Result<crate::commands::CommandResult, String> {
        match cmd {
            Command::Play => {
                let queue = QUEUE.get().unwrap();
                self.0.load_all_episodes(queue.get_spotify());

                if let Some(ref episodes) = self.0.episodes {
                    let index = queue.append_next(episodes);
                    queue.play(index, true, true);
                }
                Ok(CommandResult::Consumed(None))
            }
            Command::PlayNext => {
                let queue = QUEUE.get().unwrap();
                self.0.load_all_episodes(queue.get_spotify());

                if let Some(ref episodes) = self.0.episodes {
                    for ep in episodes.iter().rev() {
                        queue.insert_after_current(ep.clone());
                    }
                }
                Ok(CommandResult::Consumed(None))
            }
            Command::Queue => {
                let queue = QUEUE.get().unwrap();
                self.0.load_all_episodes(queue.get_spotify());

                if let Some(ref episodes) = self.0.episodes {
                    for ep in episodes {
                        queue.append(ep.clone());
                    }
                }
                Ok(CommandResult::Consumed(None))
            }
            Command::Open(TargetMode::Selected) => Ok(CommandResult::View(Box::new(List::new(
                Arc::new(RwLock::new(self.0.episodes.clone().unwrap_or_default())),
            )))),
            _ => Ok(CommandResult::Ignored),
        }
    }
}

impl ListItem for ShowListItem {
    fn contains(&self, text: &str) -> bool {
        self.0.name.to_lowercase().contains(&text.to_lowercase())
    }
}
