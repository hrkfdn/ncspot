use chrono::{DateTime, Utc};
use rspotify::model::PlayableItem;

use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::episode::Episode;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::traits::{ListItem, ViewExt};
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum Playable {
    Track(Track),
    Episode(Episode),
}

impl Playable {
    pub fn id(&self) -> Option<String> {
        match self {
            Playable::Track(track) => track.id.clone(),
            Playable::Episode(episode) => Some(episode.id.clone()),
        }
    }

    pub fn uri(&self) -> String {
        match self {
            Playable::Track(track) => track.uri.clone(),
            Playable::Episode(episode) => episode.uri.clone(),
        }
    }

    pub fn cover_url(&self) -> Option<String> {
        match self {
            Playable::Track(track) => track.cover_url.clone(),
            Playable::Episode(episode) => episode.cover_url.clone(),
        }
    }

    pub fn duration(&self) -> u32 {
        match self {
            Playable::Track(track) => track.duration,
            Playable::Episode(episode) => episode.duration,
        }
    }

    pub fn list_index(&self) -> usize {
        match self {
            Playable::Track(track) => track.list_index,
            Playable::Episode(episode) => episode.list_index,
        }
    }

    pub fn set_list_index(&mut self, index: usize) {
        match self {
            Playable::Track(track) => track.list_index = index,
            Playable::Episode(episode) => episode.list_index = index,
        }
    }

    pub fn set_added_at(&mut self, added_at: Option<DateTime<Utc>>) {
        match self {
            Playable::Track(track) => track.added_at = added_at,
            Playable::Episode(episode) => episode.added_at = added_at,
        }
    }

    pub fn duration_str(&self) -> String {
        let duration = self.duration();
        let minutes = duration / 60_000;
        let seconds = (duration / 1000) % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }

    pub fn as_listitem(&self) -> Box<dyn ListItem> {
        match self {
            Playable::Track(track) => track.as_listitem(),
            Playable::Episode(episode) => episode.as_listitem(),
        }
    }
}

impl From<&PlayableItem> for Playable {
    fn from(item: &PlayableItem) -> Self {
        match item {
            PlayableItem::Episode(episode) => Playable::Episode(episode.into()),
            PlayableItem::Track(track) => Playable::Track(track.into()),
        }
    }
}

impl fmt::Display for Playable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Playable::Track(track) => track.fmt(f),
            Playable::Episode(episode) => episode.fmt(f),
        }
    }
}

impl ListItem for Playable {
    fn is_playing(&self, queue: Arc<Queue>) -> bool {
        self.as_listitem().is_playing(queue)
    }

    fn display_left(&self) -> String {
        self.as_listitem().display_left()
    }

    fn display_center(&self, library: Arc<Library>) -> String {
        self.as_listitem().display_center(library)
    }

    fn display_right(&self, library: Arc<Library>) -> String {
        self.as_listitem().display_right(library)
    }

    fn play(&mut self, queue: Arc<Queue>) {
        self.as_listitem().play(queue)
    }

    fn play_next(&mut self, queue: Arc<Queue>) {
        self.as_listitem().play_next(queue)
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        self.as_listitem().queue(queue)
    }

    fn toggle_saved(&mut self, library: Arc<Library>) {
        self.as_listitem().toggle_saved(library)
    }

    fn save(&mut self, library: Arc<Library>) {
        self.as_listitem().save(library)
    }

    fn unsave(&mut self, library: Arc<Library>) {
        self.as_listitem().unsave(library)
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        self.as_listitem().open(queue, library)
    }

    fn share_url(&self) -> Option<String> {
        self.as_listitem().share_url()
    }

    fn album(&self, queue: Arc<Queue>) -> Option<Album> {
        self.as_listitem().album(queue)
    }

    fn artists(&self) -> Option<Vec<Artist>> {
        self.as_listitem().artists()
    }

    fn track(&self) -> Option<Track> {
        self.as_listitem().track()
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        self.as_listitem()
    }
}
