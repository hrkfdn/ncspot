use std::cmp::{max, min};
use std::sync::{Arc, RwLock};

use cursive::align::HAlign;
use cursive::event::{Event, EventResult, MouseButton, MouseEvent};
use cursive::theme::{ColorStyle, ColorType, PaletteColor};
use cursive::traits::View;
use cursive::view::ScrollBase;
use cursive::{Cursive, Printer, Rect, Vec2};
use unicode_width::UnicodeWidthStr;

use commands::CommandResult;
use queue::Queue;
use traits::{ListItem, ViewExt};

pub struct ListView<I: 'static + ListItem> {
    content: Arc<RwLock<Vec<I>>>,
    last_content_len: usize,
    selected: usize,
    last_size: Vec2,
    scrollbar: ScrollBase,
    queue: Arc<Queue>,
}

impl<I: ListItem> ListView<I> {
    pub fn new(content: Arc<RwLock<Vec<I>>>, queue: Arc<Queue>) -> Self {
        Self {
            content,
            last_content_len: 0,
            selected: 0,
            last_size: Vec2::new(0, 0),
            scrollbar: ScrollBase::new(),
            queue,
        }
    }

    pub fn get_selected_index(&self) -> usize {
        self.selected
    }

    pub fn move_focus_to(&mut self, target: usize) {
        let len = self.content.read().unwrap().len().saturating_sub(1);
        self.selected = min(target, len);
        self.scrollbar.scroll_to(self.selected);
    }

    pub fn move_focus(&mut self, delta: i32) {
        let new = self.selected as i32 + delta;
        self.move_focus_to(max(new, 0) as usize);
    }
}

impl<I: ListItem> View for ListView<I> {
    fn draw(&self, printer: &Printer<'_, '_>) {
        let content = self.content.read().unwrap();

        self.scrollbar.draw(printer, |printer, i| {
            let item = &content[i];

            let style = if self.selected == i {
                let fg = if item.is_playing(self.queue.clone()) {
                    *printer.theme.palette.custom("playing").unwrap()
                } else {
                    PaletteColor::Tertiary.resolve(&printer.theme.palette)
                };
                ColorStyle::new(
                    ColorType::Color(fg),
                    ColorType::Palette(PaletteColor::Highlight),
                )
            } else if item.is_playing(self.queue.clone()) {
                ColorStyle::new(
                    ColorType::Color(*printer.theme.palette.custom("playing").unwrap()),
                    ColorType::Color(*printer.theme.palette.custom("playing_bg").unwrap()),
                )
            } else {
                ColorStyle::primary()
            };

            let left = item.display_left();
            let right = item.display_right();

            // draw left string
            printer.with_color(style, |printer| {
                printer.print_hline((0, 0), printer.size.x, " ");
                printer.print((0, 0), &left);
            });

            // draw ".." to indicate a cut off string
            let max_length = printer.size.x.checked_sub(right.width() + 1).unwrap_or(0);
            if max_length < left.width() {
                let offset = max_length.checked_sub(1).unwrap_or(0);
                printer.with_color(style, |printer| {
                    printer.print((offset, 0), "..");
                });
            }

            // draw right string
            let offset = HAlign::Right.get_offset(right.width(), printer.size.x);

            printer.with_color(style, |printer| {
                printer.print((offset, 0), &right);
            });
        });
    }

    fn layout(&mut self, size: Vec2) {
        self.last_content_len = self.content.read().unwrap().len();
        self.last_size = size;
        self.scrollbar.set_heights(size.y, self.last_content_len);
    }

    fn needs_relayout(&self) -> bool {
        self.content.read().unwrap().len() != self.last_content_len
    }

    fn on_event(&mut self, e: Event) -> EventResult {
        match e {
            Event::Mouse {
                event: MouseEvent::WheelUp,
                ..
            } => self.move_focus(-3),
            Event::Mouse {
                event: MouseEvent::WheelDown,
                ..
            } => self.move_focus(3),
            Event::Mouse {
                event: MouseEvent::Press(MouseButton::Left),
                position,
                offset,
            } => {
                if self.scrollbar.scrollable()
                    && position.y > 0
                    && position.y <= self.last_size.y
                    && position
                        .checked_sub(offset)
                        .map(|p| self.scrollbar.start_drag(p, self.last_size.x))
                        .unwrap_or(false)
                {}
            }
            Event::Mouse {
                event: MouseEvent::Hold(MouseButton::Left),
                position,
                offset,
            } => {
                if self.scrollbar.scrollable() {
                    self.scrollbar.drag(position.saturating_sub(offset));
                }
            }
            Event::Mouse {
                event: MouseEvent::Release(MouseButton::Left),
                ..
            } => {
                self.scrollbar.release_grab();
            }
            _ => {
                return EventResult::Ignored;
            }
        }

        EventResult::Consumed(None)
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2::new(constraint.x, self.content.read().unwrap().len())
    }

    fn important_area(&self, view_size: Vec2) -> Rect {
        if self.content.read().unwrap().len() > 0 {
            Rect::from((view_size.x, self.selected))
        } else {
            Rect::from((0, 0))
        }
    }
}

impl<I: ListItem> ViewExt for ListView<I> {
    fn on_command(
        &mut self,
        _s: &mut Cursive,
        cmd: &str,
        args: &[String],
    ) -> Result<CommandResult, String> {
        if cmd == "play" {
            let mut content = self.content.write().unwrap();
            if let Some(item) = content.get_mut(self.selected) {
                item.play(self.queue.clone());
            }
            return Ok(CommandResult::Consumed(None));
        }

        if cmd == "queue" {
            let mut content = self.content.write().unwrap();
            if let Some(item) = content.get_mut(self.selected) {
                item.queue(self.queue.clone());
            }
            return Ok(CommandResult::Consumed(None));
        }

        if cmd == "move" {
            if let Some(dir) = args.get(0) {
                let amount: i32 = args
                    .get(1)
                    .unwrap_or(&"1".to_string())
                    .parse()
                    .map_err(|e| format!("{:?}", e))?;

                let len = self.content.read().unwrap().len();

                if dir == "up" && self.selected > 0 {
                    self.move_focus(-amount);
                    return Ok(CommandResult::Consumed(None));
                }

                if dir == "down" && self.selected < len.saturating_sub(1) {
                    self.move_focus(amount);
                    return Ok(CommandResult::Consumed(None));
                }
            }
        }

        Ok(CommandResult::Ignored)
    }
}
