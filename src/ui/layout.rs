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
use cursive::{Cursive, Printer};
use unicode_width::UnicodeWidthStr;

use crate::command::Command;
use crate::commands::CommandResult;
use crate::events;
use crate::traits::{IntoBoxedViewExt, ViewExt};

struct Screen {
    title: String,
    view: Box<dyn ViewExt>,
}

pub struct Layout {
    views: HashMap<String, Screen>,
    stack: Vec<Screen>,
    statusbar: Box<dyn View>,
    focus: Option<String>,
    pub cmdline: EditView,
    cmdline_focus: bool,
    result: Result<Option<String>, String>,
    result_time: Option<SystemTime>,
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
            stack: Vec::new(),
            statusbar: status.as_boxed_view(),
            focus: None,
            cmdline: EditView::new().filler(" ").style(style),
            cmdline_focus: false,
            result: Ok(None),
            result_time: None,
            screenchange: true,
            last_size: Vec2::new(0, 0),
            ev: ev.clone(),
            theme,
        }
    }

    pub fn enable_cmdline(&mut self) {
        if !self.cmdline_focus {
            self.cmdline.set_content(":");
            self.cmdline_focus = true;
        }
    }

    pub fn enable_jump(&mut self) {
        if !self.cmdline_focus {
            self.cmdline.set_content("/");
            self.cmdline_focus = true;
        }
    }

    pub fn add_view<S: Into<String>, T: IntoBoxedViewExt>(&mut self, id: S, view: T, title: S) {
        let s = id.into();
        let screen = Screen {
            title: title.into(),
            view: view.as_boxed_view_ext(),
        };
        self.views.insert(s.clone(), screen);
        self.focus = Some(s);
    }

    pub fn view<S: Into<String>, T: IntoBoxedViewExt>(mut self, id: S, view: T, title: S) -> Self {
        (&mut self).add_view(id, view, title);
        self
    }

    pub fn set_view<S: Into<String>>(&mut self, id: S) {
        let s = id.into();
        self.focus = Some(s);
        self.cmdline_focus = false;
        self.screenchange = true;
        self.stack.clear();

        // trigger a redraw
        self.ev.trigger();
    }

    pub fn set_result(&mut self, result: Result<Option<String>, String>) {
        self.result = result;
        self.result_time = Some(SystemTime::now());
    }

    pub fn clear_cmdline(&mut self) {
        self.cmdline.set_content("");
        self.cmdline_focus = false;
        self.result = Ok(None);
        self.result_time = None;
    }

    fn get_result(&self) -> Result<Option<String>, String> {
        if let Some(t) = self.result_time {
            if t.elapsed().unwrap() > Duration::from_secs(5) {
                return Ok(None);
            }
        }
        self.result.clone()
    }

    pub fn push_view(&mut self, view: Box<dyn ViewExt>) {
        let title = view.title();
        let screen = Screen { title, view };

        self.stack.push(screen);
    }

    pub fn pop_view(&mut self) {
        self.stack.pop();
    }

    fn get_current_screen(&self) -> Option<&Screen> {
        if !self.stack.is_empty() {
            return self.stack.last();
        }

        if let Some(id) = self.focus.as_ref() {
            self.views.get(id)
        } else {
            None
        }
    }

    fn get_current_screen_mut(&mut self) -> Option<&mut Screen> {
        if !self.stack.is_empty() {
            return self.stack.last_mut();
        }

        if let Some(id) = self.focus.as_ref() {
            self.views.get_mut(id)
        } else {
            None
        }
    }
}

impl View for Layout {
    fn draw(&self, printer: &Printer<'_, '_>) {
        let result = self.get_result();

        let cmdline_visible = self.cmdline.get_content().len() > 0;
        let mut cmdline_height = if cmdline_visible { 1 } else { 0 };
        if result.as_ref().map(Option::is_some).unwrap_or(true) {
            cmdline_height += 1;
        }

        if let Some(screen) = self.get_current_screen() {
            // screen title
            printer.with_color(ColorStyle::title_primary(), |printer| {
                let offset = HAlign::Center.get_offset(screen.title.width(), printer.size.x);
                printer.print((offset, 0), &screen.title);

                if !self.stack.is_empty() {
                    printer.print((1, 0), "<");
                }
            });

            // screen content
            let printer = &printer
                .offset((0, 1))
                .cropped((printer.size.x, printer.size.y - 3 - cmdline_height))
                .focused(true);
            screen.view.draw(printer);
        }

        self.statusbar
            .draw(&printer.offset((0, printer.size.y - 2 - cmdline_height)));

        if let Ok(Some(r)) = result {
            printer.print_hline((0, printer.size.y - cmdline_height), printer.size.x, " ");
            printer.print((0, printer.size.y - cmdline_height), &r);
        } else if let Err(e) = result {
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

    fn layout(&mut self, size: Vec2) {
        self.last_size = size;

        self.statusbar.layout(Vec2::new(size.x, 2));

        self.cmdline.layout(Vec2::new(size.x, 1));

        if let Some(screen) = self.get_current_screen_mut() {
            screen.view.layout(Vec2::new(size.x, size.y - 3));
        }

        // the focus view has changed, let the views know so they can redraw
        // their items
        if self.screenchange {
            debug!("layout: new screen selected: {:?}", self.focus);
            self.screenchange = false;
        }
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, constraint.y)
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        if let Event::Mouse { position, .. } = event {
            let result = self.get_result();

            let cmdline_visible = self.cmdline.get_content().len() > 0;
            let mut cmdline_height = if cmdline_visible { 1 } else { 0 };
            if result.as_ref().map(Option::is_some).unwrap_or(true) {
                cmdline_height += 1;
            }

            if position.y < self.last_size.y.saturating_sub(2 + cmdline_height) {
                if let Some(ref id) = self.focus {
                    let screen = self.views.get_mut(id).unwrap();
                    screen.view.on_event(event);
                }
            } else if position.y < self.last_size.y - cmdline_height {
                self.statusbar.on_event(
                    event.relativized(Vec2::new(0, self.last_size.y - 2 - cmdline_height)),
                );
            }

            return EventResult::Consumed(None);
        }

        if self.cmdline_focus {
            return self.cmdline.on_event(event);
        }

        if let Some(screen) = self.get_current_screen_mut() {
            screen.view.on_event(event)
        } else {
            EventResult::Ignored
        }
    }

    fn call_on_any<'a>(&mut self, s: &Selector, c: AnyCb<'a>) {
        if let Some(screen) = self.get_current_screen_mut() {
            screen.view.call_on_any(s, c);
        }
    }

    fn take_focus(&mut self, source: Direction) -> bool {
        if self.cmdline_focus {
            return self.cmdline.take_focus(source);
        }

        if let Some(screen) = self.get_current_screen_mut() {
            screen.view.take_focus(source)
        } else {
            false
        }
    }
}

impl ViewExt for Layout {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Search(_) => {
                self.set_view("search");
                self.get_current_screen_mut()
                    .map(|search| search.view.on_command(s, cmd));
                Ok(CommandResult::Consumed(None))
            }
            Command::Focus(view) => {
                if self.views.keys().any(|k| k == view) {
                    self.set_view(view.clone());
                    let screen = self.views.get_mut(view).unwrap();
                    screen.view.on_command(s, cmd)?;
                }

                Ok(CommandResult::Consumed(None))
            }
            Command::Back => {
                self.pop_view();
                Ok(CommandResult::Consumed(None))
            }
            _ => {
                if let Some(screen) = self.get_current_screen_mut() {
                    screen.view.on_command(s, cmd)
                } else {
                    Ok(CommandResult::Ignored)
                }
            }
        }
    }
}
