use crate::episode::Episode;
use crate::library::Library;
use crate::playable::Playable;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::show::ShowView;
use rspotify::model::show::{FullShow, SimplifiedShow};
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Deserialize, Serialize)]
pub struct Show {
    pub id: String,
    pub uri: String,
    pub name: String,
    pub publisher: String,
    pub description: String,
    pub cover_url: Option<String>,
    pub episodes: Option<Vec<Episode>>,
}

impl Show {
    pub fn load_episodes(&mut self, spotify: Arc<Spotify>) {
        if self.episodes.is_some() {
            return;
        }

        let mut collected_episodes = Vec::new();

        let mut episodes_result = spotify.show_episodes(&self.id, 0);
        while let Some(ref episodes) = episodes_result.clone() {
            for item in &episodes.items {
                collected_episodes.push(item.into())
            }
            debug!("got {} episodes", episodes.items.len());

            // load next batch if necessary
            episodes_result = match episodes.next {
                Some(_) => {
                    debug!("requesting episodes again..");
                    spotify.show_episodes(&self.id, episodes.offset + episodes.items.len() as u32)
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
            uri: show.uri.clone(),
            name: show.name.clone(),
            publisher: show.publisher.clone(),
            description: show.description.clone(),
            cover_url: show.images.get(0).map(|i| i.url.clone()),
            episodes: None,
        }
    }
}

impl From<&FullShow> for Show {
    fn from(show: &FullShow) -> Self {
        Self {
            id: show.id.clone(),
            uri: show.uri.clone(),
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
    fn is_playing(&self, _queue: Arc<Queue>) -> bool {
        false
    }

    fn display_left(&self) -> String {
        format!("{}", self)
    }

    fn display_center(&self) -> String {
        "".to_string()
    }

    fn display_right(&self, library: Arc<Library>) -> String {
        let saved = if library.is_saved_show(self) {
            if library.use_nerdfont {
                "\u{f62b} "
            } else {
                "✓ "
            }
        } else {
            ""
        };
        saved.to_owned()
    }

    fn play(&mut self, queue: Arc<Queue>) {
        self.load_episodes(queue.get_spotify());

        let playables = self
            .episodes
            .as_ref()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|ep| Playable::Episode(ep.clone()))
            .collect();

        let index = queue.append_next(playables);
        queue.play(index, true, true);
    }

    fn play_next(&mut self, queue: Arc<Queue>) {
        self.load_episodes(queue.get_spotify());

        if let Some(episodes) = self.episodes.as_ref() {
            for ep in episodes.iter().rev() {
                queue.insert_after_current(Playable::Episode(ep.clone()));
            }
        }
    }

    fn queue(&mut self, queue: Arc<Queue>) {
        self.load_episodes(queue.get_spotify());

        for ep in self.episodes.as_ref().unwrap_or(&Vec::new()) {
            queue.append(Playable::Episode(ep.clone()));
        }
    }

    fn toggle_saved(&mut self, library: Arc<Library>) {
        if library.is_saved_show(self) {
            self.unsave(library);
        } else {
            self.save(library);
        }
    }

    fn save(&mut self, library: Arc<Library>) {
        library.save_show(self);
    }

    fn unsave(&mut self, library: Arc<Library>) {
        library.unsave_show(self);
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(ShowView::new(queue, library, self).as_boxed_view_ext())
    }

    fn share_url(&self) -> Option<String> {
        Some(format!("https://open.spotify.com/show/{}", self.id))
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
