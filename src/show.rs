use crate::episode::Episode;
use crate::library::Library;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::show::ShowView;
use rspotify::model::show::SimplifiedShow;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Deserialize, Serialize)]
pub struct Show {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub description: String,
    pub cover_url: Option<String>,
    pub episodes: Option<Vec<Episode>>,
}

impl Show {
    pub fn load_episodes(&mut self, spotify: Arc<Spotify>) {
        let mut collected_episodes = Vec::new();

        let mut episodes_result = spotify.show_episodes(&self.id, 50, 0);
        while let Some(ref episodes) = episodes_result.clone() {
            for item in &episodes.items {
                collected_episodes.push(item.into())
            }
            debug!("got {} episodes", episodes.items.len());

            // load next batch if necessary
            episodes_result = match episodes.next {
                Some(_) => {
                    debug!("requesting episodes again..");
                    spotify.show_episodes(
                        &self.id,
                        50,
                        episodes.offset + episodes.items.len() as u32,
                    )
                }
                None => None,
            }
        }

        self.episodes = Some(collected_episodes);
    }
}

impl From<&SimplifiedShow> for Show {
    fn from(show: &SimplifiedShow) -> Self {
        Self {
            id: show.id.clone(),
            name: show.name.clone(),
            publisher: show.publisher.clone(),
            description: show.description.clone(),
            cover_url: show.images.get(0).map(|i| i.url.clone()),
            episodes: None,
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
        self.episodes
            .as_ref()
            .map(|eps| format!("{} episodes", eps.len()))
            .unwrap_or_default()
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
        Some(ShowView::new(queue, library, self).as_boxed_view_ext())
    }

    fn share_url(&self) -> Option<String> {
        unimplemented!()
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}