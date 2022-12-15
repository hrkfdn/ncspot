//! Representation of a [Category](crate::model::category::Category) in a
//! [List].

use cursive::{views::ScrollView, View, event::{Event, MouseEvent, MouseButton, EventResult, Callback}};

use crate::{
    command::{Command, TargetMode},
    commands::CommandResult,
    model::category::Category,
    traits::ViewExt,
    QUEUE, ui::contextmenu::ContextMenu, LIBRARY,
};

use super::{List, ListItem};

#[derive(Clone)]
pub struct CategoryListItem(pub Category);

impl From<Category> for Box<dyn ListItem> {
    fn from(category: Category) -> Self {
        Box::new(CategoryListItem(category))
    }
}

impl View for CategoryListItem {
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
