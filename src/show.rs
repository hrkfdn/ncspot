use crate::library::Library;
use crate::queue::Queue;
use crate::traits::{ListItem, ViewExt};
use rspotify::model::show::SimplifiedShow;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Deserialize, Serialize)]
pub struct Show {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub description: String,
}

impl From<&SimplifiedShow> for Show {
    fn from(show: &SimplifiedShow) -> Self {
        Self {
            id: show.id.clone(),
            name: show.name.clone(),
            publisher: show.publisher.clone(),
            description: show.description.clone(),
        }
    }
}

impl fmt::Display for Show {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} - {}", self.publisher, self.name)
    }
}

impl ListItem for Show {
    fn is_playing(&self, queue: Arc<Queue>) -> bool {
        false
    }

    fn display_left(&self) -> String {
        format!("{}", self)
    }

    fn display_right(&self, library: Arc<Library>) -> String {
        "".to_string()
    }

    fn play(&mut self, queue: Arc<Queue>) {
        unimplemented!()
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        unimplemented!()
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
