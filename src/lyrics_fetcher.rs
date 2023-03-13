use crate::model::track::Track;

#[derive(Clone)]
pub struct LyricsFetcher {}

impl LyricsFetcher {
    pub fn new() -> LyricsFetcher {
        Self {}
    }

    /// Fetches the lyrics of the given song using the specified lyrics source
    pub fn fetch(&self, track: &Track) -> String {
        // std::thread::sleep(std::time::Duration::from_secs(2));

        format!("Sample Lyrics for {}\n", track.title)
    }
}

impl Default for LyricsFetcher {
    fn default() -> Self {
        LyricsFetcher::new() // TODO: check the prefered fetcher
    }
}
