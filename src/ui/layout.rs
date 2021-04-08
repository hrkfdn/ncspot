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

pub struct Layout {
    screens: HashMap<String, Box<dyn ViewExt>>,
    stack: HashMap<String, Vec<Box<dyn ViewExt>>>,
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
            screens: HashMap::new(),
            stack: HashMap::new(),
            statusbar: status.into_boxed_view(),
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

    pub fn add_screen<S: Into<String>, T: IntoBoxedViewExt>(&mut self, id: S, view: T) {
        if let Some(view) = self.get_top_view() {
            view.on_leave();
        }

        let s = id.into();
        self.screens.insert(s.clone(), view.into_boxed_view_ext());
        self.stack.insert(s.clone(), Vec::new());
        self.focus = Some(s);
    }

    pub fn screen<S: Into<String>, T: IntoBoxedViewExt>(mut self, id: S, view: T) -> Self {
        (&mut self).add_screen(id, view);
        self
    }

    pub fn set_screen<S: Into<String>>(&mut self, id: S) {
        if let Some(view) = self.get_top_view() {
            view.on_leave();
        }

        let s = id.into();
        self.focus = Some(s);
        self.cmdline_focus = false;
        self.screenchange = true;

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
        if let Some(view) = self.get_top_view() {
            view.on_leave();
        }

        if let Some(stack) = self.get_focussed_stack_mut() {
            stack.push(view)
        }
    }

    pub fn pop_view(&mut self) {
        if let Some(view) = self.get_top_view() {
            view.on_leave();
        }

        self.get_focussed_stack_mut().map(|stack| stack.pop());
    }

    fn get_current_screen(&self) -> Option<&Box<dyn ViewExt>> {
        self.focus
            .as_ref()
            .and_then(|focus| self.screens.get(focus))
    }

    fn get_focussed_stack_mut(&mut self) -> Option<&mut Vec<Box<dyn ViewExt>>> {
        let focus = self.focus.clone();
        if let Some(focus) = &focus {
            self.stack.get_mut(focus)
        } else {
            None
        }
    }

    fn get_focussed_stack(&self) -> Option<&Vec<Box<dyn ViewExt>>> {
        self.focus.as_ref().and_then(|focus| self.stack.get(focus))
    }

    fn get_top_view(&self) -> Option<&Box<dyn ViewExt>> {
        let focussed_stack = self.get_focussed_stack();
        if focussed_stack.map(|s| s.len()).unwrap_or_default() > 0 {
            focussed_stack.unwrap().last()
        } else if let Some(id) = &self.focus {
            self.screens.get(id)
        } else {
            None
        }
    }

    fn get_current_view_mut(&mut self) -> Option<&mut Box<dyn ViewExt>> {
        if let Some(focus) = &self.focus {
            let last_view = self
                .stack
                .get_mut(focus)
                .filter(|stack| !stack.is_empty())
                .and_then(|stack| stack.last_mut());
            if last_view.is_some() {
                last_view
            } else {
                self.screens.get_mut(focus)
            }
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

        let screen_title = self
            .get_current_screen()
            .map(|screen| screen.title())
            .unwrap_or_default();

        if let Some(view) = self.get_top_view() {
            // back button + title
            if !self
                .get_focussed_stack()
                .map(|s| s.is_empty())
                .unwrap_or(false)
            {
                printer.with_color(ColorStyle::title_secondary(), |printer| {
                    printer.print((1, 0), &format!("< {}", screen_title));
                });
            }

            // view title
            printer.with_color(ColorStyle::title_primary(), |printer| {
                let offset = HAlign::Center.get_offset(view.title().width(), printer.size.x);
                printer.print((offset, 0), &view.title());
            });

            printer.with_color(ColorStyle::secondary(), |printer| {
                let offset = HAlign::Right.get_offset(view.title_sub().width(), printer.size.x);
                printer.print((offset, 0), &view.title_sub());
            });

            // screen content
            let printer = &printer
                .offset((0, 1))
                .cropped((printer.size.x, printer.size.y - 3 - cmdline_height))
                .focused(true);
            view.draw(printer);
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

        if let Some(view) = self.get_current_view_mut() {
            view.layout(Vec2::new(size.x, size.y - 3));
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
                if let Some(view) = self.get_current_view_mut() {
                    view.on_event(event);
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

        if let Some(view) = self.get_current_view_mut() {
            view.on_event(event)
        } else {
            EventResult::Ignored
        }
    }

    fn call_on_any<'a>(&mut self, s: &Selector, c: AnyCb<'a>) {
        if let Some(view) = self.get_current_view_mut() {
            view.call_on_any(s, c);
        }
    }

    fn take_focus(&mut self, source: Direction) -> bool {
        if self.cmdline_focus {
            return self.cmdline.take_focus(source);
        }

        if let Some(view) = self.get_current_view_mut() {
            view.take_focus(source)
        } else {
            false
        }
    }
}

impl ViewExt for Layout {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Focus(view) => {
                // Clear search results and return to search bar
                // If trying to focus search screen while already on it
                let search_view_name = "search";
                if view == search_view_name && self.focus == Some(search_view_name.into()) {
                    if let Some(stack) = self.stack.get_mut(search_view_name) {
                        stack.clear();
                    }
                }

                if self.screens.keys().any(|k| k == view) {
                    self.set_screen(view.clone());
                    let screen = self.screens.get_mut(view).unwrap();
                    screen.on_command(s, cmd)?;
                }

                Ok(CommandResult::Consumed(None))
            }
            Command::Back => {
                self.pop_view();
                Ok(CommandResult::Consumed(None))
            }
            _ => {
                if let Some(view) = self.get_current_view_mut() {
                    view.on_command(s, cmd)
                } else {
                    Ok(CommandResult::Ignored)
                }
            }
        }
    }
}
