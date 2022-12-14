//! Representation of a [Playable](crate::model::playable::Playable) in a
//! [List].

use cursive::View;

use crate::{
    command::Command, commands::CommandResult, model::playable::Playable, traits::ViewExt,
};

use super::{episode::EpisodeListItem, track::TrackListItem, ListItem};

#[derive(Clone)]
pub struct PlayableListItem(pub Playable);

impl From<Playable> for Box<dyn ListItem> {
    fn from(playable: Playable) -> Self {
        Box::new(PlayableListItem(playable))
    }
}

impl View for PlayableListItem {
    fn draw(&self, printer: &cursive::Printer) {
        match self.0 {
            Playable::Track(ref track) => <Box<dyn ListItem>>::from(track.clone()).draw(printer),
            Playable::Episode(ref episode) => {
                <Box<dyn ListItem>>::from(episode.clone()).draw(printer)
            }
        }
    }

    fn on_event(&mut self, event: cursive::event::Event) -> cursive::event::EventResult {
        match self.0 {
            Playable::Track(ref track) => TrackListItem(track.clone()).on_event(event),
            Playable::Episode(ref episode) => EpisodeListItem(episode.clone()).on_event(event),
        }
    }
}

impl ViewExt for PlayableListItem {
    fn on_command(
        &mut self,
        s: &mut cursive::Cursive,
        cmd: &Command,
    ) -> Result<CommandResult, String> {
        match self.0 {
            Playable::Track(ref track) => TrackListItem(track.clone()).on_command(s, cmd),
            Playable::Episode(ref episode) => EpisodeListItem(episode.clone()).on_command(s, cmd),
        }
    }
}

impl ListItem for PlayableListItem {
    fn contains(&self, text: &str) -> bool {
        match self.0 {
            Playable::Track(ref track) => <Box<dyn ListItem>>::from(track.clone()).contains(text),
            Playable::Episode(ref episode) => {
                <Box<dyn ListItem>>::from(episode.clone()).contains(text)
            }
        }
    }
}
