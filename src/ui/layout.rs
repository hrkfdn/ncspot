use std::collections::HashMap;

use cursive::direction::Direction;
use cursive::event::{AnyCb, Event, EventResult};
use cursive::traits::View;
use cursive::vec::Vec2;
use cursive::view::{IntoBoxedView, Selector};
use cursive::Printer;

pub struct Layout {
    views: HashMap<String, Box<dyn View>>,
    statusbar: Box<dyn View>,
    focus: Option<String>,
}

impl Layout {
    pub fn new<T: IntoBoxedView>(status: T) -> Layout {
        Layout {
            views: HashMap::new(),
            statusbar: status.as_boxed_view(),
            focus: None,
        }
    }

    pub fn add_view<S: Into<String>, T: IntoBoxedView>(&mut self, id: S, view: T) {
        let s = id.into();
        self.views.insert(s.clone(), view.as_boxed_view());
        self.focus = Some(s);
    }

    pub fn view<S: Into<String>, T: IntoBoxedView>(mut self, id: S, view: T) -> Self {
        (&mut self).add_view(id, view);
        self
    }

    pub fn set_view<S: Into<String>>(&mut self, id: S) {
        let s = id.into();
        self.focus = Some(s);
    }
}

impl View for Layout {
    fn draw(&self, printer: &Printer<'_, '_>) {
        if let Some(ref id) = self.focus {
            let v = self.views.get(id).unwrap();
            let printer = &printer
                .offset((0, 0))
                .cropped((printer.size.x, printer.size.y))
                .focused(true);
            v.draw(printer);
        }

        self.statusbar
            .draw(&printer.offset((0, printer.size.y - 2)));
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, constraint.y)
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        if let Some(ref id) = self.focus {
            let v = self.views.get_mut(id).unwrap();
            v.on_event(event)
        } else {
            EventResult::Ignored
        }
    }

    fn layout(&mut self, size: Vec2) {
        if let Some(ref id) = self.focus {
            let v = self.views.get_mut(id).unwrap();
            v.layout(Vec2::new(size.x, size.y - 2));
        }
    }

    fn call_on_any<'a>(&mut self, s: &Selector, c: AnyCb<'a>) {
        if let Some(ref id) = self.focus {
            let v = self.views.get_mut(id).unwrap();
            v.call_on_any(s, c);
        }
    }

    fn take_focus(&mut self, source: Direction) -> bool {
        if let Some(ref id) = self.focus {
            let v = self.views.get_mut(id).unwrap();
            v.take_focus(source)
        } else {
            false
        }
    }
}
