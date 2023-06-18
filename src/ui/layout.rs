use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use cursive::align::HAlign;
use cursive::direction::Direction;
use cursive::event::{AnyCb, Event, EventResult, Key, MouseButton, MouseEvent};
use cursive::theme::{ColorStyle, ColorType, Theme};
use cursive::traits::View;
use cursive::vec::Vec2;
use cursive::view::{CannotFocus, IntoBoxedView, Selector};
use cursive::views::EditView;
use cursive::{Cursive, Printer};
use unicode_width::UnicodeWidthStr;

use crate::application::UserData;
use crate::command::{self, Command, JumpMode};
use crate::commands::CommandResult;
use crate::config::{self, Config};
use crate::events;
use crate::ext_traits::CursiveExt;
use crate::traits::{IntoBoxedViewExt, ViewExt};

pub struct Layout {
    screens: HashMap<String, Box<dyn ViewExt>>,
    stack: HashMap<String, Vec<Box<dyn ViewExt>>>,
    statusbar: Box<dyn View>,
    focus: Option<String>,
    cmdline: EditView,
    cmdline_focus: bool,
    result: Result<Option<String>, String>,
    result_time: Option<SystemTime>,
    last_size: Vec2,
    ev: events::EventManager,
    theme: Theme,
    configuration: Arc<Config>,
}

impl Layout {
    pub fn new<T: IntoBoxedView>(
        status: T,
        ev: &events::EventManager,
        theme: Theme,
        configuration: Arc<Config>,
    ) -> Layout {
        let style = ColorStyle::new(
            ColorType::Color(*theme.palette.custom("cmdline_bg").unwrap()),
            ColorType::Color(*theme.palette.custom("cmdline").unwrap()),
        );
        let mut command_line_input = EditView::new().filler(" ").style(style);

        let event_manager = ev.clone();
        // 1. When a search was submitted on the commandline...
        command_line_input.set_on_submit(move |s, cmd| {
            // 2. Clear the commandline on Layout...
            s.on_layout(|_, mut layout| layout.clear_cmdline());

            // 3. Get the actual command without the prefix (like `:` or `/`)...
            let mut command_characters = cmd.chars();
            command_characters.next();
            let cmd_without_prefix = command_characters.as_str();
            if cmd.strip_prefix('/').is_some() {
                // 4. If it is a search command...

                // 5. Send a jump command with the search query to the command manager.
                let command = Command::Jump(JumpMode::Query(cmd_without_prefix.to_string()));
                if let Some(data) = s.user_data::<UserData>().cloned() {
                    data.cmd.handle(s, command);
                }
            } else {
                // 4. If it is an actual command...

                // 5. Parse the command and...
                match command::parse(cmd_without_prefix) {
                    Ok(commands) => {
                        // 6. Send the parsed command to the command manager.
                        if let Some(data) = s.user_data::<UserData>().cloned() {
                            for cmd in commands {
                                data.cmd.handle(s, cmd);
                            }
                        }
                    }
                    Err(err) => {
                        // 6. Set an error message on the global layout.
                        s.on_layout(|_, mut layout| layout.set_result(Err(err.to_string())));
                    }
                }
            }
            event_manager.trigger();
        });

        Layout {
            screens: HashMap::new(),
            stack: HashMap::new(),
            statusbar: status.into_boxed_view(),
            focus: None,
            cmdline: command_line_input,
            cmdline_focus: false,
            result: Ok(None),
            result_time: None,
            last_size: Vec2::new(0, 0),
            ev: ev.clone(),
            theme,
            configuration,
        }
    }

    pub fn enable_cmdline(&mut self, prefix: char) {
        if !self.cmdline_focus {
            self.cmdline.set_content(prefix);
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
        self.add_screen(id, view);
        self
    }

    pub fn has_screen(&self, id: &str) -> bool {
        self.screens.contains_key(id)
    }

    pub fn set_screen<S: Into<String>>(&mut self, id: S) {
        if let Some(view) = self.get_top_view() {
            view.on_leave();
        }

        let s = id.into();
        self.focus = Some(s);
        self.cmdline_focus = false;

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

    #[allow(clippy::borrowed_box)]
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

    fn is_current_stack_empty(&self) -> bool {
        self.get_focussed_stack()
            .map(|s| s.is_empty())
            .unwrap_or(false)
    }

    #[allow(clippy::borrowed_box)]
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

    /// Propagate the given event to the command line.
    fn command_line_handle_event(&mut self, event: Event) -> EventResult {
        let is_left_right_event = matches!(event, Event::Key(Key::Left) | Event::Key(Key::Right));
        let result = self.cmdline.on_event(event);

        if self.cmdline.get_content().is_empty() {
            self.clear_cmdline();
        }

        if is_left_right_event {
            EventResult::consumed()
        } else {
            result
        }
    }
}

impl View for Layout {
    fn draw(&self, printer: &Printer<'_, '_>) {
        let result = self.get_result();

        let cmdline_visible = self.cmdline.get_content().len() > 0;
        let mut cmdline_height = usize::from(cmdline_visible);
        if result.as_ref().map(Option::is_some).unwrap_or(true) {
            cmdline_height += 1;
        }

        let screen_title = self
            .get_current_screen()
            .map(|screen| screen.title())
            .unwrap_or_default();

        if let Some(view) = self.get_top_view() {
            // back button + title
            if !self.is_current_stack_empty() {
                printer.with_color(ColorStyle::title_secondary(), |printer| {
                    printer.print((1, 0), &format!("< {screen_title}"));
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
                printer.print((0, printer.size.y - cmdline_height), &format!("ERROR: {e}"));
            });
        }

        if cmdline_visible {
            let printer = &printer.offset((0, printer.size.y - 1));
            self.cmdline.draw(printer);
        }
    }

    fn layout(&mut self, size: Vec2) {
        self.last_size = size;

        self.statusbar.layout(Vec2::new(size.x, 2));

        self.cmdline.layout(Vec2::new(size.x, 1));

        if let Some(view) = self.get_current_view_mut() {
            view.layout(Vec2::new(size.x, size.y - 3));
        }
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, constraint.y)
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        match event {
            Event::Key(Key::Esc) if self.cmdline_focus => {
                self.clear_cmdline();
                EventResult::consumed()
            }
            _ if self.cmdline_focus => self.command_line_handle_event(event),
            Event::Char(character)
                if !self.cmdline_focus
                    && (character
                        == self
                            .configuration
                            .values()
                            .command_key
                            .unwrap_or(config::DEFAULT_COMMAND_KEY)
                        || character == '/') =>
            {
                let result = self
                    .get_current_view_mut()
                    .map(|view| view.on_event(event))
                    .unwrap_or(EventResult::Ignored);

                if let EventResult::Ignored = result {
                    let command_key = self
                        .configuration
                        .values()
                        .command_key
                        .unwrap_or(config::DEFAULT_COMMAND_KEY);

                    if character == command_key {
                        self.enable_cmdline(command_key);
                        EventResult::consumed()
                    } else if character == '/' {
                        self.enable_jump();
                        EventResult::consumed()
                    } else {
                        EventResult::Ignored
                    }
                } else {
                    result
                }
            }
            Event::Mouse {
                position,
                event: mouse_event,
                ..
            } => {
                // Handle mouse events in the command/jump area.
                if position.y == 0 {
                    if mouse_event == MouseEvent::Press(MouseButton::Left)
                        && !self.is_current_stack_empty()
                        && position.x
                            < self
                                .get_current_screen()
                                .map(|screen| screen.title())
                                .unwrap_or_default()
                                .len()
                                + 3
                    {
                        self.pop_view();
                    }
                    return EventResult::consumed();
                }

                let result = self.get_result();

                let cmdline_visible = self.cmdline.get_content().len() > 0;
                let mut cmdline_height = usize::from(cmdline_visible);
                if result.as_ref().map(Option::is_some).unwrap_or(true) {
                    cmdline_height += 1;
                }

                if position.y >= self.last_size.y.saturating_sub(2 + cmdline_height)
                    && position.y < self.last_size.y - cmdline_height
                {
                    self.statusbar.on_event(
                        event.relativized(Vec2::new(0, self.last_size.y - 2 - cmdline_height)),
                    );
                    return EventResult::consumed();
                }

                if let Some(view) = self.get_current_view_mut() {
                    view.on_event(event.relativized((0, 1)))
                } else {
                    EventResult::Ignored
                }
            }
            _ => {
                if let Some(view) = self.get_current_view_mut() {
                    view.on_event(event.relativized((0, 1)))
                } else {
                    EventResult::Ignored
                }
            }
        }
    }

    fn call_on_any(&mut self, s: &Selector, c: AnyCb<'_>) {
        if let Some(view) = self.get_current_view_mut() {
            view.call_on_any(s, c);
        }
    }

    fn take_focus(&mut self, source: Direction) -> Result<EventResult, CannotFocus> {
        if self.cmdline_focus {
            return self.cmdline.take_focus(source);
        }

        if let Some(view) = self.get_current_view_mut() {
            view.take_focus(source)
        } else {
            Err(CannotFocus)
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
