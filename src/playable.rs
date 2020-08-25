use crate::album::Album;
use crate::artist::Artist;
use crate::episode::Episode;
use crate::library::Library;
use crate::queue::Queue;
use crate::track::Track;
use crate::traits::{ListItem, ViewExt};
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug)]
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

    pub fn cover_url_str(&self) -> String {
        match self.cover_url() {
            Some(cover) => cover.clone(),
            _ => String::from(""),
        }
    }

    pub fn duration(&self) -> u32 {
        match self {
            Playable::Track(track) => track.duration,
            Playable::Episode(episode) => episode.duration,
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

    pub fn title(&self) -> String {
        match self {
            Playable::Track(track) => track.title.clone(),
            Playable::Episode(episode) => episode.name.clone(),
        }
    }

    pub fn artist(&self) -> String {
        match self {
            Playable::Track(track) => track.artists.clone().join(", "),
            Playable::Episode(_) => String::from(""),
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

    fn display_right(&self, library: Arc<Library>) -> String {
        self.as_listitem().display_right(library)
    }

    fn play(&mut self, queue: Arc<Queue>) {
        self.as_listitem().play(queue)
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

    fn artist(&self) -> Option<Artist> {
        self.as_listitem().artist()
    }

    fn track(&self) -> Option<Track> {
        self.as_listitem().track()
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        self.as_listitem()
    }
}
