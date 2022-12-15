//! Representation of an [Artist](crate::model::artist::Artist) in a [List].

use cursive::{View, event::{Callback, EventResult, MouseButton, MouseEvent, Event}};

use crate::{
    command::Command,
    commands::CommandResult,
    library::Saveable,
    model::artist::Artist,
    traits::ViewExt,
    ui::{artist::ArtistView, printer::PrinterExt, contextmenu::ContextMenu},
    LIBRARY, QUEUE,
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

impl ViewExt for ArtistListItem {
    fn on_command(
        &mut self,
        _s: &mut cursive::Cursive,
        cmd: &crate::command::Command,
    ) -> Result<crate::commands::CommandResult, String> {
        match cmd {
            Command::Play => {
                let queue = QUEUE.get().unwrap();
                self.0.load_top_tracks(queue.get_spotify());

                if let Some(ref tracks) = self.0.tracks {
                    let index = queue.append_next(tracks);
                    queue.play(index, true, true);
                }
                Ok(CommandResult::Consumed(None))
            }
            Command::PlayNext => {
                let queue = QUEUE.get().unwrap();
                self.0.load_top_tracks(queue.get_spotify());
                if let Some(ref tracks) = self.0.tracks {
                    for t in tracks.iter().rev() {
                        queue.insert_after_current(t.clone());
                    }
                }
                Ok(CommandResult::Consumed(None))
            }
            Command::Queue => {
                let queue = QUEUE.get().unwrap();
                self.0.load_top_tracks(queue.get_spotify());

                if let Some(ref tracks) = self.0.tracks {
                    for t in tracks {
                        queue.append(t.clone());
                    }
                }
                Ok(CommandResult::Consumed(None))
            }
            Command::Open(crate::command::TargetMode::Selected) => {
                Ok(CommandResult::View(Box::new(ArtistView::new(
                    QUEUE.get().unwrap().clone(),
                    LIBRARY.get().unwrap().clone(),
                    &self.0,
                ))))
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
