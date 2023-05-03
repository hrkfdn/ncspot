use crate::library::Library;
use crate::model::episode::Episode;
use crate::model::playable::Playable;
use crate::queue::Queue;
use crate::spotify::Spotify;
use crate::traits::{IntoBoxedViewExt, ListItem, ViewExt};
use crate::ui::show::ShowView;
use rspotify::model::show::{FullShow, SimplifiedShow};
use rspotify::model::Id;
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
    pub fn load_all_episodes(&mut self, spotify: Spotify) {
        if self.episodes.is_some() {
            return;
        }

        let episodes_result = spotify.api.show_episodes(&self.id);
        while !episodes_result.at_end() {
            episodes_result.next();
        }

        let episodes = episodes_result.items.read().unwrap().clone();
        self.episodes = Some(episodes);
    }
}

impl From<&SimplifiedShow> for Show {
    fn from(show: &SimplifiedShow) -> Self {
        Self {
            id: show.id.id().to_string(),
            uri: show.id.uri(),
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
            id: show.id.id().to_string(),
            uri: show.id.uri(),
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
    fn is_playing(&self, _queue: &Queue) -> bool {
        false
    }

    fn display_left(&self, _library: &Library) -> String {
        format!("{self}")
    }

    fn display_right(&self, library: &Library) -> String {
        let saved = if library.is_saved_show(self) {
            if library.cfg.values().use_nerdfont.unwrap_or(false) {
                "\u{f012c} "
            } else {
                "✓ "
            }
        } else {
            ""
        };
        saved.to_owned()
    }

    fn play(&mut self, queue: &Queue) {
        self.load_all_episodes(queue.get_spotify());

        let playables = self
            .episodes
            .as_ref()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|ep| Playable::Episode(ep.clone()))
            .collect();

        let index = queue.append_next(&playables);
        queue.play(index, true, true);
    }

    fn play_next(&mut self, queue: &Queue) {
        self.load_all_episodes(queue.get_spotify());

        if let Some(episodes) = self.episodes.as_ref() {
            for ep in episodes.iter().rev() {
                queue.insert_after_current(Playable::Episode(ep.clone()));
            }
        }
    }

    fn queue(&mut self, queue: &Queue) {
        self.load_all_episodes(queue.get_spotify());

        for ep in self.episodes.as_ref().unwrap_or(&Vec::new()) {
            queue.append(Playable::Episode(ep.clone()));
        }
    }

    fn toggle_saved(&mut self, library: &Library) {
        if library.is_saved_show(self) {
            self.unsave(library);
        } else {
            self.save(library);
        }
    }

    fn save(&mut self, library: &Library) {
        library.save_show(self);
    }

    fn unsave(&mut self, library: &Library) {
        library.unsave_show(self);
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        Some(ShowView::new(queue, library, self).into_boxed_view_ext())
    }

    fn share_url(&self) -> Option<String> {
        Some(format!("https://open.spotify.com/show/{}", self.id))
    }

    #[inline]
    fn is_saved(&self, library: &Library) -> Option<bool> {
        Some(library.is_saved_show(self))
    }

    #[inline]
    fn is_playable(&self) -> bool {
        true
    }

    fn as_listitem(&self) -> Box<dyn ListItem> {
        Box::new(self.clone())
    }
}
