#![allow(unused_imports)]

use cursive::direction::Orientation;
use cursive::event::{AnyCb, Event, EventResult, Key};
use cursive::traits::{Boxable, Finder, Identifiable, View};
use cursive::view::{Selector, ViewWrapper};
use cursive::views::{EditView, IdView, ViewRef};
use cursive::{Cursive, Printer, Vec2};
use std::cell::RefCell;
use std::sync::{Arc, Mutex, RwLock};

use album::Album;
use artist::Artist;
use commands::CommandResult;
use events::EventManager;
use playlists::{Playlist, Playlists};
use queue::Queue;
use spotify::Spotify;
use track::Track;
use traits::ViewExt;
use ui::listview::ListView;
use ui::tabview::TabView;

pub struct SearchView {
    results_tracks: Arc<RwLock<Vec<Track>>>,
    results_albums: Arc<RwLock<Vec<Album>>>,
    results_artists: Arc<RwLock<Vec<Artist>>>,
    results_playlists: Arc<RwLock<Vec<Playlist>>>,
    edit: IdView<EditView>,
    list: IdView<TabView>,
    edit_focused: bool,
    events: EventManager,
    spotify: Arc<Spotify>,
}

pub const LIST_ID: &str = "search_list";
pub const EDIT_ID: &str = "search_edit";
impl SearchView {
    pub fn new(events: EventManager, spotify: Arc<Spotify>, queue: Arc<Queue>) -> SearchView {
        let results_tracks = Arc::new(RwLock::new(Vec::new()));
        let results_albums = Arc::new(RwLock::new(Vec::new()));
        let results_artists = Arc::new(RwLock::new(Vec::new()));
        let results_playlists = Arc::new(RwLock::new(Vec::new()));

        let searchfield = EditView::new()
            .on_submit(move |s, input| {
                if !input.is_empty() {
                    s.call_on_id("search", |v: &mut SearchView| {
                        v.run_search(input);
                        v.focus_view(&Selector::Id(LIST_ID)).unwrap();
                    });
                }
            })
            .with_id(EDIT_ID);

        let tabs = TabView::new()
            .tab(
                "tracks",
                "Tracks",
                ListView::new(results_tracks.clone(), queue.clone()),
            )
            .tab(
                "albums",
                "Albums",
                ListView::new(results_albums.clone(), queue.clone()),
            )
            .tab(
                "artists",
                "Artists",
                ListView::new(results_artists.clone(), queue.clone()),
            )
            .tab(
                "playlists",
                "Playlists",
                ListView::new(results_playlists.clone(), queue.clone()),
            );

        SearchView {
            results_tracks,
            results_albums,
            results_artists,
            results_playlists,
            edit: searchfield,
            list: tabs.with_id(LIST_ID),
            edit_focused: true,
            events,
            spotify,
        }
    }

    pub fn clear(&mut self) {
        self.edit
            .call_on(&Selector::Id(EDIT_ID), |v: &mut EditView| {
                v.set_content("");
            });
    }

    fn search_track(spotify: Arc<Spotify>, tracks: Arc<RwLock<Vec<Track>>>, query: String) {
        if let Some(results) = spotify.search_track(&query, 50, 0) {
            let t = results.tracks.items.iter().map(|ft| ft.into()).collect();
            let mut r = tracks.write().unwrap();
            *r = t;
        }
    }

    fn search_album(spotify: Arc<Spotify>, albums: Arc<RwLock<Vec<Album>>>, query: String) {
        if let Some(results) = spotify.search_album(&query, 50, 0) {
            let a = results.albums.items.iter().map(|sa| sa.into()).collect();
            let mut r = albums.write().unwrap();
            *r = a;
        }
    }

    fn search_artist(spotify: Arc<Spotify>, artists: Arc<RwLock<Vec<Artist>>>, query: String) {
        if let Some(results) = spotify.search_artist(&query, 50, 0) {
            let a = results.artists.items.iter().map(|fa| fa.into()).collect();
            let mut r = artists.write().unwrap();
            *r = a;
        }
    }

    fn search_playlist(
        spotify: Arc<Spotify>,
        playlists: Arc<RwLock<Vec<Playlist>>>,
        query: String,
    ) {
        if let Some(results) = spotify.search_playlist(&query, 50, 0) {
            let pls = results
                .playlists
                .items
                .iter()
                .map(|sp| Playlists::process_simplified_playlist(sp, &&spotify))
                .collect();
            let mut r = playlists.write().unwrap();
            *r = pls;
        }
    }

    pub fn run_search<S: Into<String>>(&mut self, query: S) {
        let query = query.into();

        self.edit_focused = false;

        {
            let query = query.clone();
            self.edit
                .call_on(&Selector::Id(EDIT_ID), |v: &mut EditView| {
                    v.set_content(query);
                });
        }

        self.spotify.refresh_token();

        {
            let ev = self.events.clone();
            let spotify = self.spotify.clone();
            let results = self.results_tracks.clone();
            let query = query.clone();
            std::thread::spawn(move || {
                Self::search_track(spotify, results, query);
                ev.trigger();
            });
        }

        {
            let ev = self.events.clone();
            let spotify = self.spotify.clone();
            let results = self.results_albums.clone();
            let query = query.clone();
            std::thread::spawn(move || {
                Self::search_album(spotify, results, query);
                ev.trigger();
            });
        }

        {
            let ev = self.events.clone();
            let spotify = self.spotify.clone();
            let results = self.results_artists.clone();
            let query = query.clone();
            std::thread::spawn(move || {
                Self::search_artist(spotify, results, query);
                ev.trigger();
            });
        }

        {
            let ev = self.events.clone();
            let spotify = self.spotify.clone();
            let results = self.results_playlists.clone();
            let query = query.clone();
            std::thread::spawn(move || {
                Self::search_playlist(spotify, results, query);
                ev.trigger();
            });
        }
    }
}

impl View for SearchView {
    fn draw(&self, printer: &Printer<'_, '_>) {
        {
            let printer = &printer
                .offset((0, 0))
                .cropped((printer.size.x, 1))
                .focused(self.edit_focused);
            self.edit.draw(printer);
        }

        let printer = &printer
            .offset((0, 1))
            .cropped((printer.size.x, printer.size.y - 1))
            .focused(!self.edit_focused);
        self.list.draw(printer);
    }

    fn layout(&mut self, size: Vec2) {
        self.edit.layout(Vec2::new(size.x, 1));
        self.list.layout(Vec2::new(size.x, size.y - 1));
    }

    fn call_on_any<'a>(&mut self, selector: &Selector<'_>, mut callback: AnyCb<'a>) {
        self.edit.call_on_any(selector, Box::new(|v| callback(v)));
        self.list.call_on_any(selector, Box::new(|v| callback(v)));
    }

    fn focus_view(&mut self, selector: &Selector<'_>) -> Result<(), ()> {
        if let Selector::Id(s) = selector {
            self.edit_focused = s == &"search_edit";
            Ok(())
        } else {
            Err(())
        }
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        if self.edit_focused {
            self.edit.on_event(event)
        } else {
            self.list.on_event(event)
        }
    }
}

impl ViewExt for SearchView {
    fn on_command(
        &mut self,
        s: &mut Cursive,
        cmd: &str,
        args: &[String],
    ) -> Result<CommandResult, String> {
        if cmd == "search" && !args.is_empty() {
            self.run_search(args.join(" "));
            return Ok(CommandResult::Consumed(None));
        }

        if cmd == "focus" {
            self.edit_focused = true;
            self.clear();
            return Ok(CommandResult::Consumed(None));
        }

        let result = if !self.edit_focused {
            self.list.on_command(s, cmd, args)?
        } else {
            CommandResult::Ignored
        };

        if result == CommandResult::Ignored && cmd == "move" {
            if let Some(dir) = args.get(0) {
                if dir == "up" && !self.edit_focused {
                    self.edit_focused = true;
                    return Ok(CommandResult::Consumed(None));
                }

                if dir == "down" && self.edit_focused {
                    self.edit_focused = false;
                    return Ok(CommandResult::Consumed(None));
                }
            }
        }

        Ok(result)
    }
}
