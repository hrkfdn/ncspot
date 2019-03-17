use std::cmp::{max, min};
use std::sync::{Arc, RwLock};

use cursive::align::HAlign;
use cursive::event::{Event, EventResult, Key};
use cursive::theme::ColorStyle;
use cursive::traits::View;
use cursive::{Printer, Rect, Vec2};
use unicode_width::UnicodeWidthStr;

use queue::Queue;
use traits::ListItem;

pub struct ListView<I: 'static + ListItem> {
    content: Arc<RwLock<Vec<I>>>,
    selected: usize,
    queue: Arc<Queue>,
}

impl<I: ListItem> ListView<I> {
    pub fn new(content: Arc<RwLock<Vec<I>>>, queue: Arc<Queue>) -> Self {
        Self {
            content: content,
            selected: 0,
            queue: queue,
        }
    }

    pub fn with_selected(&self, cb: Box<Fn(&I) -> ()>) {
        match self.content.read().unwrap().get(self.selected) {
            Some(x) => cb(x),
            None => error!("listview: invalid item index: {})", self.selected)
        }
    }

    pub fn get_selected_index(&self) -> usize {
        self.selected
    }

    pub fn move_focus(&mut self, delta: i32) {
        let len = self.content.read().unwrap().len() as i32;
        let new = self.selected as i32 + delta;
        self.selected = min(max(new, 0), len - 1) as usize;
    }
}

impl<I: ListItem> View for ListView<I> {
    fn draw(&self, printer: &Printer<'_, '_>) {
        for (i, item) in self.content.read().unwrap().iter().enumerate() {
            let style = if self.selected == i {
                ColorStyle::highlight()
            } else if item.is_playing(self.queue.clone()) {
                ColorStyle::secondary()
            } else {
                ColorStyle::primary()
            };

            let left = item.display_left();
            let right = item.display_right();

            // draw left string
            printer.with_color(style, |printer| {
                printer.print((0, i), &left);
            });

            // draw ".." to indicate a cut off string
            let max_length = printer.size.x.checked_sub(right.width() + 1).unwrap_or(0);
            if max_length < left.width() {
                let offset = max_length.checked_sub(1).unwrap_or(0);
                printer.with_color(style, |printer| {
                    printer.print((offset, i), "..");
                });
            }

            // draw right string
            let offset = HAlign::Right.get_offset(right.width(), printer.size.x);

            printer.with_color(style, |printer| {
                printer.print((offset, i), &right);
            });
        }
    }

    fn on_event(&mut self, e: Event) -> EventResult {
        match e {
            Event::Key(Key::Up) => {
                self.move_focus(-1);
                EventResult::Consumed(None)
            }
            Event::Key(Key::Down) => {
                self.move_focus(1);
                EventResult::Consumed(None)
            }
            Event::Key(Key::PageUp) => {
                self.move_focus(-10);
                EventResult::Consumed(None)
            }
            Event::Key(Key::PageDown) => {
                self.move_focus(10);
                EventResult::Consumed(None)
            }
            _ => EventResult::Ignored,
        }
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, self.content.read().unwrap().len())
    }

    fn important_area(&self, view_size: Vec2) -> Rect {
        if self.content.read().unwrap().len() > 0 {
            Rect::from((view_size.x, self.selected))
        }
        else {
            Rect::from((0, 0))
        }
    }
}
