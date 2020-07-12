use crate::library::Library;
use crate::playable::Playable;
use crate::queue::Queue;
use crate::traits::{ListItem, ViewExt};
use rspotify::model::show::SimplifiedEpisode;
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
}

impl Episode {
    pub fn duration_str(&self) -> String {
        let minutes = self.duration / 60_000;
        let seconds = (self.duration / 1000) % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }
}

impl From<&SimplifiedEpisode> for Episode {
    fn from(episode: &SimplifiedEpisode) -> Self {
        Self {
            id: episode.id.clone(),
            uri: episode.uri.clone(),
            duration: episode.duration_ms,
            name: episode.name.clone(),
            description: episode.description.clone(),
            release_date: episode.release_date.clone(),
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
        false
    }

    fn display_left(&self) -> String {
        self.name.clone()
    }

    fn display_right(&self, library: Arc<Library>) -> String {
        format!("{} [{}]", self.duration_str(), self.release_date)
    }

    fn play(&mut self, queue: Arc<Queue>) {
        let index = queue.append_next(vec![Playable::Episode(self.clone())]);
        queue.play(index, true, false);
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        queue.append(Playable::Episode(self.clone()));
    }

    fn toggle_saved(&mut self, library: Arc<Library>) {
        unimplemented!()
    }

    fn save(&mut self, library: Arc<Library>) {
        unimplemented!()
    }

    fn unsave(&mut self, library: Arc<Library>) {
        unimplemented!()
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        None
    }

    fn share_url(&self) -> Option<String> {
        Some(format!("https://open.spotify.com/episode/{}", self.id))
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
