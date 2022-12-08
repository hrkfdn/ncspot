//! Representation of an [Episode](crate::model::episode::Episode) in a [List].

use cursive::View;

use crate::{model::episode::Episode, traits::ViewExt};

use super::ListItem;

#[derive(Clone)]
pub struct EpisodeListItem(pub Episode);

impl From<Episode> for Box<dyn ListItem> {
    fn from(episode: Episode) -> Self {
        Box::new(EpisodeListItem(episode))
    }
}

impl View for EpisodeListItem {
    fn draw(&self, printer: &cursive::Printer) {
        printer.print((0, 0), &self.0.name);
    }
}

impl ViewExt for EpisodeListItem {}

impl ListItem for EpisodeListItem {
    fn contains(&self, text: &str) -> bool {
        self.0.name.to_lowercase().contains(&text.to_lowercase())
    }
}

