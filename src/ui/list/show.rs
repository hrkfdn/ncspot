//! Representation of a [Show](crate::model::show::Show) in a [List].

use std::sync::{Arc, RwLock};

use cursive::View;

use crate::{
    command::{Command, TargetMode},
    commands::CommandResult,
    model::show::Show,
    traits::ViewExt,
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
        s: &mut cursive::Cursive,
        cmd: &crate::command::Command,
    ) -> Result<crate::commands::CommandResult, String> {
        match cmd {
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

