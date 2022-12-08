//! Representation of an [Artist](crate::model::artist::Artist) in a [List].

use cursive::View;

use crate::{
    commands::CommandResult,
    model::artist::Artist,
    traits::ViewExt,
    ui::{artist::ArtistView, printer::PrinterExt},
    LIBRARY, QUEUE, library::Saveable,
};

use super::ListItem;

#[derive(Clone)]
pub struct ArtistListItem(pub Artist);

impl From<Artist> for Box<dyn ListItem> {
    fn from(artist: Artist) -> Self {
        Box::new(ArtistListItem(artist))
    }
}

impl View for ArtistListItem {
    fn draw(&self, printer: &cursive::Printer) {
        printer.print_at_start(&self.0.name);
        printer.print_at_end(&format!(
            "[{}] {:>3} saved tracks",
            match self.0.is_saved(LIBRARY.get().unwrap()) {
                true => "x",
                false => " ",
            },
            &self.0.tracks.as_ref().unwrap_or(&Vec::new()).len()
        ));
    }
}

impl ViewExt for ArtistListItem {
    fn on_command(
        &mut self,
        s: &mut cursive::Cursive,
        cmd: &crate::command::Command,
    ) -> Result<crate::commands::CommandResult, String> {
        match cmd {
            crate::command::Command::Open(crate::command::TargetMode::Selected) => {
                Ok(CommandResult::View(Box::new(ArtistView::new(QUEUE.get().unwrap().clone(), LIBRARY.get().unwrap().clone(), &self.0))))
            }
            _ => Ok(CommandResult::Ignored),
        }
    }
}

impl ListItem for ArtistListItem {
    fn contains(&self, text: &str) -> bool {
        self.0.name.to_lowercase().contains(&text.to_lowercase())
    }
}

