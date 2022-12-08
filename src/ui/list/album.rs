//! Representation of an [Album](crate::model::album::Album) in a [List].

use std::sync::{Arc, RwLock};

use cursive::{views::ScrollView, View};

use super::{List, ListItem};
use crate::{
    commands::CommandResult, model::album::Album, traits::ViewExt, ui::printer::PrinterExt, LIBRARY, library::Saveable,
};

#[derive(Clone)]
pub struct AlbumListItem(pub Album);

impl From<Album> for Box<dyn ListItem> {
    fn from(album: Album) -> Self {
        Box::new(AlbumListItem(album))
    }
}

impl View for AlbumListItem {
    fn draw(&self, printer: &cursive::Printer) {
        let mut artists_album_text = String::new();
        for artist in &self.0.artists {
            artists_album_text.push_str(artist);
            artists_album_text.push_str(", ");
        }
        // TODO: Go sleep... pop pop :D
        artists_album_text.pop();
        artists_album_text.pop();
        artists_album_text.push_str(" - ");
        artists_album_text.push_str(&self.0.title);
        printer.print_at_start(&artists_album_text);
        let mut end_text = match self.0.is_saved(LIBRARY.get().unwrap()) {
            true => "[x] ",
            false => "[ ] ",
        }
        .to_string();
        end_text.push_str(&self.0.year);
        printer.print_at_end(&end_text);
    }
}

impl ViewExt for AlbumListItem {
    fn on_command(
        &mut self,
        s: &mut cursive::Cursive,
        cmd: &crate::command::Command,
    ) -> Result<crate::commands::CommandResult, String> {
        match cmd {
            crate::command::Command::Open(crate::command::TargetMode::Selected) => {
                Ok(CommandResult::View(Box::new(ScrollView::new(List::new(
                    Arc::new(RwLock::new(self.0.tracks.clone().unwrap_or_default())),
                )))))
            }
            _ => Ok(CommandResult::Ignored),
        }
    }
}

impl ListItem for AlbumListItem {
    fn contains(&self, text: &str) -> bool {
        self.0.title.to_lowercase().contains(&text.to_lowercase())
    }
}
