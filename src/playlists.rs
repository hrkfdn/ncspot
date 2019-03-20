use std::iter::Iterator;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use rspotify::spotify::model::playlist::SimplifiedPlaylist;

use config;
use events::EventManager;
use queue::Queue;
use spotify::Spotify;
use track::Track;
use traits::ListItem;

#[derive(Clone, Deserialize, Serialize)]
pub struct Playlist {
    pub meta: SimplifiedPlaylist,
    pub tracks: Vec<Track>,
}

#[derive(Clone)]
pub struct Playlists {
    pub store: Arc<RwLock<Vec<Playlist>>>,
    ev: EventManager,
    spotify: Arc<Spotify>,
    cache_path: PathBuf,
}

impl ListItem for Playlist {
    fn is_playing(&self, queue: Arc<Queue>) -> bool {
        let playing: Vec<String> = queue
            .queue
            .read()
            .unwrap()
            .iter()
            .map(|t| t.id.clone())
            .collect();
        let ids: Vec<String> = self.tracks.iter().map(|t| t.id.clone()).collect();
        ids.len() > 0 && playing == ids
    }

    fn display_left(&self) -> String {
        self.meta.name.clone()
    }

    fn display_right(&self) -> String {
        format!("{} tracks", self.tracks.len())
    }
}

impl Playlists {
    pub fn new(ev: &EventManager, spotify: &Arc<Spotify>) -> Playlists {
        Playlists {
            store: Arc::new(RwLock::new(Vec::new())),
            ev: ev.clone(),
            spotify: spotify.clone(),
            cache_path: config::cache_path(),
        }
    }

    pub fn load_cache(&self) {
        if let Ok(contents) = std::fs::read_to_string(&self.cache_path) {
            debug!(
                "loading playlist cache from {}",
                self.cache_path.to_str().unwrap()
            );
            let parsed: Result<Vec<Playlist>, _> = serde_json::from_str(&contents);
            match parsed {
                Ok(cache) => {
                    debug!("playlist cache loaded ({} lists)", cache.len());
                    let mut store = self.store.write().expect("can't writelock playlist store");
                    store.clear();
                    store.extend(cache);

                    // force refresh of UI (if visible)
                    self.ev.trigger();
                }
                Err(e) => {
                    error!("can't parse playlist cache: {}", e);
                }
            }
        }
    }

    pub fn save_cache(&self) {
        match serde_json::to_string(&self.store.deref()) {
            Ok(contents) => std::fs::write(&self.cache_path, contents).unwrap(),
            Err(e) => error!("could not write playlist cache: {:?}", e),
        }
    }

    fn process_playlist(list: &SimplifiedPlaylist, spotify: &Spotify) -> Playlist {
        debug!("got list: {}", list.name);
        let id = list.id.clone();

        let mut collected_tracks = Vec::new();

        let mut tracks_result = spotify.user_playlist_tracks(&id, 100, 0);
        while let Some(ref tracks) = tracks_result.clone() {
            for listtrack in &tracks.items {
                collected_tracks.push(Track::new(&listtrack.track));
            }
            debug!("got {} tracks", tracks.items.len());

            // load next batch if necessary
            tracks_result = match tracks.next {
                Some(_) => {
                    debug!("requesting tracks again..");
                    spotify.user_playlist_tracks(
                        &id,
                        100,
                        tracks.offset + tracks.items.len() as u32,
                    )
                }
                None => None,
            }
        }
        Playlist {
            meta: list.clone(),
            tracks: collected_tracks,
        }
    }

    fn needs_download(&self, remote: &SimplifiedPlaylist) -> bool {
        for local in self.store.read().expect("can't readlock playlists").iter() {
            if local.meta.id == remote.id {
                return local.meta.snapshot_id != remote.snapshot_id;
            }
        }
        true
    }

    fn append_or_update(&self, updated: &Playlist) -> usize {
        let mut store = self.store.write().expect("can't writelock playlists");
        for (index, mut local) in store.iter_mut().enumerate() {
            if local.meta.id == updated.meta.id {
                *local = updated.clone();
                return index;
            }
        }
        store.push(updated.clone());
        store.len() - 1
    }

    pub fn fetch_playlists(&self) {
        debug!("loading playlists");
        let mut lists_result = self.spotify.current_user_playlist(50, 0);
        while let Some(ref lists) = lists_result.clone() {
            for remote in &lists.items {
                if self.needs_download(remote) {
                    info!("updating playlist {}", remote.name);
                    let playlist = Self::process_playlist(&remote, &self.spotify);
                    self.append_or_update(&playlist);
                    // trigger redraw
                    self.ev.trigger();
                }
            }

            // load next batch if necessary
            lists_result = match lists.next {
                Some(_) => {
                    debug!("requesting playlists again..");
                    self.spotify
                        .current_user_playlist(50, lists.offset + lists.items.len() as u32)
                }
                None => None,
            }
        }
    }
}
