use crate::library::Library;
use crate::model::playable::Playable;
use crate::queue::Queue;
use crate::traits::{ListItem, ViewExt};
use chrono::{DateTime, Utc};
use rspotify::model::show::{FullEpisode, SimplifiedEpisode};
use rspotify::model::Id;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Episode {
    pub id: String,
    pub uri: String,
    pub duration: u32,
    pub name: String,
    pub description: String,
    pub release_date: String,
    pub cover_url: Option<String>,
    pub added_at: Option<DateTime<Utc>>,
    pub list_index: usize,
}

impl Episode {
    pub fn duration_str(&self) -> String {
        let minutes = self.duration / 60_000;
        let seconds = (self.duration / 1000) % 60;
        format!("{minutes:02}:{seconds:02}")
    }
}

impl From<&SimplifiedEpisode> for Episode {
    fn from(episode: &SimplifiedEpisode) -> Self {
        Self {
            id: episode.id.id().to_string(),
            uri: episode.id.uri(),
            duration: episode.duration.as_millis() as u32,
            name: episode.name.clone(),
            description: episode.description.clone(),
            release_date: episode.release_date.clone(),
            cover_url: episode.images.get(0).map(|img| img.url.clone()),
            added_at: None,
            list_index: 0,
        }
    }
}

impl From<&FullEpisode> for Episode {
    fn from(episode: &FullEpisode) -> Self {
        Self {
            id: episode.id.id().to_string(),
            uri: episode.id.uri(),
            duration: episode.duration.as_millis() as u32,
            name: episode.name.clone(),
            description: episode.description.clone(),
            release_date: episode.release_date.clone(),
            cover_url: episode.images.get(0).map(|img| img.url.clone()),
            added_at: None,
            list_index: 0,
        }
    }
}

impl fmt::Display for Episode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl ListItem for Episode {
    fn is_playing(&self, queue: Arc<Queue>) -> bool {
        let current = queue.get_current();
        current
            .map(|t| t.id() == Some(self.id.clone()))
            .unwrap_or(false)
    }

    fn display_left(&self, _library: Arc<Library>) -> String {
        self.name.clone()
    }

    fn display_right(&self, _library: Arc<Library>) -> String {
        format!("{} [{}]", self.duration_str(), self.release_date)
    }

    fn play(&mut self, queue: Arc<Queue>) {
        let index = queue.append_next(&vec![Playable::Episode(self.clone())]);
        queue.play(index, true, false);
    }

    fn play_next(&mut self, queue: Arc<Queue>) {
        queue.insert_after_current(Playable::Episode(self.clone()));
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        queue.append(Playable::Episode(self.clone()));
    }

    fn toggle_saved(&mut self, _library: Arc<Library>) {}

    fn save(&mut self, _library: Arc<Library>) {}

    fn unsave(&mut self, _library: Arc<Library>) {}

    fn open(&self, _queue: Arc<Queue>, _library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        None
    }

    fn share_url(&self) -> Option<String> {
        Some(format!("https://open.spotify.com/episode/{}", self.id))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
