use std::collections::HashMap;

use cursive::align::HAlign;
use cursive::direction::Direction;
use cursive::event::{AnyCb, Event, EventResult};
use cursive::theme::ColorStyle;
use cursive::traits::View;
use cursive::vec::Vec2;
use cursive::view::{IntoBoxedView, Selector};
use cursive::Printer;
use unicode_width::UnicodeWidthStr;

struct Screen {
    title: String,
    view: Box<dyn View>,
}

pub struct Layout {
    views: HashMap<String, Screen>,
    title: String,
    statusbar: Box<dyn View>,
    focus: Option<String>,
}

impl Layout {
    pub fn new<T: IntoBoxedView>(status: T) -> Layout {
        Layout {
            views: HashMap::new(),
            title: String::new(),
            statusbar: status.as_boxed_view(),
            focus: None,
        }
    }

    pub fn add_view<S: Into<String>, T: IntoBoxedView>(&mut self, id: S, view: T, title: &str) {
        let s = id.into();
        let screen = Screen {
            title: title.to_string(),
            view: view.as_boxed_view(),
        };
        self.views.insert(s.clone(), screen);
        self.focus = Some(s);
    }

    pub fn view<S: Into<String>, T: IntoBoxedView>(mut self, id: S, view: T, title: &str) -> Self {
        (&mut self).add_view(id, view, title);
        self.title = title.to_owned();
        self
    }

    pub fn set_view<S: Into<String>>(&mut self, id: S) {
        let s = id.into();
        let title = &self.views.get(&s).unwrap().title;
        self.title = title.clone();
        self.focus = Some(s);
    }
}

impl View for Layout {
    fn draw(&self, printer: &Printer<'_, '_>) {
        // screen title
        printer.with_color(ColorStyle::title_primary(), |printer| {
            let offset = HAlign::Center.get_offset(self.title.width(), printer.size.x);
            printer.print((offset, 0), &self.title);
        });

        // screen content
        if let Some(ref id) = self.focus {
            let screen = self.views.get(id).unwrap();
            let printer = &printer
                .offset((0, 1))
                .cropped((printer.size.x, printer.size.y - 3))
                .focused(true);
            screen.view.draw(printer);
        }

        self.statusbar
            .draw(&printer.offset((0, printer.size.y - 2)));
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, constraint.y)
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        if let Some(ref id) = self.focus {
            let screen = self.views.get_mut(id).unwrap();
            screen.view.on_event(event)
        } else {
            EventResult::Ignored
        }
    }

    fn layout(&mut self, size: Vec2) {
        if let Some(ref id) = self.focus {
            let screen = self.views.get_mut(id).unwrap();
            screen.view.layout(Vec2::new(size.x, size.y - 3));
        }
    }

    fn call_on_any<'a>(&mut self, s: &Selector, c: AnyCb<'a>) {
        if let Some(ref id) = self.focus {
            let screen = self.views.get_mut(id).unwrap();
            screen.view.call_on_any(s, c);
        }
    }

    fn take_focus(&mut self, source: Direction) -> bool {
        if let Some(ref id) = self.focus {
            let screen = self.views.get_mut(id).unwrap();
            screen.view.take_focus(source)
        } else {
            false
        }
    }
}
