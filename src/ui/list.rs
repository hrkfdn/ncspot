//! This module offers an abstract list to represent items in a Cursive TUI. The
//! items of the list need to implement [NewListItem].

use std::{
    cmp,
    sync::{Arc, RwLock},
};

use cursive::{
    event::{Event, EventResult, MouseButton, MouseEvent},
    theme::{self, ColorStyle},
    Rect, View,
};

use crate::{
    command::{Command, JumpMode, MoveAmount, MoveMode},
    commands::CommandResult,
    traits::ViewExt,
};

use super::mouse_coordinates_to_view;

pub mod album;
pub mod artist;
pub mod category;
pub mod episode;
pub mod playable;
pub mod playlist;
pub mod show;
pub mod track;

// static lifetime needed for AsRef and AsMut impls.
pub trait ListItem: ViewExt {
    /// In order to filter ListItems, a List must be able to check if any of its
    /// items contain a given string.
    fn contains(&self, text: &str) -> bool;
}

// PERF: Now we keep the raw data in a shared pointer. Maybe there is a way
// to keep something that implements NewListItem but also updates with
// changes to the raw data, so we don't need to create the Box<dyn
// NewListItem> whenever we need it. It's probably expensive to create the
// vtable everytime, for example for a draw.

/// A function that receives an item and returns a ColorStyle based on it.
type ColorstyleCallback<T> = Option<Box<dyn Fn(&T) -> Option<ColorStyle>>>;

/// An abstract list that can display any view. It keeps its contents as shared
/// reference to the actual content. This way, when the content is updated, the
/// list will also show the updated content on the next draw. bla bla bal bal
/// qmlkjf qmlkdsfj
pub struct List<T> {
    /// The collection that is represented by this list.
    pub items: Arc<RwLock<Vec<T>>>,
    /// The currently selected item in the list.
    selected: usize,
    /// If a search is active, this is Some(Vec<usize>), so the user can move
    /// between the results.
    /// If no search is active, this is None.
    search: Option<Vec<usize>>,
    /// A function that is called for every draw of a child item, to get an
    /// optionally different colorstyle. Can be used for example to highlight
    /// the currently playing item in a list.
    colorstyle: ColorstyleCallback<T>,
}

impl<T> List<T>
where
    T: Clone + Into<Box<dyn ListItem>>,
{
    pub fn new(items: Arc<RwLock<Vec<T>>>) -> Self {
        Self::from(items)
    }

    /// Get the index of the selected item.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Set the callback to use for getting a colorstyle before drawing items.
    pub fn set_colorstyle_callback(&mut self, colorstyle: ColorstyleCallback<T>) {
        self.colorstyle = colorstyle;
    }

    /// Remove the element at `index` from the list.
    ///
    /// # Panics
    /// Panics if `index` is out of bounds.
    pub fn remove(&mut self, index: usize) {
        if self.selected + 1 == self.items.read().unwrap().len() && self.selected > 0 {
            self.selected -= 1;
        }
        self.items.write().unwrap().remove(index);
    }

    /// Move the focus by the given `amount`. If the resulting focus is outside
    /// of the items bounds, it will saturate at the bounds.
    pub fn move_focus(&mut self, amount: i32) {
        if amount > 0 {
            let new_selected = self.selected.saturating_add(amount as usize);
            if new_selected > self.items.read().unwrap().len() {
                self.selected = self.items.read().unwrap().len().saturating_sub(1);
            } else {
                self.selected = new_selected;
            }
        } else {
            self.selected = self.selected.saturating_sub(-amount as usize);
        }
    }

    /// Move the focus to the item at `index`. If the index is bigger than the
    /// amount of items, the last item will be selected instead.
    pub fn move_focus_to(&mut self, index: usize) {
        if index + 1 > self.items.read().unwrap().len() {
            self.selected = self.items.read().unwrap().len().saturating_sub(1);
        } else {
            self.selected = index;
        }
    }

    /// Search for all the items that match `query` in the list, and go to the
    /// first one.
    fn search_and_go(&mut self, query: &str) {
        let mut results = Vec::new();
        for (index, item) in self.items.read().unwrap().iter().enumerate() {
            if item.clone().into().contains(query) {
                results.push(index);
            }
        }
        if !results.is_empty() {
            self.selected = results[0];
            self.search = Some(results);
        } else {
            self.search = None;
        }
    }

    /// Move the selection to the previous search result.
    fn previous_search_result(&mut self) {
        if let Some(ref search_indices) = self.search {
            if self.selected
                > *search_indices
                    .first()
                    .unwrap_or(&self.items.read().unwrap().len())
            {
                let back: Vec<&usize> = search_indices
                    .iter()
                    .filter(|x| **x < self.selected)
                    .collect();
                self.selected = **back.last().unwrap();
            }
        }
    }

    /// Move the selection to the next search result.
    fn next_search_result(&mut self) {
        if let Some(ref search_indices) = self.search {
            if self.selected < *search_indices.last().unwrap_or(&0) {
                let further: Vec<&usize> = search_indices
                    .iter()
                    .filter(|x| **x > self.selected)
                    .collect();
                self.selected = **further.first().unwrap();
            }
        }
    }
}

impl<T> From<Arc<RwLock<Vec<T>>>> for List<T> {
    fn from(data: Arc<RwLock<Vec<T>>>) -> Self {
        Self {
            items: data,
            selected: 0,
            search: None,
            colorstyle: None,
        }
    }
}

impl<T> View for List<T>
where
    T: Clone + Into<Box<dyn ListItem>> + 'static,
{
    fn draw(&self, printer: &cursive::Printer) {
        for (index, child) in self.items.read().unwrap()[printer.content_offset.y
            ..printer.content_offset.y
                + cmp::min(
                    printer.output_size.y,
                    self.items.read().unwrap().clone().len() - printer.content_offset.y,
                )]
            .iter()
            .enumerate()
        {
            let item_printer = printer.windowed(Rect::from_size(
                (0, printer.content_offset.y + index),
                (printer.size.x, 1),
            ));
            if index + printer.content_offset.y == self.selected {
                // Draw the currently selected item highlighted.
                item_printer.with_color(theme::ColorStyle::highlight(), |printer| {
                    // Draw the background color.
                    printer.print_hline((0, 0), printer.output_size.x, " ");
                    // Let the child item draw itself.
                    child.clone().into().draw(printer);
                });
            } else if let Some(ref search) = self.search {
                // If the search contains the index, highlight it, otherwise,
                // draw it normally.
                if search.contains(&index) {
                    item_printer.with_color(
                        ColorStyle::new(
                            *item_printer.theme.palette.custom("search_match").unwrap(),
                            ColorStyle::primary().back,
                        ),
                        |printer| {
                            child.clone().into().draw(printer);
                        },
                    )
                } else if let Some(colorstyle) = &self.colorstyle {
                    if let Some(colorstyle) = colorstyle(child) {
                        item_printer.with_color(colorstyle, |printer| {
                            child.clone().into().draw(printer);
                        })
                    } else {
                        child.clone().into().draw(&item_printer);
                    }
                } else {
                    child.clone().into().draw(&item_printer);
                }
            } else {
                // Draw the item as normal.
                if let Some(colorstyle) = &self.colorstyle {
                    if let Some(colorstyle) = colorstyle(child) {
                        item_printer.with_color(colorstyle, |printer| {
                            child.clone().into().draw(printer);
                        })
                    } else {
                        child.clone().into().draw(&item_printer);
                    }
                } else {
                    child.clone().into().draw(&item_printer);
                }
            }
        }
    }

    fn layout(&mut self, _: cursive::Vec2) {}

    fn needs_relayout(&self) -> bool {
        true
    }

    fn required_size(&mut self, constraint: cursive::Vec2) -> cursive::Vec2 {
        let _ = constraint;
        cursive::Vec2::new(1, self.items.read().unwrap().clone().len())
    }

    fn on_event(&mut self, event: cursive::event::Event) -> cursive::event::EventResult {
        match event {
            Event::Mouse {
                offset,
                position,
                event: mouse_event,
            } => {
                if let MouseEvent::Press(MouseButton::Left) = mouse_event {
                    let relative_mouse_coordinates = mouse_coordinates_to_view(position, offset);
                    self.move_focus_to(relative_mouse_coordinates.y);
                    let child = self
                        .items
                        .write()
                        .unwrap()
                        .get_mut(relative_mouse_coordinates.y)
                        .cloned();
                    if let Some(child) = child {
                        child.into().on_event(event)
                    } else {
                        EventResult::Ignored
                    }
                } else {
                    EventResult::Ignored
                }
            }
            Event::Char(char) => match char {
                // Very important to handle the events here, and not in
                // ViewExt::on_command! Otherwise View::important_area doesn't
                // work!
                'k' => {
                    self.selected = self.selected.saturating_sub(1);
                    EventResult::Consumed(None)
                }
                'j' => {
                    if self.items.read().unwrap().clone().len() > self.selected + 1 {
                        self.selected = self.selected.saturating_add(1);
                    }
                    EventResult::Consumed(None)
                }
                _ => {
                    if !self.items.read().unwrap().clone().len() == 0 {
                        self.items.read().unwrap()[self.selected]
                            .clone()
                            .into()
                            .on_event(event)
                    } else {
                        EventResult::Ignored
                    }
                }
            },
            _ => {
                if !self.items.read().unwrap().is_empty() {
                    self.items.read().unwrap()[self.selected]
                        .clone()
                        .into()
                        .on_event(event)
                } else {
                    EventResult::Ignored
                }
            }
        }
    }

    fn important_area(&self, view_size: cursive::Vec2) -> Rect {
        Rect::from_point((view_size.x, self.selected))
    }
}

impl<T> ViewExt for List<T>
where
    T: Clone + Into<Box<dyn ListItem>> + 'static,
{
    fn on_command(
        &mut self,
        s: &mut cursive::Cursive,
        cmd: &Command,
    ) -> Result<CommandResult, String> {
        match cmd {
            // crate::command::Command::QueueAll => {
            //     for (index, item) in self.items.read().unwrap()[self.selected..]
            //         .iter()
            //         .enumerate()
            //     {
            //         // Skip the currently selected song.
            //         if index > 0 {
            //             item.as_ref().on_command(s, &Command::Queue).unwrap();
            //         }
            //     }
            //     Ok(CommandResult::Consumed(None))
            // }
            Command::Jump(mode) => {
                match mode {
                    JumpMode::Previous => self.previous_search_result(),
                    JumpMode::Next => self.next_search_result(),
                    JumpMode::Query(query) => self.search_and_go(query),
                }
                Ok(CommandResult::Consumed(None))
            }
            Command::Move(mode, amount) => match mode {
                MoveMode::Up => match amount {
                    MoveAmount::Integer(amount) => {
                        self.selected = self.selected.saturating_sub(*amount as usize);
                        Ok(CommandResult::Consumed(None))
                    }
                    _ => self.items.read().unwrap()[self.selected]
                        .clone()
                        .into()
                        .on_command(s, cmd),
                },
                MoveMode::Down => match amount {
                    MoveAmount::Integer(amount) => {
                        self.selected = self.selected.saturating_add(*amount as usize);
                        Ok(CommandResult::Consumed(None))
                    }
                    _ => self.items.read().unwrap()[self.selected]
                        .clone()
                        .into()
                        .on_command(s, cmd),
                },
                _ => Ok(CommandResult::Ignored),
            },
            _ => {
                let items = self.items.read().unwrap();
                if !items.is_empty() {
                    self.items.read().unwrap()[self.selected]
                        .clone()
                        .into()
                        .on_command(s, cmd)
                } else {
                    Ok(CommandResult::Ignored)
                }
            }
        }
    }
}
