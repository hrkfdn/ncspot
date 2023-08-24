use std::sync::Arc;

use log::debug;

use crate::{config::Config, model::track::Track};

pub trait LyricsFetcher {
    fn fetch(&self, track: &Track) -> String;
}

pub struct MusixMatchLyricsFetcher {
    api_key: String,
}

impl LyricsFetcher for MusixMatchLyricsFetcher {
    fn fetch(&self, track: &Track) -> String {
        let track_title = track.title.clone();
        let track_authors = track.artists.join(", ");

        debug!("Fetching lyrics for {} by {}", track_title, track_authors);

        let client = reqwest::blocking::Client::new();

        let response = client
            .get("https://api.musixmatch.com/ws/1.1/matcher.lyrics.get")
            .query(&[
                ("q_track", track_title.clone()),
                ("q_artist", track_authors),
                ("apikey", self.api_key.clone()),
            ])
            .send()
            .unwrap();

        if response.status() != 200 {
            debug!("Error fetching lyrics for {}", track_title);
            return format!("Error fetching lyrics for {}", track_title);
        }

        // Do this since we do not have a specific body type to parse into
        let text = response.text().unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();

        debug!("Received {:?}", json);

        if json["status_code"] != 200 {
            debug!("Error fetching lyrics for {}", track_title);
            return format!("Error fetching lyrics for {}", track_title);
        }

        json["message"]["body"]["lyrics"]["lyrics_body"].to_string()
    }
}

/// Create a default lyrics fetcher.
pub fn default_fetcher(cfg: Arc<Config>) -> Box<dyn LyricsFetcher> {
    Box::new(MusixMatchLyricsFetcher {
        api_key: cfg.values().backend.clone().unwrap_or_default(),
    })
}
