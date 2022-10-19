use crate::command::Command;
use crate::commands::CommandResult;
use crate::events::EventManager;
use crate::library::Library;
use crate::model::album::Album;
use crate::model::artist::Artist;
use crate::model::episode::Episode;
use crate::model::playlist::Playlist;
use crate::model::show::Show;
use crate::model::track::Track;
use crate::queue::Queue;
use crate::spotify::{Spotify, UriType};
use crate::spotify_url::SpotifyUrl;
use crate::traits::{ListItem, ViewExt};
use crate::ui::listview::ListView;
use crate::ui::pagination::Pagination;
use crate::ui::tabview::TabView;
use cursive::view::ViewWrapper;
use cursive::Cursive;
use rspotify::model::search::SearchResult;
use rspotify::model::SearchType;
use std::sync::{Arc, RwLock};

pub struct SearchResultsView {
    search_term: String,
    results_tracks: Arc<RwLock<Vec<Track>>>,
    pagination_tracks: Pagination<Track>,
    results_albums: Arc<RwLock<Vec<Album>>>,
    pagination_albums: Pagination<Album>,
    results_artists: Arc<RwLock<Vec<Artist>>>,
    pagination_artists: Pagination<Artist>,
    results_playlists: Arc<RwLock<Vec<Playlist>>>,
    pagination_playlists: Pagination<Playlist>,
    results_shows: Arc<RwLock<Vec<Show>>>,
    pagination_shows: Pagination<Show>,
    results_episodes: Arc<RwLock<Vec<Episode>>>,
    pagination_episodes: Pagination<Episode>,
    tabs: TabView,
    spotify: Spotify,
    events: EventManager,
}

type SearchHandler<I> =
    Box<dyn Fn(&Spotify, &Arc<RwLock<Vec<I>>>, &str, usize, bool) -> u32 + Send + Sync>;

impl SearchResultsView {
    pub fn new(
        search_term: String,
        events: EventManager,
        queue: Arc<Queue>,
        library: Arc<Library>,
    ) -> SearchResultsView {
        let results_tracks = Arc::new(RwLock::new(Vec::new()));
        let results_albums = Arc::new(RwLock::new(Vec::new()));
        let results_artists = Arc::new(RwLock::new(Vec::new()));
        let results_playlists = Arc::new(RwLock::new(Vec::new()));
        let results_shows = Arc::new(RwLock::new(Vec::new()));
        let results_episodes = Arc::new(RwLock::new(Vec::new()));

        let list_tracks = ListView::new(results_tracks.clone(), queue.clone(), library.clone());
        let pagination_tracks = list_tracks.get_pagination().clone();
        let list_albums = ListView::new(results_albums.clone(), queue.clone(), library.clone());
        let pagination_albums = list_albums.get_pagination().clone();
        let list_artists = ListView::new(results_artists.clone(), queue.clone(), library.clone());
        let pagination_artists = list_artists.get_pagination().clone();
        let list_playlists =
            ListView::new(results_playlists.clone(), queue.clone(), library.clone());
        let pagination_playlists = list_playlists.get_pagination().clone();
        let list_shows = ListView::new(results_shows.clone(), queue.clone(), library.clone());
        let pagination_shows = list_shows.get_pagination().clone();
        let list_episodes = ListView::new(results_episodes.clone(), queue.clone(), library);
        let pagination_episodes = list_episodes.get_pagination().clone();

        let tabs = TabView::new()
            .tab("tracks", list_tracks.with_title("Tracks"))
            .tab("albums", list_albums.with_title("Albums"))
            .tab("artists", list_artists.with_title("Artists"))
            .tab("playlists", list_playlists.with_title("Playlists"))
            .tab("shows", list_shows.with_title("Podcasts"))
            .tab("episodes", list_episodes.with_title("Podcast Episodes"));

        let mut view = SearchResultsView {
            search_term,
            results_tracks,
            pagination_tracks,
            results_albums,
            pagination_albums,
            results_artists,
            pagination_artists,
            results_playlists,
            pagination_playlists,
            results_shows,
            pagination_shows,
            results_episodes,
            pagination_episodes,
            tabs,
            spotify: queue.get_spotify(),
            events,
        };

        view.run_search();
        view
    }

    fn get_track(
        spotify: &Spotify,
        tracks: &Arc<RwLock<Vec<Track>>>,
        query: &str,
        _offset: usize,
        _append: bool,
    ) -> u32 {
        if let Some(results) = spotify.api.track(query) {
            let t = vec![(&results).into()];
            let mut r = tracks.write().unwrap();
            *r = t;
            return 1;
        }
        0
    }

    fn search_track(
        spotify: &Spotify,
        tracks: &Arc<RwLock<Vec<Track>>>,
        query: &str,
        offset: usize,
        append: bool,
    ) -> u32 {
        if let Some(SearchResult::Tracks(results)) =
            spotify
                .api
                .search(SearchType::Track, query, 50, offset as u32)
        {
            let mut t = results.items.iter().map(|ft| ft.into()).collect();
            let mut r = tracks.write().unwrap();

            if append {
                r.append(&mut t);
            } else {
                *r = t;
            }
            return results.total;
        }
        0
    }

    fn get_album(
        spotify: &Spotify,
        albums: &Arc<RwLock<Vec<Album>>>,
        query: &str,
        _offset: usize,
        _append: bool,
    ) -> u32 {
        if let Some(results) = spotify.api.album(query) {
            let a = vec![(&results).into()];
            let mut r = albums.write().unwrap();
            *r = a;
            return 1;
        }
        0
    }

    fn search_album(
        spotify: &Spotify,
        albums: &Arc<RwLock<Vec<Album>>>,
        query: &str,
        offset: usize,
        append: bool,
    ) -> u32 {
        if let Some(SearchResult::Albums(results)) =
            spotify
                .api
                .search(SearchType::Album, query, 50, offset as u32)
        {
            let mut a = results.items.iter().map(|sa| sa.into()).collect();
            let mut r = albums.write().unwrap();

            if append {
                r.append(&mut a);
            } else {
                *r = a;
            }
            return results.total;
        }
        0
    }

    fn get_artist(
        spotify: &Spotify,
        artists: &Arc<RwLock<Vec<Artist>>>,
        query: &str,
        _offset: usize,
        _append: bool,
    ) -> u32 {
        if let Some(results) = spotify.api.artist(query) {
            let a = vec![(&results).into()];
            let mut r = artists.write().unwrap();
            *r = a;
            return 1;
        }
        0
    }

    fn search_artist(
        spotify: &Spotify,
        artists: &Arc<RwLock<Vec<Artist>>>,
        query: &str,
        offset: usize,
        append: bool,
    ) -> u32 {
        if let Some(SearchResult::Artists(results)) =
            spotify
                .api
                .search(SearchType::Artist, query, 50, offset as u32)
        {
            let mut a = results.items.iter().map(|fa| fa.into()).collect();
            let mut r = artists.write().unwrap();

            if append {
                r.append(&mut a);
            } else {
                *r = a;
            }
            return results.total;
        }
        0
    }

    fn get_playlist(
        spotify: &Spotify,
        playlists: &Arc<RwLock<Vec<Playlist>>>,
        query: &str,
        _offset: usize,
        _append: bool,
    ) -> u32 {
        if let Some(result) = spotify.api.playlist(query).as_ref() {
            let pls = vec![result.into()];
            let mut r = playlists.write().unwrap();
            *r = pls;
            return 1;
        }
        0
    }

    fn search_playlist(
        spotify: &Spotify,
        playlists: &Arc<RwLock<Vec<Playlist>>>,
        query: &str,
        offset: usize,
        append: bool,
    ) -> u32 {
        if let Some(SearchResult::Playlists(results)) =
            spotify
                .api
                .search(SearchType::Playlist, query, 50, offset as u32)
        {
            let mut pls = results.items.iter().map(|sp| sp.into()).collect();
            let mut r = playlists.write().unwrap();

            if append {
                r.append(&mut pls);
            } else {
                *r = pls;
            }
            return results.total;
        }
        0
    }

    fn get_show(
        spotify: &Spotify,
        shows: &Arc<RwLock<Vec<Show>>>,
        query: &str,
        _offset: usize,
        _append: bool,
    ) -> u32 {
        if let Some(result) = spotify.api.get_show(query).as_ref() {
            let pls = vec![result.into()];
            let mut r = shows.write().unwrap();
            *r = pls;
            return 1;
        }
        0
    }

    fn search_show(
        spotify: &Spotify,
        shows: &Arc<RwLock<Vec<Show>>>,
        query: &str,
        offset: usize,
        append: bool,
    ) -> u32 {
        if let Some(SearchResult::Shows(results)) =
            spotify
                .api
                .search(SearchType::Show, query, 50, offset as u32)
        {
            let mut pls = results.items.iter().map(|sp| sp.into()).collect();
            let mut r = shows.write().unwrap();

            if append {
                r.append(&mut pls);
            } else {
                *r = pls;
            }
            return results.total;
        }
        0
    }

    fn get_episode(
        spotify: &Spotify,
        episodes: &Arc<RwLock<Vec<Episode>>>,
        query: &str,
        _offset: usize,
        _append: bool,
    ) -> u32 {
        if let Some(result) = spotify.api.episode(query).as_ref() {
            let e = vec![result.into()];
            let mut r = episodes.write().unwrap();
            *r = e;
            return 1;
        }
        0
    }

    fn search_episode(
        spotify: &Spotify,
        episodes: &Arc<RwLock<Vec<Episode>>>,
        query: &str,
        offset: usize,
        append: bool,
    ) -> u32 {
        if let Some(SearchResult::Episodes(results)) =
            spotify
                .api
                .search(SearchType::Episode, query, 50, offset as u32)
        {
            let mut e = results.items.iter().map(|se| se.into()).collect();
            let mut r = episodes.write().unwrap();

            if append {
                r.append(&mut e);
            } else {
                *r = e;
            }
            return results.total;
        }
        0
    }

    fn perform_search<I: ListItem + Clone>(
        &self,
        handler: SearchHandler<I>,
        results: &Arc<RwLock<Vec<I>>>,
        query: &str,
        paginator: Option<&Pagination<I>>,
    ) {
        let spotify = self.spotify.clone();
        let query = query.to_owned();
        let results = results.clone();
        let ev = self.events.clone();
        let paginator = paginator.cloned();

        std::thread::spawn(move || {
            let total_items = handler(&spotify, &results, &query, 0, false) as usize;

            // register paginator if the API has more than one page of results
            if let Some(mut paginator) = paginator {
                let loaded_items = results.read().unwrap().len();
                if total_items > loaded_items {
                    let ev = ev.clone();

                    // paginator callback
                    let cb = move |items: Arc<RwLock<Vec<I>>>| {
                        let offset = items.read().unwrap().len();
                        handler(&spotify, &results, &query, offset, true);
                        ev.trigger();
                    };
                    paginator.set(loaded_items, total_items, Box::new(cb));
                } else {
                    paginator.clear()
                }
            }
            ev.trigger();
        });
    }

    pub fn run_search(&mut self) {
        let query = self.search_term.clone();

        // check if API token refresh is necessary before commencing multiple
        // requests to avoid deadlock, as the parallel requests might
        // simultaneously try to refresh the token
        self.spotify.api.update_token();

        // is the query a Spotify URI?
        if let Some(uritype) = UriType::from_uri(&query) {
            match uritype {
                UriType::Track => {
                    self.perform_search(
                        Box::new(Self::get_track),
                        &self.results_tracks,
                        &query,
                        None,
                    );
                    self.tabs.move_focus_to(0);
                }
                UriType::Album => {
                    self.perform_search(
                        Box::new(Self::get_album),
                        &self.results_albums,
                        &query,
                        None,
                    );
                    self.tabs.move_focus_to(1);
                }
                UriType::Artist => {
                    self.perform_search(
                        Box::new(Self::get_artist),
                        &self.results_artists,
                        &query,
                        None,
                    );
                    self.tabs.move_focus_to(2);
                }
                UriType::Playlist => {
                    self.perform_search(
                        Box::new(Self::get_playlist),
                        &self.results_playlists,
                        &query,
                        None,
                    );
                    self.tabs.move_focus_to(3);
                }
                UriType::Show => {
                    self.perform_search(
                        Box::new(Self::get_show),
                        &self.results_shows,
                        &query,
                        None,
                    );
                    self.tabs.move_focus_to(4);
                }
                UriType::Episode => {
                    self.perform_search(
                        Box::new(Self::get_episode),
                        &self.results_episodes,
                        &query,
                        None,
                    );
                    self.tabs.move_focus_to(5);
                }
            }
        // Is the query a spotify URL?
        // https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC
        } else if let Some(url) = SpotifyUrl::from_url(&query) {
            match url.uri_type {
                UriType::Track => {
                    self.perform_search(
                        Box::new(Self::get_track),
                        &self.results_tracks,
                        &url.id,
                        None,
                    );
                    self.tabs.move_focus_to(0);
                }
                UriType::Album => {
                    self.perform_search(
                        Box::new(Self::get_album),
                        &self.results_albums,
                        &url.id,
                        None,
                    );
                    self.tabs.move_focus_to(1);
                }
                UriType::Artist => {
                    self.perform_search(
                        Box::new(Self::get_artist),
                        &self.results_artists,
                        &url.id,
                        None,
                    );
                    self.tabs.move_focus_to(2);
                }
                UriType::Playlist => {
                    self.perform_search(
                        Box::new(Self::get_playlist),
                        &self.results_playlists,
                        &url.id,
                        None,
                    );
                    self.tabs.move_focus_to(3);
                }
                UriType::Show => {
                    self.perform_search(
                        Box::new(Self::get_show),
                        &self.results_shows,
                        &url.id,
                        None,
                    );
                    self.tabs.move_focus_to(4);
                }
                UriType::Episode => {
                    self.perform_search(
                        Box::new(Self::get_episode),
                        &self.results_episodes,
                        &url.id,
                        None,
                    );
                    self.tabs.move_focus_to(5);
                }
            }
        } else {
            self.perform_search(
                Box::new(Self::search_track),
                &self.results_tracks,
                &query,
                Some(&self.pagination_tracks),
            );
            self.perform_search(
                Box::new(Self::search_album),
                &self.results_albums,
                &query,
                Some(&self.pagination_albums),
            );
            self.perform_search(
                Box::new(Self::search_artist),
                &self.results_artists,
                &query,
                Some(&self.pagination_artists),
            );
            self.perform_search(
                Box::new(Self::search_playlist),
                &self.results_playlists,
                &query,
                Some(&self.pagination_playlists),
            );
            self.perform_search(
                Box::new(Self::search_show),
                &self.results_shows,
                &query,
                Some(&self.pagination_shows),
            );
            self.perform_search(
                Box::new(Self::search_episode),
                &self.results_episodes,
                &query,
                Some(&self.pagination_episodes),
            );
        }
    }
}

impl ViewWrapper for SearchResultsView {
    wrap_impl!(self.tabs: TabView);
}

impl ViewExt for SearchResultsView {
    fn title(&self) -> String {
        format!("Search: {}", self.search_term)
    }
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        self.tabs.on_command(s, cmd)
    }
}
