//! Representation of a [Playlist](crate::model::playlist::Playlist) in a
//! [List].

use std::sync::{Arc, RwLock};

use cursive::{views::ScrollView, View};

use crate::{
    command::{Command, TargetMode},
    commands::CommandResult,
    library::Saveable,
    model::playlist::Playlist,
    traits::ViewExt,
    ui::printer::PrinterExt,
    LIBRARY, QUEUE,
};

use super::{List, ListItem};

#[derive(Clone)]
pub struct PlaylistListItem(pub Playlist);

impl From<Playlist> for Box<dyn ListItem> {
    fn from(playlist: Playlist) -> Self {
        Box::new(PlaylistListItem(playlist))
    }
}

impl View for PlaylistListItem {
    fn draw(&self, printer: &cursive::Printer) {
        let title = &self.0.name;
        let author = self
            .0
            .owner_name
            .clone()
            .unwrap_or_else(|| String::from("Unknown"));
        let saved = match Saveable::is_saved(&self.0, LIBRARY.get().unwrap()) {
            true => "x",
            false => " ",
        };
        let amount = self.0.num_tracks;
        printer.print_at_start(&format!("{title} - {author}"));
        printer.print_at_end(&format!("[{saved}] {amount:>4} tracks"));
    }
}

impl ViewExt for PlaylistListItem {
    fn on_command(
        &mut self,
        _s: &mut cursive::Cursive,
        cmd: &Command,
    ) -> Result<CommandResult, String> {
        match cmd {
            Command::Play => {
                let queue = QUEUE.get().unwrap();
                self.0.load_tracks(queue.get_spotify());

                if let Some(ref tracks) = self.0.tracks {
                    let index = queue.append_next(tracks);
                    queue.play(index, true, true);
                }
                Ok(CommandResult::Consumed(None))
            }
            Command::PlayNext => {
                let queue = QUEUE.get().unwrap();
                self.0.load_tracks(queue.get_spotify());

                if let Some(ref tracks) = self.0.tracks {
                    for track in tracks.iter().rev() {
                        queue.insert_after_current(track.clone());
                    }
                }

                Ok(CommandResult::Consumed(None))
            }
            Command::Queue => {
                let queue = QUEUE.get().unwrap();
                self.0.load_tracks(queue.get_spotify());

                if let Some(ref tracks) = self.0.tracks {
                    for track in tracks {
                        queue.append(track.clone());
                    }
                }

                Ok(CommandResult::Consumed(None))
            }
            Command::Open(TargetMode::Selected) => {
                Ok(CommandResult::View(Box::new(ScrollView::new(List::new(
                    Arc::new(RwLock::new(self.0.tracks.clone().unwrap_or_default())),
                )))))
            }
            _ => Ok(CommandResult::Ignored),
        }
    }
}

impl ListItem for PlaylistListItem {
    fn contains(&self, text: &str) -> bool {
        self.0.name.to_lowercase().contains(&text.to_lowercase())
    }
}
