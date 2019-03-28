#![allow(unused_imports)]

use cursive::direction::Orientation;
use cursive::event::{AnyCb, Event, EventResult, Key};
use cursive::traits::{Boxable, Finder, Identifiable, View};
use cursive::view::{Selector, ViewWrapper};
use cursive::views::{EditView, IdView, ViewRef};
use cursive::{Cursive, Printer, Vec2};
use std::cell::RefCell;
use std::sync::{Arc, Mutex, RwLock};

use commands::CommandResult;
use playlists::{Playlist, Playlists};
use queue::Queue;
use spotify::Spotify;
use track::Track;
use traits::ViewExt;
use ui::listview::ListView;
use ui::tabview::TabView;

pub struct SearchView {
    results_tracks: Arc<RwLock<Vec<Track>>>,
    results_playlists: Arc<RwLock<Vec<Playlist>>>,
    edit: IdView<EditView>,
    list: IdView<TabView>,
    edit_focused: bool,
    spotify: Arc<Spotify>,
}

pub const LIST_ID: &str = "search_list";
pub const EDIT_ID: &str = "search_edit";
impl SearchView {
    pub fn new(spotify: Arc<Spotify>, queue: Arc<Queue>) -> SearchView {
        let results_tracks = Arc::new(RwLock::new(Vec::new()));
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
            .tab("tracks", "Tracks", ListView::new(results_tracks.clone(), queue.clone()))
            .tab("playlists", "Playlists", ListView::new(results_playlists.clone(), queue.clone()));

        SearchView {
            results_tracks,
            results_playlists,
            edit: searchfield,
            list: tabs.with_id(LIST_ID),
            edit_focused: true,
            spotify
        }
    }

    pub fn clear(&mut self) {
        self.edit
            .call_on(&Selector::Id(EDIT_ID), |v: &mut EditView| {
                v.set_content("");
            });
    }

    pub fn run_search<S: Into<String>>(&mut self, query: S) {
        let query = query.into();
        let q = query.clone();
        self.edit
            .call_on(&Selector::Id(EDIT_ID), |v: &mut EditView| {
                v.set_content(q);
            });

        if let Some(results) = self.spotify.search_track(&query, 50, 0) {
            let tracks = results
                .tracks
                .items
                .iter()
                .map(|ft| Track::new(ft))
                .collect();
            let mut r = self.results_tracks.write().unwrap();
            *r = tracks;
            self.edit_focused = false;
        }

        if let Some(results) = self.spotify.search_playlist(&query, 50, 0) {
            let pls = results
                .playlists
                .items
                .iter()
                .map(|sp| Playlists::process_playlist(sp, &&self.spotify))
                .collect();
            let mut r = self.results_playlists.write().unwrap();
            *r = pls;
            self.edit_focused = false;
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
