use std::sync::Arc;

use log::debug;

use crate::{config::Config, model::track::Track};
use urlencoding::encode;
pub trait LyricsFetcher {
    fn fetch(&self, track: &Track) -> String;
}

pub struct OVHLyricsFetcher;

impl LyricsFetcher for OVHLyricsFetcher {
    fn fetch(&self, track: &Track) -> String {
        let track_title = track.title.clone();
        let track_authors = track.artists.join(", ");

        debug!("Fetching lyrics for {} by {}", track_title, track_authors);

        let client = reqwest::blocking::Client::new();

        let endpoint = reqwest::Url::parse(
            format!(
                "https://api.lyrics.ovh/v1/{}/{}",
                encode(track.artists[0].as_str()).into_owned(),
                encode(track_title.as_str()).into_owned()
            )
            .as_str(),
        )
        .unwrap();

        // TODO: probably should not be blocking
        let response = client.get(endpoint).send().unwrap();

        if response.status() != 200 {
            debug!(
                "Error fetching lyrics for {}: {}",
                track_title,
                response.status()
            );
            return format!("Error fetching lyrics for {}", track_title);
        }

        // Do this since we do not have a specific body type to parse into
        let text = response.text().unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();

        debug!("Received {:?}", json);

        json["lyrics"].to_string()
    }
}

/// Create a default lyrics fetcher.
pub fn default_fetcher(cfg: Arc<Config>) -> Box<dyn LyricsFetcher> {
    Box::new(OVHLyricsFetcher {})
}
