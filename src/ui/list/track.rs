//! Representation of a [Track](crate::model::track::Track) in a [List].

use cursive::{View, event::EventResult};

use crate::{
    command::Command,
    commands::CommandResult,
    model::{playable::Playable, track::Track},
    traits::ViewExt,
    ui::printer::PrinterExt,
    LIBRARY, QUEUE, library::Saveable,
};

use super::ListItem;

#[derive(Clone)]
pub struct TrackListItem(pub Track);

impl From<Track> for Box<dyn ListItem> {
    fn from(track: Track) -> Self {
        Box::new(TrackListItem(track))
    }
}

impl View for TrackListItem {
    fn draw(&self, printer: &cursive::Printer) {
        printer.print_at_start(&self.0.title);
        let mut end_text = match Saveable::is_saved(&self.0, LIBRARY.get().unwrap()) {
            true => "[x] ",
            false => "[ ] ",
        }
        .to_string();
        end_text.push_str(&self.0.duration_str());
        printer.print_at_end(&end_text);
        printer.print_at_percent_absolute(50, self.0.album.as_ref().unwrap_or(&"".to_string()));
    }

    fn on_event(&mut self, event: cursive::event::Event) -> EventResult {
        match event {
    //         cursive::event::Event::Key(key) => {
    //             // HACK: To allow QueueAll, isn't very easy to do otherwise.
    //             if key == Key::Enter {
    //                 // Start playing the track, but also queue all the other
    //                 // songs in the collection.
    //                 QUEUE.get().unwrap().clear();
    //                 self.0.play(QUEUE.get().unwrap().clone());
    //                 EventResult::Consumed(Some(Callback::from_fn_once(|s| {
    //                     let mut layout: ViewRef<Layout> = s.find_name("main").unwrap();
    //                     layout.on_command(s, &Command::QueueAll).unwrap();
    //                 })))
    //             } else {
    //                 EventResult::Ignored
    //             }
    //         }
            _ => EventResult::Ignored,
        }
    }
}

impl ViewExt for TrackListItem {
    fn on_command(
        &mut self,
        _s: &mut cursive::Cursive,
        cmd: &Command,
    ) -> Result<CommandResult, String> {
        match cmd {
            Command::Queue => {
                QUEUE.get().unwrap().append(Playable::Track(self.0.clone()));
                Ok(CommandResult::Consumed(None))
            }
            _ => Ok(CommandResult::Ignored),
        }
    }

    fn title(&self) -> String {
        "".into()
    }

    fn title_sub(&self) -> String {
        "".into()
    }

    fn on_leave(&self) {}
}

impl ListItem for TrackListItem {
    fn contains(&self, text: &str) -> bool {
        self.0.title.to_lowercase().contains(&text.to_lowercase())
    }
}

