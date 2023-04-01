use std::sync::Arc;

use crate::{
    lyrics_fetcher::LyricsFetcher,
    model::{playable::Playable, track::Track},
    queue::Queue,
};

#[derive(Clone)]
pub struct LyricsManager {
    queue: Arc<Queue>,
    fetcher: LyricsFetcher,
    // TODO: add a cache
}

impl LyricsManager {
    pub fn new(queue: Arc<Queue>, fetcher: LyricsFetcher) -> Self {
        LyricsManager { queue, fetcher }
    }

    /// Saves the given lyrics to the user's filesystem.
    ///
    /// Returns an optional message indicating the outcome of this operation.
    pub fn save_lyrics(&self, lyrics: String) -> Option<String> {
        Some("".to_string())
    }

    /// Fetches and returns the lyrics of the given track
    pub fn get_lyrics(&self, track: Track) -> String {
        // TODO: implement caching

        self.fetcher.fetch(&track)
    }

    /// Fetches and returns the lyrics of the currently playing track
    pub fn get_lyrics_for_current(&self) -> String {
        match self.get_current_track() {
            None => String::from("No track currently playing: could not get lyrics"),
            Some(track) => self.get_lyrics(track),
        }
    }

    /// Returns the track being played currently, or nothing if the user is listening to a podcast episodes
    pub fn get_current_track(&self) -> Option<Track> {
        let playable = self.queue.get_current().unwrap();

        match playable {
            Playable::Track(track) => Some(track),
            Playable::Episode(_) => None,
        }
    }
}
