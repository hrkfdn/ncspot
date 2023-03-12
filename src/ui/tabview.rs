use std::cmp::{max, min};
use std::collections::HashMap;

use cursive::align::HAlign;
use cursive::event::{Event, EventResult, MouseButton, MouseEvent};
use cursive::theme::ColorStyle;
use cursive::traits::View;
use cursive::{Cursive, Printer, Vec2};
use unicode_width::UnicodeWidthStr;

use crate::command::{Command, MoveAmount, MoveMode};
use crate::commands::CommandResult;
use crate::traits::{IntoBoxedViewExt, ViewExt};

pub struct Tab {
    view: Box<dyn ViewExt>,
}

pub struct TabView {
    tabs: Vec<Tab>,
    ids: HashMap<String, usize>,
    selected: usize,
    size: Vec2,
}

impl TabView {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            ids: HashMap::new(),
            selected: 0,
            size: Vec2::default(),
        }
    }

    pub fn add_tab<S: Into<String>, V: IntoBoxedViewExt>(&mut self, id: S, view: V) {
        let tab = Tab {
            view: view.into_boxed_view_ext(),
        };
        self.tabs.push(tab);
        self.ids.insert(id.into(), self.tabs.len() - 1);
    }

    pub fn tab<S: Into<String>, V: IntoBoxedViewExt>(mut self, id: S, view: V) -> Self {
        self.add_tab(id, view);
        self
    }

    pub fn move_focus_to(&mut self, target: usize) {
        let len = self.tabs.len().saturating_sub(1);
        self.selected = min(target, len);
    }

    pub fn move_focus(&mut self, delta: i32) {
        let new = self.selected as i32 + delta;
        self.move_focus_to(max(new, 0) as usize);
    }
}

impl View for TabView {
    fn draw(&self, printer: &Printer<'_, '_>) {
        if self.tabs.is_empty() {
            return;
        }

        let tabwidth = printer.size.x / self.tabs.len();
        for (i, tab) in self.tabs.iter().enumerate() {
            let style = if self.selected == i {
                ColorStyle::highlight()
            } else {
                ColorStyle::primary()
            };

            let mut width = tabwidth;
            if i == self.tabs.len() - 1 {
                width += printer.size.x % self.tabs.len();
            }

            let title = tab.view.title();
            let offset = HAlign::Center.get_offset(title.width(), width);

            printer.with_color(style, |printer| {
                printer.print_hline((i * tabwidth, 0), width, " ");
                printer.print((i * tabwidth + offset, 0), &title);
            });
        }

        if let Some(tab) = self.tabs.get(self.selected) {
            let printer = printer
                .offset((0, 1))
                .cropped((printer.size.x, printer.size.y - 1));

            tab.view.draw(&printer);
        }
    }

    fn layout(&mut self, size: Vec2) {
        self.size = size;
        if let Some(tab) = self.tabs.get_mut(self.selected) {
            tab.view.layout(Vec2::new(size.x, size.y - 1));
        }
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        if let Event::Mouse {
            offset,
            position,
            event,
        } = event
        {
            let position = position.checked_sub(offset);
            if let Some(0) = position.map(|p| p.y) {
                match event {
                    MouseEvent::WheelUp => self.move_focus(-1),
                    MouseEvent::WheelDown => self.move_focus(1),
                    MouseEvent::Press(MouseButton::Left) => {
                        let tabwidth = self.size.x / self.tabs.len();
                        if let Some(selected_tab) = position.and_then(|p| p.x.checked_div(tabwidth))
                        {
                            self.move_focus_to(selected_tab);
                        }
                    }
                    _ => {}
                };
                return EventResult::consumed();
            }
        }

        if let Some(tab) = self.tabs.get_mut(self.selected) {
            tab.view.on_event(event.relativized((0, 1)))
        } else {
            EventResult::Ignored
        }
    }
}

impl ViewExt for TabView {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        if let Command::Move(mode, amount) = cmd {
            let last_idx = self.tabs.len() - 1;

            match mode {
                MoveMode::Left if self.selected > 0 => {
                    match amount {
                        MoveAmount::Extreme => self.move_focus_to(0),
                        MoveAmount::Integer(amount) => self.move_focus(-(*amount)),
                        _ => (),
                    }
                    return Ok(CommandResult::Consumed(None));
                }
                MoveMode::Right if self.selected < last_idx => {
                    match amount {
                        MoveAmount::Extreme => self.move_focus_to(last_idx),
                        MoveAmount::Integer(amount) => self.move_focus(*amount),
                        _ => (),
                    }
                    return Ok(CommandResult::Consumed(None));
                }
                _ => {}
            }
        }

        if let Some(tab) = self.tabs.get_mut(self.selected) {
            tab.view.on_command(s, cmd)
        } else {
            Ok(CommandResult::Ignored)
        }
    }
}
