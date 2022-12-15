//! Representation of a [Category](crate::model::category::Category) in a
//! [List].

use cursive::{views::ScrollView, View};

use crate::{
    command::{Command, TargetMode},
    commands::CommandResult,
    model::category::Category,
    traits::ViewExt,
    QUEUE,
};

use super::{List, ListItem};

#[derive(Clone)]
pub struct CategoryListItem(pub Category);

impl From<Category> for Box<dyn ListItem> {
    fn from(category: Category) -> Self {
        Box::new(CategoryListItem(category))
    }
}

impl AsRef<dyn ListItem> for CategoryListItem {
    fn as_ref(&self) -> &dyn ListItem {
        self
    }
}

impl AsMut<dyn ListItem> for CategoryListItem {
    fn as_mut(&mut self) -> &mut dyn ListItem {
        self
    }
}

impl View for CategoryListItem {
    fn draw(&self, printer: &cursive::Printer) {
        printer.print((0, 0), &self.0.name);
    }
}

impl ViewExt for CategoryListItem {
    fn on_command(
        &mut self,
        _s: &mut cursive::Cursive,
        cmd: &crate::command::Command,
    ) -> Result<crate::commands::CommandResult, String> {
        match cmd {
            Command::Play => {
                // TODO: Implement play for categories.
                Ok(CommandResult::Consumed(None))
            }
            Command::PlayNext => {
                // TODO: Implement play next for categories.
                Ok(CommandResult::Consumed(None))
            }
            Command::Queue => {
                // TODO: Implement queue for categories.
                Ok(CommandResult::Consumed(None))
            }
            Command::Open(TargetMode::Selected) => {
                Ok(CommandResult::View(Box::new(ScrollView::new(List::new(
                    QUEUE
                        .get()
                        .unwrap()
                        .get_spotify()
                        .api
                        .category_playlists(&self.0.id)
                        .items,
                )))))
            }
            _ => Ok(CommandResult::Ignored),
        }
    }
}

impl ListItem for CategoryListItem {
    fn contains(&self, text: &str) -> bool {
        self.0.name.to_lowercase().contains(&text.to_lowercase())
    }
}
