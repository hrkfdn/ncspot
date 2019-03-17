#![allow(unused_imports)]

use cursive::direction::Orientation;
use cursive::event::{AnyCb, Event, EventResult, Key};
use cursive::traits::{Boxable, Identifiable, Finder, View};
use cursive::view::{Selector, ViewWrapper};
use cursive::views::{EditView, IdView, ScrollView, ViewRef};
use cursive::{Cursive, Printer, Vec2};
use std::cell::RefCell;
use std::sync::{Arc, Mutex, RwLock};

use queue::Queue;
use spotify::Spotify;
use track::Track;
use ui::listview::ListView;

pub struct SearchView {
    results: Arc<RwLock<Vec<Track>>>,
    edit: IdView<EditView>,
    list: ScrollView<IdView<ListView<Track>>>,
    edit_focused: bool,
}

impl SearchView {
    pub fn new(spotify: Arc<Spotify>, queue: Arc<Queue>) -> SearchView {
        let results = Arc::new(RwLock::new(Vec::new()));

        let searchfield = EditView::new()
            .on_submit(move |s, input| {
                if input.len() > 0 {
                    s.call_on_id("search", |v: &mut SearchView| {
                        v.run_search(input, spotify.clone());
                        v.focus_view(&Selector::Id("list")).unwrap();
                    });
                }
            })
            .with_id("search_edit");
        let list = ListView::new(results.clone(), queue).with_id("list");
        let scrollable = ScrollView::new(list);

        SearchView {
            results: results,
            edit: searchfield,
            list: scrollable,
            edit_focused: false
        }
    }

    pub fn run_search<S: Into<String>>(&mut self, query: S, spotify: Arc<Spotify>) {
        let query = query.into();
        let q = query.clone();
        self.edit.call_on(&Selector::Id("search_edit"), |v: &mut EditView| {
            v.set_content(q);
        });

        if let Ok(results) = spotify.search(&query, 50, 0) {
            let tracks = results.tracks.items.iter().map(|ft| Track::new(ft)).collect();
            let mut r = self.results.write().unwrap();
            *r = tracks;
            self.edit_focused = false;
        }
    }

    pub fn focus_search(&mut self) {
        self.edit.call_on(&Selector::Id("search_edit"), |v: &mut EditView| {
            v.set_content("");
        });
        self.edit_focused = true;
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
            if event == Event::Key(Key::Esc) {
                self.edit_focused = false;
                EventResult::Consumed(None)
            } else {
                self.edit.on_event(event)
            }
        } else {
            self.list.on_event(event)
        }
    }
}
