use std::cmp::{max, min};
use std::collections::HashMap;

use cursive::align::HAlign;
use cursive::theme::{ColorStyle, ColorType, PaletteColor};
use cursive::traits::View;
use cursive::{Cursive, Printer, Vec2};
use unicode_width::UnicodeWidthStr;

use crate::command::{Command, MoveAmount, MoveMode};
use crate::commands::CommandResult;
use crate::traits::{IntoBoxedViewExt, ViewExt};

pub struct Tab {
    title: String,
    view: Box<dyn ViewExt>,
}

pub struct TabView {
    tabs: Vec<Tab>,
    ids: HashMap<String, usize>,
    selected: usize,
}

impl TabView {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            ids: HashMap::new(),
            selected: 0,
        }
    }

    pub fn add_tab<S: Into<String>, V: IntoBoxedViewExt>(&mut self, id: S, title: S, view: V) {
        let tab = Tab {
            title: title.into(),
            view: view.as_boxed_view_ext(),
        };
        self.tabs.push(tab);
        self.ids.insert(id.into(), self.tabs.len() - 1);
    }

    pub fn tab<S: Into<String>, V: IntoBoxedViewExt>(mut self, id: S, title: S, view: V) -> Self {
        self.add_tab(id, title, view);
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
                ColorStyle::new(
                    ColorType::Palette(PaletteColor::Tertiary),
                    ColorType::Palette(PaletteColor::Highlight),
                )
            } else {
                ColorStyle::primary()
            };

            let mut width = tabwidth;
            if i == self.tabs.len() - 1 {
                width += printer.size.x % self.tabs.len();
            }

            let offset = HAlign::Center.get_offset(tab.title.width(), width);

            printer.with_color(style, |printer| {
                printer.print_hline((i * tabwidth, 0), width, " ");
                printer.print((i * tabwidth + offset, 0), &tab.title);
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
        if let Some(tab) = self.tabs.get_mut(self.selected) {
            tab.view.layout(Vec2::new(size.x, size.y - 1));
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
                    }
                    return Ok(CommandResult::Consumed(None));
                }
                MoveMode::Right if self.selected < last_idx => {
                    match amount {
                        MoveAmount::Extreme => self.move_focus_to(last_idx),
                        MoveAmount::Integer(amount) => self.move_focus(*amount),
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
