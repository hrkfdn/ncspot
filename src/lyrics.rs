use log::trace;
use std::{cell::RefCell, collections::HashMap, sync::Arc};

use crate::{
    lyrics_fetcher::LyricsFetcher,
    model::{playable::Playable, track::Track},
    queue::Queue,
};

#[derive(Clone)]
pub struct LyricsManager {
    queue: Arc<Queue>,
    fetcher: LyricsFetcher,
    cache: RefCell<HashMap<String, String>>,
}

impl LyricsManager {
    pub fn new(queue: Arc<Queue>, fetcher: LyricsFetcher) -> Self {
        LyricsManager {
            queue,
            fetcher,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Saves the given lyrics to the user's filesystem.
    ///
    /// Returns an optional message indicating the outcome of this operation.
    pub fn save_lyrics(&self, lyrics: String) -> Option<String> {
        Some(lyrics)
    }

    /// Fetches and returns the lyrics of the given track
    pub fn get_lyrics(&self, track: Track) -> String {
        // TODO: see if this panics later on
        let track_id = track.id.as_ref().unwrap();

        {
            // insert new scope so that we can perform both borrows from the RefCell
            // the immutable borrow present in this scope is dropped,
            // so it is safe to do another borrow after

            let cache = self.cache.borrow();

            if cache.contains_key(track_id) {
                trace!("Retrieving cached lyrics for {}", track.title);
                return cache.get(track_id).unwrap().to_owned();
            }
        }

        // if we reach this point it means that the cache does not contain this entry yet, update it
        let mut cache = self.cache.borrow_mut();

        // make network request to fetch track's lyrics
        let lyrics = self.fetcher.fetch(&track);

        cache.insert(track_id.to_owned(), lyrics.clone());

        lyrics
    }

    /// Fetches and returns the lyrics of the currently playing track
    pub fn get_lyrics_for_current(&self) -> String {
        match self.get_current_track() {
            None => String::from("No track currently playing: could not get lyrics"),
            Some(track) => self.get_lyrics(track),
        }
    }

    /// Returns the track being played currently, or nothing if the user is listening to a podcast episode
    pub fn get_current_track(&self) -> Option<Track> {
        let playable = self.queue.get_current().unwrap();

        match playable {
            Playable::Track(track) => Some(track),
            _ => None,
        }
    }
}
