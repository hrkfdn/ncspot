#![allow(unused_imports)]

use cursive::direction::Orientation;
use cursive::event::{AnyCb, Event, EventResult, Key};
use cursive::traits::{Boxable, Finder, Identifiable, View};
use cursive::view::{Selector, ViewWrapper};
use cursive::views::{EditView, IdView, ViewRef};
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
    list: IdView<ListView<Track>>,
    edit_focused: bool,
}

impl SearchView {
    pub fn new(spotify: Arc<Spotify>, queue: Arc<Queue>) -> SearchView {
        let results = Arc::new(RwLock::new(Vec::new()));

        let searchfield = EditView::new()
            .on_submit(move |s, input| {
                if !input.is_empty() {
                    s.call_on_id("search", |v: &mut SearchView| {
                        v.run_search(input, spotify.clone());
                        v.focus_view(&Selector::Id("list")).unwrap();
                    });
                }
            })
            .with_id("search_edit");
        let list = ListView::new(results.clone(), queue).with_id("list");

        SearchView {
            results,
            edit: searchfield,
            list,
            edit_focused: true,
        }
    }

    pub fn run_search<S: Into<String>>(&mut self, query: S, spotify: Arc<Spotify>) {
        let query = query.into();
        let q = query.clone();
        self.edit
            .call_on(&Selector::Id("search_edit"), |v: &mut EditView| {
                v.set_content(q);
            });

        if let Some(results) = spotify.search(&query, 50, 0) {
            let tracks = results
                .tracks
                .items
                .iter()
                .map(|ft| Track::new(ft))
                .collect();
            let mut r = self.results.write().unwrap();
            *r = tracks;
            self.edit_focused = false;
        }
    }

    fn list_index(&self) -> usize {
        self.list.with_view(|v| v.get_selected_index()).unwrap_or(0)
    }

    fn pass_event_focused(&mut self, event: Event) -> EventResult {
        if self.edit_focused {
            self.edit.on_event(event)
        } else {
            self.list.on_event(event)
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
        match event {
            Event::Key(Key::Tab) => {
                self.edit_focused = !self.edit_focused;
                EventResult::Consumed(None)
            }
            Event::Key(Key::Esc) if self.edit_focused => {
                self.edit_focused = false;
                EventResult::Consumed(None)
            }
            Event::Key(Key::Down) if self.edit_focused => {
                self.edit_focused = false;
                EventResult::Consumed(None)
            }
            Event::Key(Key::Up) if (!self.edit_focused && self.list_index() == 0) => {
                self.edit_focused = true;
                EventResult::Consumed(None)
            }
            _ => self.pass_event_focused(event),
        }
    }
}
