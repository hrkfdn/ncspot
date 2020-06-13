use crate::library::Library;
use crate::queue::{Playable, Queue};
use crate::traits::{ListItem, ViewExt};
use rspotify::model::show::SimplifiedEpisode;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Episode {
    pub id: String,
    pub name: String,
    pub description: String,
    pub release_date: String,
}

impl From<&SimplifiedEpisode> for Episode {
    fn from(episode: &SimplifiedEpisode) -> Self {
        Self {
            id: episode.id.clone(),
            name: episode.name.clone(),
            description: episode.description.clone(),
            release_date: episode.release_date.clone(),
        }
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
        self.release_date.clone()
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
        unimplemented!()
    }

    fn share_url(&self) -> Option<String> {
        unimplemented!()
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
