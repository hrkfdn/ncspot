use std::cmp::min;

use cursive::{
    Cursive, Printer, Vec2, View,
    align::HAlign,
    event::{Event, EventResult, MouseButton, MouseEvent},
    theme::ColorStyle,
    view::Nameable,
    views::NamedView,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    command::{Command, MoveAmount, MoveMode},
    commands::CommandResult,
    traits::{BoxedViewExt, IntoBoxedViewExt, ViewExt},
};

/// A view that displays other views in a tab layout.
#[derive(Default)]
pub struct TabbedView {
    /// The list of tabs
    tabs: Vec<NamedView<BoxedViewExt>>,
    /// The index of the currently visible tab from `tabs`
    selected: usize,
    /// The size given to the last call to `layout()`
    last_layout_size: Vec2,
}

impl TabbedView {
    pub fn new() -> Self {
        Default::default()
    }

    /// Add `view` as a new tab to the end of this [TabbedView].
    pub fn add_tab(&mut self, title: impl Into<String>, view: impl IntoBoxedViewExt) {
        let tab = BoxedViewExt::new(view.into_boxed_view_ext()).with_name(title);
        self.tabs.push(tab);
    }

    /// Return a mutable reference to the tab at `index`, or None if there is no tab at `index`.
    pub fn tab_mut(&mut self, index: usize) -> Option<&mut NamedView<BoxedViewExt>> {
        self.tabs.get_mut(index)
    }

    /// Return a mutable reference to the selected tab, or None if there is no selected tab
    /// currently.
    pub fn selected_tab_mut(&mut self) -> Option<&mut NamedView<BoxedViewExt>> {
        self.tab_mut(self.selected)
    }

    /// Return the amount of tabs in this view.
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// Check whether there are tabs in this [TabbedView].
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Set the tab at `index` as currently visible.
    pub fn set_selected(&mut self, index: usize) {
        self.selected = min(self.len().saturating_sub(1), index);
    }

    /// Move the focus by `amount`, clipping at the edges.
    pub fn move_selected(&mut self, amount: isize) {
        self.selected = min(
            self.selected.saturating_add_signed(amount),
            self.len().saturating_sub(1),
        );
    }

    pub fn move_left(&mut self) {
        self.move_selected(-1);
    }

    pub fn move_right(&mut self) {
        self.move_selected(1);
    }

    /// Move the focus to the first tab.
    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    /// Move the focus to the last tab.
    pub fn select_last(&mut self) {
        self.selected = self.len() - 1;
    }

    /// Return whether we are on the first tab.
    pub fn on_first_tab(&mut self) -> bool {
        self.selected == 0
    }

    /// Return whether we are on the last tab.
    pub fn on_last_tab(&mut self) -> bool {
        self.selected == self.len() - 1
    }

    /// Return the width of a single tab.
    ///
    /// Keep in mind that this is an average. It's only provided to make sure all functions use the
    /// same calculation for tab width to prevent off-by-one errors.
    pub fn tab_width(&self) -> usize {
        self.last_layout_size.x / self.len()
    }
}

impl View for TabbedView {
    fn draw(&self, printer: &Printer<'_, '_>) {
        if self.is_empty() {
            return;
        }

        let tabwidth = self.tab_width();
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

            let title = tab.name();
            let offset = HAlign::Center.get_offset(title.width(), width);

            printer.with_color(style, |printer| {
                printer.print_hline((i * tabwidth, 0), width, " ");
                printer.print((i * tabwidth + offset, 0), title);
            });
        }

        if let Some(tab) = self.tabs.get(self.selected) {
            let printer = printer
                .offset((0, 1))
                .cropped((printer.size.x, printer.size.y - 1));

            tab.draw(&printer);
        }
    }

    fn layout(&mut self, size: Vec2) {
        self.last_layout_size = size;
        if let Some(tab) = self.tab_mut(self.selected) {
            tab.layout((size.x, size.y - 1).into())
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
                    MouseEvent::WheelUp => self.move_left(),
                    MouseEvent::WheelDown => self.move_right(),
                    MouseEvent::Press(MouseButton::Left) => {
                        let tabwidth = self.tab_width();
                        if let Some(selected_tab) = position.and_then(|p| p.x.checked_div(tabwidth))
                        {
                            self.set_selected(selected_tab);
                        }
                    }
                    _ => {}
                };
                return EventResult::consumed();
            }
        }

        if let Some(tab) = self.tab_mut(self.selected) {
            tab.on_event(event.relativized((0, 1)))
        } else {
            EventResult::Ignored
        }
    }
}

impl ViewExt for TabbedView {
    fn on_command(&mut self, s: &mut Cursive, cmd: &Command) -> Result<CommandResult, String> {
        match cmd {
            Command::Move(mode, amount) if matches!(mode, MoveMode::Left | MoveMode::Right) => {
                if matches!(mode, MoveMode::Left) && !self.on_first_tab() {
                    match amount {
                        MoveAmount::Extreme => self.select_first(),
                        MoveAmount::Integer(amount) => self.move_selected(-(*amount) as isize),
                        _ => (),
                    }
                } else if matches!(mode, MoveMode::Right) && !self.on_last_tab() {
                    match amount {
                        MoveAmount::Extreme => self.select_last(),
                        MoveAmount::Integer(amount) => self.move_selected(*amount as isize),
                        _ => (),
                    }
                }
                Ok(CommandResult::Consumed(None))
            }
            _ => {
                if let Some(tab) = self.selected_tab_mut() {
                    tab.on_command(s, cmd)
                } else {
                    Ok(CommandResult::Ignored)
                }
            }
        }
    }
}
