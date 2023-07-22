use chrono::{DateTime, Utc};
use rspotify::model::PlayableItem;

use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::episode::Episode;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::traits::{ListItem, ViewExt};
use crate::utils::ms_to_hms;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum Playable {
    Track(Track),
    Episode(Episode),
}

impl Playable {
    pub fn format(playable: &Playable, formatting: &str, library: &Library) -> String {
        formatting
            .replace(
                "%artists",
                if let Some(artists) = playable.artists() {
                    artists
                        .iter()
                        .map(|artist| artist.clone().name)
                        .collect::<Vec<String>>()
                        .join(", ")
                } else {
                    String::new()
                }
                .as_str(),
            )
            .replace(
                "%title",
                match playable.clone() {
                    Playable::Episode(episode) => episode.name,
                    Playable::Track(track) => track.title,
                }
                .as_str(),
            )
            .replace(
                "%album",
                match playable.clone() {
                    Playable::Track(track) => track.album.unwrap_or_default(),
                    _ => String::new(),
                }
                .as_str(),
            )
            .replace(
                "%saved",
                if library.is_saved_track(&match playable.clone() {
                    Playable::Episode(episode) => Playable::Episode(episode),
                    Playable::Track(track) => Playable::Track(track),
                }) {
                    if library.cfg.values().use_nerdfont.unwrap_or_default() {
                        "\u{f012c}"
                    } else {
                        "✓"
                    }
                } else {
                    ""
                },
            )
            .replace("%duration", playable.duration_str().as_str())
    }

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
        ms_to_hms(self.duration())
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

impl From<&Playable> for Option<rspotify::prelude::PlayableId<'_>> {
    fn from(p: &Playable) -> Self {
        match p {
            Playable::Track(t) => {
                t.id.clone()
                    .and_then(|id| rspotify::model::TrackId::from_id(id).ok())
                    .map(rspotify::prelude::PlayableId::Track)
            }
            Playable::Episode(e) => rspotify::model::EpisodeId::from_id(e.id.clone())
                .map(rspotify::prelude::PlayableId::Episode)
                .ok(),
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
    fn is_playing(&self, queue: &Queue) -> bool {
        self.as_listitem().is_playing(queue)
    }

    fn display_left(&self, library: &Library) -> String {
        self.as_listitem().display_left(library)
    }

    fn display_center(&self, library: &Library) -> String {
        self.as_listitem().display_center(library)
    }

    fn display_right(&self, library: &Library) -> String {
        self.as_listitem().display_right(library)
    }

    fn play(&mut self, queue: &Queue) {
        self.as_listitem().play(queue)
    }

    fn play_next(&mut self, queue: &Queue) {
        self.as_listitem().play_next(queue)
    }

    fn queue(&mut self, queue: &Queue) {
        self.as_listitem().queue(queue)
    }

    fn toggle_saved(&mut self, library: &Library) {
        self.as_listitem().toggle_saved(library)
    }

    fn save(&mut self, library: &Library) {
        self.as_listitem().save(library)
    }

    fn unsave(&mut self, library: &Library) {
        self.as_listitem().unsave(library)
    }

    fn open(&self, queue: Arc<Queue>, library: Arc<Library>) -> Option<Box<dyn ViewExt>> {
        self.as_listitem().open(queue, library)
    }

    fn share_url(&self) -> Option<String> {
        self.as_listitem().share_url()
    }

    fn album(&self, queue: &Queue) -> Option<Album> {
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
