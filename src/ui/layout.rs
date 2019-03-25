use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use cursive::align::HAlign;
use cursive::direction::Direction;
use cursive::event::{AnyCb, Event, EventResult};
use cursive::theme::{ColorStyle, ColorType, Theme};
use cursive::traits::View;
use cursive::vec::Vec2;
use cursive::view::{IntoBoxedView, Selector};
use cursive::views::EditView;
use cursive::Printer;
use unicode_width::UnicodeWidthStr;

use events;

struct Screen {
    title: String,
    view: Box<dyn View>,
}

pub struct Layout {
    views: HashMap<String, Screen>,
    title: String,
    statusbar: Box<dyn View>,
    focus: Option<String>,
    pub cmdline: EditView,
    cmdline_focus: bool,
    error: Option<String>,
    error_time: Option<SystemTime>,
    screenchange: bool,
    last_size: Vec2,
    ev: events::EventManager,
    theme: Theme,
}

impl Layout {
    pub fn new<T: IntoBoxedView>(status: T, ev: &events::EventManager, theme: Theme) -> Layout {
        let style = ColorStyle::new(
            ColorType::Color(*theme.palette.custom("cmdline_bg").unwrap()),
            ColorType::Color(*theme.palette.custom("cmdline").unwrap()),
        );

        Layout {
            views: HashMap::new(),
            title: String::new(),
            statusbar: status.as_boxed_view(),
            focus: None,
            cmdline: EditView::new().filler(" ").style(style),
            cmdline_focus: false,
            error: None,
            error_time: None,
            screenchange: true,
            last_size: Vec2::new(0, 0),
            ev: ev.clone(),
            theme: theme,
        }
    }

    pub fn enable_cmdline(&mut self) {
        if !self.cmdline_focus {
            self.cmdline.set_content(":");
            self.cmdline_focus = true;
        }
    }

    pub fn add_view<S: Into<String>, T: IntoBoxedView>(&mut self, id: S, view: T, title: &str) {
        let s = id.into();
        let screen = Screen {
            title: title.to_string(),
            view: view.as_boxed_view(),
        };
        self.views.insert(s.clone(), screen);
        self.title = title.to_owned();
        self.focus = Some(s);
    }

    pub fn view<S: Into<String>, T: IntoBoxedView>(mut self, id: S, view: T, title: &str) -> Self {
        (&mut self).add_view(id, view, title);
        self
    }

    pub fn set_view<S: Into<String>>(&mut self, id: S) {
        let s = id.into();
        let title = &self.views.get(&s).unwrap().title;
        self.title = title.clone();
        self.focus = Some(s);
        self.cmdline_focus = false;
        self.screenchange = true;

        // trigger a redraw
        self.ev.trigger();
    }

    pub fn set_error<S: Into<String>>(&mut self, error: S) {
        self.error = Some(error.into());
        self.error_time = Some(SystemTime::now());
    }

    pub fn clear_cmdline(&mut self) {
        self.cmdline.set_content("");
        self.cmdline_focus = false;
        self.error = None;
        self.error_time = None;
    }

    fn get_error(&self) -> Option<String> {
        if let Some(t) = self.error_time {
            if t.elapsed().unwrap() > Duration::from_secs(5) {
                return None;
            }
        }
        self.error.clone()
    }
}

impl View for Layout {
    fn draw(&self, printer: &Printer<'_, '_>) {
        let error = self.get_error();

        let cmdline_visible = self.cmdline.get_content().len() > 0;
        let mut cmdline_height = if cmdline_visible { 1 } else { 0 };
        if error.is_some() {
            cmdline_height += 1;
        }

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
                .cropped((printer.size.x, printer.size.y - 3 - cmdline_height))
                .focused(true);
            screen.view.draw(printer);
        }

        self.statusbar
            .draw(&printer.offset((0, printer.size.y - 2 - cmdline_height)));

        if let Some(e) = error {
            let style = ColorStyle::new(
                ColorType::Color(*self.theme.palette.custom("error").unwrap()),
                ColorType::Color(*self.theme.palette.custom("error_bg").unwrap()),
            );
            printer.with_color(style, |printer| {
                printer.print_hline((0, printer.size.y - cmdline_height), printer.size.x, " ");
                printer.print(
                    (0, printer.size.y - cmdline_height),
                    &format!("ERROR: {}", e),
                );
            });
        }

        if cmdline_visible {
            let printer = &printer.offset((0, printer.size.y - 1));
            self.cmdline.draw(&printer);
        }
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, constraint.y)
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        if let Event::Mouse {
            position,
            ..
        } = event {
            let error = self.get_error();

            let cmdline_visible = self.cmdline.get_content().len() > 0;
            let mut cmdline_height = if cmdline_visible { 1 } else { 0 };
            if error.is_some() {
                cmdline_height += 1;
            }

            if position.y < self.last_size.y - 2 - cmdline_height {
                if let Some(ref id) = self.focus {
                    let screen = self.views.get_mut(id).unwrap();
                    screen.view.on_event(event.clone());
                }
            } else if position.y < self.last_size.y - cmdline_height {
                self.statusbar.on_event(event.relativized(Vec2::new(0, self.last_size.y - 2 - cmdline_height)));
            }

            return EventResult::Consumed(None);
        }

        if self.cmdline_focus {
            return self.cmdline.on_event(event);
        }

        if let Some(ref id) = self.focus {
            let screen = self.views.get_mut(id).unwrap();
            screen.view.on_event(event)
        } else {
            EventResult::Ignored
        }
    }

    fn layout(&mut self, size: Vec2) {
        self.last_size = size;

        self.statusbar.layout(Vec2::new(size.x, 2));

        self.cmdline.layout(Vec2::new(size.x, 1));

        if let Some(ref id) = self.focus {
            let screen = self.views.get_mut(id).unwrap();
            screen.view.layout(Vec2::new(size.x, size.y - 3));

            // the focus view has changed, let the views know so they can redraw
            // their items
            if self.screenchange {
                debug!("layout: new screen selected: {}", &id);
                self.screenchange = false;
            }
        }
    }

    fn call_on_any<'a>(&mut self, s: &Selector, c: AnyCb<'a>) {
        if let Some(ref id) = self.focus {
            let screen = self.views.get_mut(id).unwrap();
            screen.view.call_on_any(s, c);
        }
    }

    fn take_focus(&mut self, source: Direction) -> bool {
        if self.cmdline_focus {
            return self.cmdline.take_focus(source);
        }

        if let Some(ref id) = self.focus {
            let screen = self.views.get_mut(id).unwrap();
            screen.view.take_focus(source)
        } else {
            false
        }
    }
}
